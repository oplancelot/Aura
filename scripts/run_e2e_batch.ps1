# LJSpeech E2E 管线测试
# 编译后遍历 WAV 文件运行 e2e_transcribe_wav
# Usage:
#   .\scripts\run_e2e_batch.ps1 [-MaxFiles N] [-Realtime] [-Suite Accuracy|Latency]
# 输出: e2e_batch_results.csv + e2e_batch_summary.json + terminal summary

param(
    [int]$MaxFiles = 0,
    [switch]$Realtime,
    [switch]$DisplayEval,
    [ValidateSet("Accuracy", "Latency")]
    [string]$Suite = "Accuracy",
    [int]$SilenceClose = 0,  # override silence_close_ms (0 = leave default)
    [int]$HardCut = 0,       # override hard_cut_ms (0 = leave default)
    [int]$Threads = 0        # override ASR thread count (0 = leave default 4)
)

if ($Suite -eq "Latency") { $Realtime = $true }
$modeName = if ($Realtime) { "realtime" } else { "accuracy" }

function Get-Percentile {
    param(
        [double[]]$Values,
        [double]$Percentile
    )
    if ($null -eq $Values -or $Values.Count -eq 0) { return $null }
    $sorted = $Values | Sort-Object
    $n = $sorted.Count
    if ($n -eq 1) { return [double]$sorted[0] }
    $rank = ($Percentile / 100.0) * ($n - 1)
    $lo = [int][math]::Floor($rank)
    $hi = [int][math]::Ceiling($rank)
    if ($lo -eq $hi) { return [double]$sorted[$lo] }
    $w = $rank - $lo
    return [double]($sorted[$lo] * (1.0 - $w) + $sorted[$hi] * $w)
}

$wavDir = "OpenSLR/LJSpeech/wavs"
$example = "core\target\release\examples\e2e_transcribe_wav.exe"
$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$csvOut = "e2e_batch_results_${modeName}_${timestamp}.csv"
$jsonOut = "e2e_batch_summary_${modeName}_${timestamp}.json"

# Protocol metadata (must match e2e_transcribe_wav + ChunkingConfig::default)
$chunkingConfig = [ordered]@{
    silence_close_ms        = 200
    provisional_start_ms    = 1000
    provisional_interval_ms = 200
    hard_cut_ms             = 5000
    hard_cut_overlap_ms     = 2000
}
$models = [ordered]@{
    vad = "assets/silero_vad.onnx"
    asr = "assets/sense-voice-small-q4_k.gguf"
}

$gitCommit = (git rev-parse HEAD 2>$null)
if (-not $gitCommit) { $gitCommit = "unknown" }
$gitDirty = (git status --porcelain 2>$null)
$gitDirtyFlag = if ($gitDirty) { $true } else { $false }
$startedAt = (Get-Date).ToUniversalTime().ToString("o")
$machine = $env:COMPUTERNAME
if (-not $machine) { $machine = "unknown" }

# Build once
Write-Host "Building e2e_transcribe_wav..."
Write-Host "Suite: $Suite  |  Mode: $modeName  |  Commit: $($gitCommit.Substring(0, [Math]::Min(12, $gitCommit.Length)))"
Push-Location core
cargo build --release --example e2e_transcribe_wav 2>&1
Pop-Location

if (-not (Test-Path $example)) {
    Write-Host "ERROR: e2e_transcribe_wav.exe not found at $example"
    exit 1
}

$wavs = @(Get-ChildItem "$wavDir/*.wav" | Sort-Object Name)
if ($MaxFiles -gt 0) {
    $wavs = $wavs | Select-Object -First $MaxFiles
}
$totalCount = $wavs.Count

$results = @()
$werList = [System.Collections.Generic.List[double]]::new()
$asrList = [System.Collections.Generic.List[double]]::new()
$totalWER = 0.0
$totalAsrMs = 0.0
$totalProcessTime = 0.0
$totalChunks = 0
$totalFinal = 0
$totalHardCut = 0
$totalProvisional = 0
$sumMinChunk = 0.0
$sumAvgChunk = 0.0
$sumMaxChunk = 0.0
$globalMinChunk = [double]::MaxValue
$globalMaxChunk = [double]::MinValue
$multiChunkFiles = 0
$flushFiles = 0
$asrErrorFiles = 0
$noRefCount = 0
    $werZero = 0
    $werUnder5 = 0
    $werOver20 = 0
    $endpointList = [System.Collections.Generic.List[double]]::new()
    $ttfpList = [System.Collections.Generic.List[double]]::new()
    $prefixMatchList = [System.Collections.Generic.List[double]]::new()
    $stabilityList = [System.Collections.Generic.List[double]]::new()
    $tested = 0

Write-Host "Testing $totalCount files...`n"

foreach ($wav in $wavs) {
    $name = $wav.BaseName
    Write-Progress -Activity "E2E Testing" -Status "$name ($tested/$totalCount)" -PercentComplete (($tested / $totalCount) * 100)

    $cmdline = @($wav.FullName)
    if ($Realtime) { $cmdline += "--realtime" }
    if ($DisplayEval) { $cmdline += "--display-eval" }
    if ($SilenceClose -gt 0) { $cmdline += "--silence-close"; $cmdline += "$SilenceClose" }
    if ($HardCut -gt 0) { $cmdline += "--hard-cut"; $cmdline += "$HardCut" }
    if ($Threads -gt 0) { $cmdline += "--threads"; $cmdline += "$Threads" }
    $output = & $example $cmdline 2>$null

    $wer = $null
    $asrMs = 0.0
    $procTime = 0.0
    $audioTime = 0.0
    $chunks = 0
    $final = 0
    $hardCut = 0
    $provisional = 0
    $minChunk = 0.0; $avgChunk = 0.0; $maxChunk = 0.0
    $flushThisFile = $false; $asrErrorsThisFile = 0
    $epP50 = $null; $epP90 = $null; $epP95 = $null
    $ttfp = $null
    $pmAvg = $null; $pmP50 = $null; $pmP90 = $null
    $stability = $null
    $parsedMode = $null

    # Parse summary lines
    # Example summary: "Audio: 6.0s | Processing: 0.9s | ASR: 720ms total | RTF: 0.15x"
    foreach ($line in $output) {
        if ($line -match "^Mode: (\w+)") { $parsedMode = $Matches[1] }
        elseif ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
        elseif ($line -match "^Audio: ([\d.]+)s\s*\|\s*Processing: ([\d.]+)s\s*\|\s*ASR: (\d+)ms") {
            $audioTime = [double]$Matches[1]
            $procTime = [double]$Matches[2]
            $asrMs = [double]$Matches[3]
        }
        elseif ($line -match "^Total chunks: (\d+).*Final: (\d+), HardCut: (\d+), Provisional: (\d+)") {
            $chunks = [int]$Matches[1]
            $final = [int]$Matches[2]
            $hardCut = [int]$Matches[3]
            $provisional = [int]$Matches[4]
        }
        elseif ($line -match "^Avg chunk: ([\d.]+)s.*Min: ([\d.]+)s.*Max: ([\d.]+)s") {
            $avgChunk = [double]$Matches[1]
            $minChunk = [double]$Matches[2]
            $maxChunk = [double]$Matches[3]
        }
        elseif ($line -match "^Flush: (\w+).*ASR errors: (\d+)") {
            $flushThisFile = ($Matches[1] -eq "yes")
            $asrErrorsThisFile = [int]$Matches[2]
        }
        elseif ($line -match "^Endpoint latency \(Final\): p50=([\d.]+)ms.*p90=([\d.]+)ms.*p95=([\d.]+)ms") {
            $epP50 = [double]$Matches[1]
            $epP90 = [double]$Matches[2]
            $epP95 = [double]$Matches[3]
        }
        elseif ($line -match "^TTFP: ([\d.]+)ms") {
            $ttfp = [double]$Matches[1]
        }
        elseif ($line -match "^Provisional ASR chunks: \d+  \|  Prefix match: p50=([\d.]+)%  p90=([\d.]+)%  avg=([\d.]+)%") {
            $pmP50 = [double]$Matches[1]
            $pmP90 = [double]$Matches[2]
            $pmAvg = [double]$Matches[3]
        }
        elseif ($line -match "^Text stability: ([\d.]+)%") {
            $stability = [double]$Matches[1]
        }
    }
    if ($parsedMode -and $parsedMode -ne $modeName) {
        Write-Host "  WARN: binary Mode=$parsedMode expected=$modeName" -ForegroundColor Yellow
    }

    Write-Host "[$($tested+1)/$totalCount] $name" -NoNewline
    if ($wer -ne $null) {
        Write-Host "  WER: ${wer}%  ASR: ${asrMs}ms"
        $results += [PSCustomObject]@{
            File = $name
            WER = $wer
            ASR_Time_ms = [math]::Round($asrMs, 0)
            Process_Time_s = [math]::Round($procTime, 2)
            Audio_Time_s = [math]::Round($audioTime, 1)
            Chunks = $chunks
            Final = $final
            HardCut = $hardCut
            Provisional = $provisional
            Min_Chunk_s = $minChunk
            Avg_Chunk_s = $avgChunk
            Max_Chunk_s = $maxChunk
            Flush = $flushThisFile
            ASR_Errors = $asrErrorsThisFile
            Endpoint_p50_ms = $epP50
            Endpoint_p90_ms = $epP90
            Endpoint_p95_ms = $epP95
            TTFP_ms = $ttfp
            PrefixMatch_avg = $pmAvg
            PrefixMatch_p50 = $pmP50
            PrefixMatch_p90 = $pmP90
            Stability_pct = $stability
        }
        $totalWER += $wer
        $totalAsrMs += $asrMs
        $totalProcessTime += $procTime
        $werList.Add($wer)
        $asrList.Add($asrMs)
        if ($wer -eq 0) { $werZero++ }
        if ($wer -lt 5) { $werUnder5++ }
        if ($wer -ge 20) { $werOver20++ }
        $totalChunks += $chunks
        $totalFinal += $final
        $totalHardCut += $hardCut
        $totalProvisional += $provisional
        $sumMinChunk += $minChunk
        $sumAvgChunk += $avgChunk
        $sumMaxChunk += $maxChunk
        if ($minChunk -gt 0 -and $minChunk -lt $globalMinChunk) { $globalMinChunk = $minChunk }
        if ($maxChunk -gt $globalMaxChunk) { $globalMaxChunk = $maxChunk }
        if ($chunks -gt 1) { $multiChunkFiles++ }
        if ($flushThisFile) { $flushFiles++ }
        if ($asrErrorsThisFile -gt 0) { $asrErrorFiles++ }
        if ($epP50 -ne $null) { $endpointList.Add($epP50) }
        if ($ttfp -ne $null) { $ttfpList.Add($ttfp) }
        if ($pmAvg -ne $null) { $prefixMatchList.Add($pmAvg) }
        if ($stability -ne $null) { $stabilityList.Add($stability) }
        $tested++
    } else {
        Write-Host "  (no reference)"
        $noRefCount++
    }
}

$finishedAt = (Get-Date).ToUniversalTime().ToString("o")
$avgWer = $null; $avgAsr = $null; $avgProc = $null
$werP50 = $null; $werP90 = $null; $werP95 = $null
$asrP50 = $null; $asrP90 = $null; $asrP95 = $null
$meanAvgChunk = $null; $meanMinChunk = $null; $meanMaxChunk = $null
$gMin = $null; $gMax = $null
$multiChunkPct = $null

Write-Host "`n=== E2E Batch Summary ==="
Write-Host "Files tested: $tested / $totalCount"
Write-Host "Suite: $Suite  |  Mode: $modeName"
if ($tested -gt 0) {
    $werArr = $werList.ToArray()
    $asrArr = $asrList.ToArray()
    $avgWer = [math]::Round($totalWER / $tested, 1)
    $avgAsr = [math]::Round($totalAsrMs / $tested, 0)
    $avgProc = [math]::Round($totalProcessTime / $tested, 2)
    $werP50 = [math]::Round((Get-Percentile -Values $werArr -Percentile 50), 1)
    $werP90 = [math]::Round((Get-Percentile -Values $werArr -Percentile 90), 1)
    $werP95 = [math]::Round((Get-Percentile -Values $werArr -Percentile 95), 1)
    $asrP50 = [math]::Round((Get-Percentile -Values $asrArr -Percentile 50), 0)
    $asrP90 = [math]::Round((Get-Percentile -Values $asrArr -Percentile 90), 0)
    $asrP95 = [math]::Round((Get-Percentile -Values $asrArr -Percentile 95), 0)
    $meanAvgChunk = [math]::Round($sumAvgChunk / $tested, 2)
    $meanMinChunk = [math]::Round($sumMinChunk / $tested, 2)
    $meanMaxChunk = [math]::Round($sumMaxChunk / $tested, 2)
    $gMin = if ($globalMinChunk -lt [double]::MaxValue) { [math]::Round($globalMinChunk, 2) } else { 0 }
    $gMax = if ($globalMaxChunk -gt [double]::MinValue) { [math]::Round($globalMaxChunk, 2) } else { 0 }
    $multiChunkPct = [math]::Round(100.0 * $multiChunkFiles / $tested, 0)

    Write-Host "Avg WER: ${avgWer}%  |  p50/p90/p95: ${werP50}% / ${werP90}% / ${werP95}%"
    Write-Host "Avg ASR: ${avgAsr}ms  |  p50/p90/p95: ${asrP50}ms / ${asrP90}ms / ${asrP95}ms"
    Write-Host "Avg Processing: ${avgProc}s"
    Write-Host "Total ASR time: $([math]::Round($totalAsrMs / 1000, 1))s"
    Write-Host "WER distribution: 0%=$werZero ($([math]::Round(100.0 * $werZero / $tested, 0))%)  |  <5%=$werUnder5 ($([math]::Round(100.0 * $werUnder5 / $tested, 0))%)  |  >=20%=$werOver20 ($([math]::Round(100.0 * $werOver20 / $tested, 0))%)"

    Write-Host "`n=== Segmentation Quality ($tested files) ==="
    Write-Host "Total chunks: $totalChunks  (Final: $totalFinal | HardCut: $totalHardCut | Provisional: $totalProvisional)"
    Write-Host "Files with >1 chunk: $multiChunkFiles ($multiChunkPct%)"
    Write-Host "Flush used: $flushFiles ($([math]::Round(100.0 * $flushFiles / $tested, 0))%)  |  ASR errors: $asrErrorFiles files"
    Write-Host "No reference found: $noRefCount"

    $pmArr = $prefixMatchList.ToArray()
    if ($pmArr.Count -gt 0) {
        $pmAvg = [math]::Round(($pmArr | Measure-Object -Average).Average, 0)
        $pmP50 = [math]::Round((Get-Percentile -Values $pmArr -Percentile 50), 0)
        $pmP90 = [math]::Round((Get-Percentile -Values $pmArr -Percentile 90), 0)
        Write-Host "Prefix match: avg=${pmAvg}%  p50/p90: ${pmP50}% / ${pmP90}%"
    }
    $stArr = $stabilityList.ToArray()
    if ($stArr.Count -gt 0) {
        $stAvg = [math]::Round(($stArr | Measure-Object -Average).Average, 0)
        Write-Host "Text stability: avg=${stAvg}%"
    }

    $epArr = $endpointList.ToArray()
    if ($epArr.Count -gt 0) {
        $epAvg = [math]::Round(($epArr | Measure-Object -Average).Average, 0)
        $epP50 = [math]::Round((Get-Percentile -Values $epArr -Percentile 50), 0)
        $epP90 = [math]::Round((Get-Percentile -Values $epArr -Percentile 90), 0)
        $epP95 = [math]::Round((Get-Percentile -Values $epArr -Percentile 95), 0)
        Write-Host "Endpoint latency (Final p50, per-file): avg=${epAvg}ms  p50/p90/p95: ${epP50}ms / ${epP90}ms / ${epP95}ms"
    }
    $ttfpArr = $ttfpList.ToArray()
    if ($ttfpArr.Count -gt 0) {
        $ttfpAvg = [math]::Round(($ttfpArr | Measure-Object -Average).Average, 0)
        $ttfpP50 = [math]::Round((Get-Percentile -Values $ttfpArr -Percentile 50), 0)
        $ttfpP90 = [math]::Round((Get-Percentile -Values $ttfpArr -Percentile 90), 0)
        $ttfpP95 = [math]::Round((Get-Percentile -Values $ttfpArr -Percentile 95), 0)
        Write-Host "TTFP: avg=${ttfpAvg}ms  p50/p90/p95: ${ttfpP50}ms / ${ttfpP90}ms / ${ttfpP95}ms"
    }

    Write-Host "Mean of per-file avg/min/max chunk: ${meanAvgChunk}s / ${meanMinChunk}s / ${meanMaxChunk}s"
    Write-Host "Global min/max chunk: ${gMin}s / ${gMax}s"
}

$results | Export-Csv $csvOut -NoTypeInformation

$summary = [ordered]@{
    protocol_version = "1.0"
    suite            = $Suite
    mode             = $modeName
    started_at_utc   = $startedAt
    finished_at_utc  = $finishedAt
    git_commit       = $gitCommit
    git_dirty        = $gitDirtyFlag
    machine          = $machine
    dataset          = [ordered]@{
        name       = "LJSpeech"
        wav_dir    = $wavDir
        max_files  = $MaxFiles
        total_wavs = $totalCount
        tested     = $tested
        selection  = if ($MaxFiles -gt 0) { "first_n_sorted_by_name" } else { "all_sorted_by_name" }
    }
    models           = $models
    chunking_config  = $chunkingConfig
    metrics          = [ordered]@{
        avg_wer_pct              = $avgWer
        wer_p50_pct              = $werP50
        wer_p90_pct              = $werP90
        wer_p95_pct              = $werP95
        wer_zero_count           = $werZero
        wer_under_5_count        = $werUnder5
        wer_over_20_count        = $werOver20
        avg_asr_ms               = $avgAsr
        asr_p50_ms               = $asrP50
        asr_p90_ms               = $asrP90
        asr_p95_ms               = $asrP95
        avg_processing_s         = $avgProc
        total_asr_s              = if ($tested -gt 0) { [math]::Round($totalAsrMs / 1000, 1) } else { $null }
        total_chunks             = $totalChunks
        final_chunks             = $totalFinal
        hardcut_chunks           = $totalHardCut
        provisional_chunks       = $totalProvisional
        multi_chunk_files        = $multiChunkFiles
        multi_chunk_pct          = $multiChunkPct
        flush_files              = $flushFiles
        flush_pct                = if ($tested -gt 0) { [math]::Round(100.0 * $flushFiles / $tested, 0) } else { $null }
        asr_error_files          = $asrErrorFiles
        no_ref_count             = $noRefCount
        prefix_match_avg_pct     = if ($pmArr.Count -gt 0) { $pmAvg } else { $null }
        prefix_match_p50_pct     = if ($pmArr.Count -gt 0) { $pmP50 } else { $null }
        prefix_match_p90_pct     = if ($pmArr.Count -gt 0) { $pmP90 } else { $null }
        text_stability_avg_pct   = if ($stArr.Count -gt 0) { $stAvg } else { $null }
        mean_avg_chunk_s         = $meanAvgChunk
        mean_min_chunk_s         = $meanMinChunk
        mean_max_chunk_s         = $meanMaxChunk
        endpoint_avg_ms          = if ($epArr.Count -gt 0) { [math]::Round(($epArr | Measure-Object -Average).Average, 0) } else { $null }
        endpoint_p50_ms          = if ($epArr.Count -gt 0) { $epP50 } else { $null }
        endpoint_p90_ms          = if ($epArr.Count -gt 0) { $epP90 } else { $null }
        endpoint_p95_ms          = if ($epArr.Count -gt 0) { $epP95 } else { $null }
        ttfp_avg_ms              = if ($ttfpArr.Count -gt 0) { $ttfpAvg } else { $null }
        ttfp_p50_ms              = if ($ttfpArr.Count -gt 0) { $ttfpP50 } else { $null }
        ttfp_p90_ms              = if ($ttfpArr.Count -gt 0) { $ttfpP90 } else { $null }
        ttfp_p95_ms              = if ($ttfpArr.Count -gt 0) { $ttfpP95 } else { $null }
        global_min_chunk_s       = $gMin
        global_max_chunk_s       = $gMax
    }
    artifacts        = [ordered]@{
        results_csv = "e2e_batch_results_${modeName}_${timestamp}.csv"
        summary_json = "e2e_batch_summary_${modeName}_${timestamp}.json"
    }
}

$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonOut -Encoding utf8
Write-Host "`nResults saved to $csvOut"
Write-Host "Summary saved to $jsonOut"
