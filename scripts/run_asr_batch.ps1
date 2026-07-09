# LJSpeech offline ASR 测试
# 编译后遍历 WAV 文件运行 transcribe_wav
# Usage:
#   .\scripts\run_asr_batch.ps1 [-MaxFiles N]
# 输出: asr_batch_results.csv + asr_batch_summary.json + terminal summary

param(
    [int]$MaxFiles = 0
)

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
$example = "core\target\release\examples\transcribe_wav.exe"
$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$csvOut = "asr_batch_results_${timestamp}.csv"
$jsonOut = "asr_batch_summary_${timestamp}.json"

$models = [ordered]@{
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
Write-Host "Building transcribe_wav..."
Write-Host "Commit: $($gitCommit.Substring(0, [Math]::Min(12, $gitCommit.Length)))"
Push-Location core
cargo build --release --example transcribe_wav 2>&1
Pop-Location

if (-not (Test-Path $example)) {
    Write-Host "ERROR: transcribe_wav.exe not found at $example"
    exit 1
}

$wavs = @(Get-ChildItem "$wavDir/*.wav" | Sort-Object Name)
if ($MaxFiles -gt 0) {
    $wavs = $wavs | Select-Object -First $MaxFiles
}
$totalCount = $wavs.Count

$results = @()
$werList = [System.Collections.Generic.List[double]]::new()
$timeList = [System.Collections.Generic.List[double]]::new()
$totalWER = 0.0
$totalTime = 0.0
$werZero = 0
$werUnder5 = 0
$werOver20 = 0
$noRefCount = 0
$tested = 0

Write-Host "Testing $totalCount files...`n"

foreach ($wav in $wavs) {
    $name = $wav.BaseName
    Write-Progress -Activity "ASR Testing" -Status "$name ($tested/$totalCount)" -PercentComplete (($tested / $totalCount) * 100)

    $output = & $example $wav.FullName 2>$null

    # Capture hyp/ref lines after === markers
    $inHyp = $false; $inRef = $false
    $hyp = ""; $ref = ""
    $wer = $null
    $timeSec = 0.0

    foreach ($line in $output) {
        if ($line -match "^=== Full transcription ===") { $inHyp = $true; $inRef = $false; continue }
        if ($line -match "^=== Reference ===") { $inRef = $true; $inHyp = $false; continue }
        if ($inHyp -and $line.Trim() -ne "") { $hyp = $line.Trim() }
        if ($inRef -and $line.Trim() -ne "") { $ref = $line.Trim() }
        if ($inHyp -and $line -match "^WER:") { break }
    }
    if ($hyp -eq "" -or $ref -eq "") {
        # Fallback: try old-style parsing
        $hasRef = $false
        foreach ($line in $output) {
            if ($line -match "^=== Full transcription ===") { $hasRef = $true }
            elseif ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
            elseif ($line -match "^Audio: .+ Processing: ([\d.]+)s") { $timeSec = [double]$Matches[1] }
        }
        # Re-read hyp from lines between markers
        $inHyp = $false
        foreach ($line in $output) {
            if ($line -match "^=== Full transcription ===") { $inHyp = $true; continue }
            if ($line -match "^=== Reference ===") { $inHyp = $false; continue }
            if ($inHyp -and $line.Trim() -ne "") {
                if ($hyp -eq "") { $hyp = $line.Trim() }
            }
        }
    } else {
        # Fresh parse for WER and time
        foreach ($line in $output) {
            if ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
            elseif ($line -match "^Audio: .+ Processing: ([\d.]+)s") { $timeSec = [double]$Matches[1] }
        }
    }

    Write-Host "[$($tested+1)/$totalCount] $name" -NoNewline
    if ($wer -ne $null) {
        Write-Host "  WER: ${wer}%  time: ${timeSec}s"
        $results += [PSCustomObject]@{
            File = $name
            WER = $wer
            Time_s = [math]::Round($timeSec, 2)
        }
        $totalWER += $wer
        $totalTime += $timeSec
        $werList.Add($wer)
        $timeList.Add($timeSec)
        if ($wer -eq 0) { $werZero++ }
        if ($wer -lt 5) { $werUnder5++ }
        if ($wer -ge 20) { $werOver20++ }
        $tested++
    } else {
        Write-Host "  (no reference)"
        $noRefCount++
    }
}

$finishedAt = (Get-Date).ToUniversalTime().ToString("o")
$avgWer = $null; $avgTime = $null
$werP50 = $null; $werP90 = $null; $werP95 = $null
$timeP50 = $null; $timeP90 = $null; $timeP95 = $null

Write-Host "`n=== ASR Batch Summary ==="
Write-Host "Files tested: $tested / $totalCount"
if ($tested -gt 0) {
    $werArr = $werList.ToArray()
    $timeArr = $timeList.ToArray()
    $avgWer = [math]::Round($totalWER / $tested, 1)
    $avgTime = [math]::Round($totalTime / $tested, 2)
    $werP50 = [math]::Round((Get-Percentile -Values $werArr -Percentile 50), 1)
    $werP90 = [math]::Round((Get-Percentile -Values $werArr -Percentile 90), 1)
    $werP95 = [math]::Round((Get-Percentile -Values $werArr -Percentile 95), 1)
    $timeP50 = [math]::Round((Get-Percentile -Values $timeArr -Percentile 50), 2)
    $timeP90 = [math]::Round((Get-Percentile -Values $timeArr -Percentile 90), 2)
    $timeP95 = [math]::Round((Get-Percentile -Values $timeArr -Percentile 95), 2)

    Write-Host "Avg WER: ${avgWer}%  |  p50/p90/p95: ${werP50}% / ${werP90}% / ${werP95}%"
    Write-Host "Avg time: ${avgTime}s  |  p50/p90/p95: ${timeP50}s / ${timeP90}s / ${timeP95}s"
    Write-Host "Total ASR time: $([math]::Round($totalTime, 1))s"
    Write-Host "WER distribution: 0%=$werZero ($([math]::Round(100.0 * $werZero / $tested, 0))%)  |  <5%=$werUnder5 ($([math]::Round(100.0 * $werUnder5 / $tested, 0))%)  |  >=20%=$werOver20 ($([math]::Round(100.0 * $werOver20 / $tested, 0))%)"
    Write-Host "No reference found: $noRefCount"
}

$results | Export-Csv $csvOut -NoTypeInformation

$summary = [ordered]@{
    protocol_version = "1.0"
    suite            = "offline-asr"
    mode             = "30s-chunk-2s-overlap"
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
    metrics          = [ordered]@{
        avg_wer_pct              = $avgWer
        wer_p50_pct              = $werP50
        wer_p90_pct              = $werP90
        wer_p95_pct              = $werP95
        wer_zero_count           = $werZero
        wer_under_5_count        = $werUnder5
        wer_over_20_count        = $werOver20
        avg_time_s               = $avgTime
        time_p50_s               = $timeP50
        time_p90_s               = $timeP90
        time_p95_s               = $timeP95
        total_time_s             = if ($tested -gt 0) { [math]::Round($totalTime, 1) } else { $null }
        no_ref_count             = $noRefCount
    }
    artifacts        = [ordered]@{
        results_csv = $csvOut
        summary_json = $jsonOut
    }
}

$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonOut -Encoding utf8
Write-Host "`nResults saved to $csvOut"
Write-Host "Summary saved to $jsonOut"
