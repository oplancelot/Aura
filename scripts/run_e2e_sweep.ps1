# ChunkingConfig 参数扫瞄
# 枚举 silence_close_ms × hard_cut_ms 组合，读取 JSON summary 做对比
# Usage:
#   .\scripts\run_e2e_sweep.ps1 [-MaxFiles 10] [-Suite Accuracy|Latency]

param(
    [int]$MaxFiles = 10,
    [ValidateSet("Accuracy", "Latency")]
    [string]$Suite = "Accuracy"
)

$silenceCloseValues = @(100, 200, 400)
$hardCutValues = @(3000, 5000, 7000)

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$results = @()
$gitCommit = (git rev-parse HEAD 2>$null)
if (-not $gitCommit) { $gitCommit = "unknown" }

Write-Host "--- ChunkingConfig Sweep ---"
Write-Host "Suite: $Suite  |  MaxFiles: $MaxFiles"
Write-Host "silence_close: $($silenceCloseValues -join ', ')"
Write-Host "hard_cut:      $($hardCutValues -join ', ')"

# Save existing summary files list so we can detect new ones
$before = @(Get-ChildItem "*e2e_batch_summary_*" | ForEach-Object { $_.FullName })

foreach ($sc in $silenceCloseValues) {
    foreach ($hc in $hardCutValues) {
        $label = "sc${sc}_hc${hc}"
        Write-Host "`n--- Sweeping: silence_close=${sc}ms  hard_cut=${hc}ms ---"

        $realtimeSwitch = if ($Suite -eq "Latency") { @("-Realtime") } else { @() }
        $null = & ".\scripts\run_e2e_batch.ps1" -MaxFiles $MaxFiles -Suite $Suite -SilenceClose $sc -HardCut $hc @realtimeSwitch 2>&1

        # Find the latest summary JSON
        $after = @(Get-ChildItem "*e2e_batch_summary_*" | ForEach-Object { $_.FullName })
        $newFiles = $after | Where-Object { $_ -notin $before }
        $latestJson = $newFiles | Sort-Object -Descending | Select-Object -First 1
        if (-not $latestJson) { $latestJson = $after | Sort-Object LastWriteTime -Descending | Select-Object -First 1 }
        $before = $after

        if (-not $latestJson -or -not (Test-Path $latestJson)) {
            Write-Host "  WARN: no summary JSON found for $label"
            continue
        }

        $meta = Get-Content $latestJson | ConvertFrom-Json
        $m = $meta.metrics

        $results += [PSCustomObject]@{
            Config = $label
            SilenceClose_ms = $sc
            HardCut_ms = $hc
            FilesTested = $meta.dataset.tested
            AvgWER = $m.avg_wer_pct
            WerP50 = $m.wer_p50_pct
            WerP90 = $m.wer_p90_pct
            WerP95 = $m.wer_p95_pct
            WerZero = $m.wer_zero_count
            AvgASR_ms = $m.avg_asr_ms
            AsrP50_ms = $m.asr_p50_ms
            AsrP90_ms = $m.asr_p90_ms
            AvgProc_s = $m.avg_processing_s
            TotalChunks = $m.total_chunks
            MultiChunkPct = $m.multi_chunk_pct
            FlushPct = $m.flush_pct
            NoRef = $m.no_ref_count
            EndpointAvg_ms = $m.endpoint_avg_ms
            EndpointP50_ms = $m.endpoint_p50_ms
            EndpointP90_ms = $m.endpoint_p90_ms
            TtfpAvg_ms = $m.ttfp_avg_ms
        }
    }
}

Write-Host "`n`n========================================"
Write-Host "   Sweep Results"
Write-Host "========================================"
Write-Host ""

# Table header
Write-Host ("{0,-14} {1,8} {2,8} {3,8} {4,8} {5,8} {6,10} {7,10} {8,10}" -f "Config", "WER_avg", "WER_p50", "ASR_ms", "Ep_avg", "Mchunk%", "Flush%", "Chunks", "n")
Write-Host ("-" * 96)

foreach ($r in $results) {
    $ep = if ($r.EndpointAvg_ms -ne $null) { [math]::Round($r.EndpointAvg_ms, 0) } else { "-" }
    Write-Host ("{0,-14} {1,7}% {2,7}% {3,7} {4,7} {5,7}% {6,8}% {7,8} {8,8}" -f $r.Config,
        [math]::Round($r.AvgWER, 1),
        [math]::Round($r.WerP50, 1),
        [math]::Round($r.AvgASR_ms, 0),
        $ep,
        [math]::Round($r.MultiChunkPct, 0),
        [math]::Round($r.FlushPct, 0),
        $r.TotalChunks,
        $r.FilesTested)
}

$csvOut = "sweep_comparison_${timestamp}.csv"
$jsonOut = "sweep_comparison_${timestamp}.json"

$results | Export-Csv $csvOut -NoTypeInformation

$summary = [ordered]@{
    protocol_version = "1.0"
    suite            = $Suite
    max_files_per_config = $MaxFiles
    git_commit       = $gitCommit
    silence_close_values = $silenceCloseValues
    hard_cut_values      = $hardCutValues
    configs          = @($results | ForEach-Object {
        [ordered]@{
            config            = $_.Config
            silence_close_ms  = $_.SilenceClose_ms
            hard_cut_ms       = $_.HardCut_ms
            files_tested      = $_.FilesTested
            avg_wer_pct       = $_.AvgWER
            wer_p50_pct       = $_.WerP50
            wer_p90_pct       = $_.WerP90
            avg_asr_ms        = $_.AvgASR_ms
            asr_p50_ms        = $_.AsrP50_ms
            asr_p90_ms        = $_.AsrP90_ms
            avg_proc_s        = $_.AvgProc_s
            total_chunks      = $_.TotalChunks
            multi_chunk_pct   = $_.MultiChunkPct
            flush_pct         = $_.FlushPct
            no_ref            = $_.NoRef
            endpoint_avg_ms   = $_.EndpointAvg_ms
            endpoint_p50_ms   = $_.EndpointP50_ms
            endpoint_p90_ms   = $_.EndpointP90_ms
            ttfp_avg_ms       = $_.TtfpAvg_ms
        }
    })
}

$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonOut -Encoding utf8
Write-Host "`nResults saved to $csvOut"
Write-Host "Summary saved to $jsonOut"
