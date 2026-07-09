# ASR 线程数扫瞄
# 枚举不同 n_threads 对比 WER 和 ASR 延迟
# Usage:
#   .\scripts\run_thread_sweep.ps1 [-MaxFiles 10] [-Suite Accuracy|Latency]

param(
    [int]$MaxFiles = 10,
    [ValidateSet("Accuracy", "Latency")]
    [string]$Suite = "Accuracy"
)

$threadValues = @(1, 2, 4, 8)

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$results = @()
$gitCommit = (git rev-parse HEAD 2>$null)
if (-not $gitCommit) { $gitCommit = "unknown" }

Write-Host "--- ASR Thread Sweep ---"
Write-Host "Suite: $Suite  |  MaxFiles: $MaxFiles"
Write-Host "threads: $($threadValues -join ', ')"

$before = @(Get-ChildItem "*e2e_batch_summary_*" | ForEach-Object { $_.FullName })

foreach ($t in $threadValues) {
    $label = "t${t}"
    Write-Host "`n--- Sweeping: threads=${t} ---"

    $realtimeSwitch = if ($Suite -eq "Latency") { @("-Realtime") } else { @() }
    $null = & ".\scripts\run_e2e_batch.ps1" -MaxFiles $MaxFiles -Suite $Suite -Threads $t @realtimeSwitch 2>&1

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
        Threads = $t
        FilesTested = $meta.dataset.tested
        AvgWER = $m.avg_wer_pct
        WerP50 = $m.wer_p50_pct
        WerP90 = $m.wer_p90_pct
        AvgASR_ms = $m.avg_asr_ms
        AsrP50_ms = $m.asr_p50_ms
        AsrP90_ms = $m.asr_p90_ms
        AvgProc_s = $m.avg_processing_s
        EndpointAvg_ms = $m.endpoint_avg_ms
        MultiChunkPct = $m.multi_chunk_pct
    }
}

Write-Host "`n`n========================================"
Write-Host "   Thread Sweep Results"
Write-Host "========================================"
Write-Host ""
Write-Host ("{0,-14} {1,8} {2,8} {3,8} {4,8} {5,8} {6,10}" -f "Threads", "WER_avg", "WER_p50", "ASR_avg", "ASR_p50", "Ep_avg", "Proc_s")
Write-Host ("-" * 74)

foreach ($r in $results) {
    $ep = if ($r.EndpointAvg_ms -ne $null) { [math]::Round($r.EndpointAvg_ms, 0) } else { "-" }
    Write-Host ("{0,-14} {1,7}% {2,7}% {3,7} {4,7} {5,7} {6,7}" -f $r.Config,
        [math]::Round($r.AvgWER, 1),
        [math]::Round($r.WerP50, 1),
        [math]::Round($r.AvgASR_ms, 0),
        [math]::Round($r.AsrP50_ms, 0),
        $ep,
        [math]::Round($r.AvgProc_s, 2))
}

$csvOut = "thread_sweep_${timestamp}.csv"
$jsonOut = "thread_sweep_${timestamp}.json"

$results | Export-Csv $csvOut -NoTypeInformation

$summary = [ordered]@{
    protocol_version = "1.0"
    suite            = $Suite
    max_files_per_config = $MaxFiles
    git_commit       = $gitCommit
    thread_values    = $threadValues
    configs          = @($results | ForEach-Object {
        [ordered]@{
            config          = $_.Config
            threads         = $_.Threads
            files_tested    = $_.FilesTested
            avg_wer_pct     = $_.AvgWER
            wer_p50_pct     = $_.WerP50
            wer_p90_pct     = $_.WerP90
            avg_asr_ms      = $_.AvgASR_ms
            asr_p50_ms      = $_.AsrP50_ms
            asr_p90_ms      = $_.AsrP90_ms
            avg_proc_s      = $_.AvgProc_s
            endpoint_avg_ms = $_.EndpointAvg_ms
            multi_chunk_pct = $_.MultiChunkPct
        }
    })
}

$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonOut -Encoding utf8
Write-Host "`nResults saved to $csvOut"
Write-Host "Summary saved to $jsonOut"
