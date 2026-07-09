# E2E vs offline ASR 基线对比
# Usage:
#   .\scripts\compare_e2e_baseline.ps1 -AsrSummary asr_batch_summary_*.json -E2eSummary e2e_batch_summary_*.json
#   .\scripts\compare_e2e_baseline.ps1  (auto-find latest pair)

param(
    [string]$AsrSummary,
    [string]$E2eSummary,
    [switch]$ShowDetail
)

function Get-Percentile {
    param([double[]]$Values, [double]$Percentile)
    if ($null -eq $Values -or $Values.Count -eq 0) { return $null }
    $sorted = $Values | Sort-Object; $n = $sorted.Count
    if ($n -eq 1) { return [double]$sorted[0] }
    $rank = ($Percentile / 100.0) * ($n - 1); $lo = [int][math]::Floor($rank); $hi = [int][math]::Ceiling($rank)
    if ($lo -eq $hi) { return [double]$sorted[$lo] }
    return [double]($sorted[$lo] * (1.0 - ($rank - $lo)) + $sorted[$hi] * ($rank - $lo))
}

# Auto-find latest pair
if (-not $AsrSummary -or -not $E2eSummary) {
    $asrJsons = @(Get-ChildItem "asr_batch_summary_*.json" | Sort-Object LastWriteTime -Descending)
    $e2eJsons = @(Get-ChildItem "e2e_batch_summary_*.json" | Sort-Object LastWriteTime -Descending)
    if ($asrJsons.Count -eq 0 -or $e2eJsons.Count -eq 0) {
        Write-Host "ERROR: Cannot find summary JSONs. Specify -AsrSummary and -E2eSummary."
        exit 1
    }
    $AsrSummary = $asrJsons[0].Name
    $E2eSummary = $e2eJsons[0].Name
}

Write-Host "Comparing:" -NoNewline
Write-Host " ASR: $AsrSummary  vs  E2E: $E2eSummary"

$asrMeta = Get-Content $AsrSummary | ConvertFrom-Json
$e2eMeta = Get-Content $E2eSummary | ConvertFrom-Json

$asrCsv = $asrMeta.artifacts.results_csv
$e2eCsv = $e2eMeta.artifacts.results_csv

if (-not (Test-Path $asrCsv)) { Write-Host "ERROR: $asrCsv not found"; exit 1 }
if (-not (Test-Path $e2eCsv)) { Write-Host "ERROR: $e2eCsv not found"; exit 1 }

$asrRows = @{}; Import-Csv $asrCsv | ForEach-Object { $asrRows[$_.File] = $_ }
$e2eRows = @{}; Import-Csv $e2eCsv | ForEach-Object { $e2eRows[$_.File] = $_ }

$deltaList = [System.Collections.Generic.List[double]]::new()
$results = @()

foreach ($file in $e2eRows.Keys | Sort-Object) {
    $e2e = $e2eRows[$file]
    $asr = $asrRows[$file]
    if (-not $asr) { continue }
    $e2eWER = [double]$e2e.WER
    $asrWER = [double]$asr.WER
    $delta = [math]::Round($e2eWER - $asrWER, 1)
    $deltaList.Add($delta)

    $obj = [PSCustomObject]@{
        File = $file
        Asr_WER = $asrWER
        E2e_WER = $e2eWER
        Delta_WER = $delta
    }
    $results += $obj

    if ($ShowDetail) {
        $sign = if ($delta -gt 0) { "+" } else { "" }
        Write-Host "  $file  ASR=$asrWER%  E2E=$e2eWER%  Δ=${sign}$delta"
    }
}

$timestamp = (Get-Date).ToUniversalTime().ToString("yyyyMMdd_HHmmss")
$csvOut = "baseline_comparison_${timestamp}.csv"

$results | Export-Csv $csvOut -NoTypeInformation

$n = $deltaList.Count
$dArr = $deltaList.ToArray()
$meanDelta = [math]::Round(($dArr | Measure-Object -Average).Average, 2)
$p50 = [math]::Round((Get-Percentile -Values $dArr -Percentile 50), 1)
$p90 = [math]::Round((Get-Percentile -Values $dArr -Percentile 90), 1)
$p95 = [math]::Round((Get-Percentile -Values $dArr -Percentile 95), 1)
$deltaGt0 = ($dArr | Where-Object { $_ -gt 0 }).Count
$deltaGt5 = ($dArr | Where-Object { $_ -gt 5 }).Count
$deltaGt10 = ($dArr | Where-Object { $_ -gt 10 }).Count
$deltaLtMinus5 = ($dArr | Where-Object { $_ -lt -5 }).Count

# L0 / L1 gate definitions
# L0 (sanity): no catastrophic ΔWER regressions
# L1 (quality): majority of files within ΔWER ±5
$l0Threshold = 10.0  # no file exceeds +10pp ΔWER
$l1Threshold = 5.0   # <20% of files exceed +5pp ΔWER
$l0Fail = $dArr | Where-Object { $_ -gt $l0Threshold }
$l1FailCount = ($dArr | Where-Object { $_ -gt $l1Threshold }).Count
$l1FailPct = [math]::Round(100.0 * $l1FailCount / $n, 1)
$l0Pass = $l0Fail.Count -eq 0
$l1Pass = $l1FailPct -lt 20.0

Write-Host "`n========================================"
Write-Host "   ΔWER 对比报告"
Write-Host "========================================"
Write-Host "Files matched: $n"
Write-Host ""
Write-Host "ΔWER = E2E_WER - ASR_WER"
Write-Host ""
Write-Host "Mean ΔWER: ${meanDelta}pp"
Write-Host "p50/p90/p95: ${p50}pp / ${p90}pp / ${p95}pp"
Write-Host ""
Write-Host "Distribution:"
Write-Host "  Δ >  +0pp: $deltaGt0 files ($([math]::Round(100.0*$deltaGt0/$n,0))%)"
Write-Host "  Δ >  +5pp: $deltaGt5 files ($([math]::Round(100.0*$deltaGt5/$n,0))%)"
Write-Host "  Δ > +10pp: $deltaGt10 files ($([math]::Round(100.0*$deltaGt10/$n,0))%)"
Write-Host "  Δ <  -5pp: $deltaLtMinus5 files ($([math]::Round(100.0*$deltaLtMinus5/$n,0))%)"
Write-Host ""
Write-Host "--- Gates ---"
Write-Host "L0 (no ΔWER > +10pp): $(if($l0Pass){'PASS'}else{'FAIL'})  (max Δ = $(if($l0Fail.Count -gt 0){[math]::Round(($l0Fail | Measure-Object -Maximum).Maximum,1)}else{'N/A'}))"
Write-Host "L1 (<20% files with Δ > +5pp): $(if($l1Pass){'PASS'}else{'FAIL'})  ($l1FailCount/$n = ${l1FailPct}%)"
Write-Host ""
if (-not $l0Pass) {
    Write-Host "Worst offenders (Δ > +10pp):" -ForegroundColor Yellow
    $results | Where-Object { $_.Delta_WER -gt $l0Threshold } | Sort-Object Delta_WER -Descending | ForEach-Object {
        Write-Host "  $($_.File): ASR=$($_.Asr_WER)%  E2E=$($_.E2e_WER)%  Δ=+$($_.Delta_WER)"
    }
}

$jsonOut = "baseline_comparison_${timestamp}.json"
$summary = [ordered]@{
    protocol_version = "1.0"
    asr_summary      = $AsrSummary
    e2e_summary      = $E2eSummary
    files_matched    = $n
    delta_metrics    = [ordered]@{
        mean_delta_pp   = $meanDelta
        p50_delta_pp    = $p50
        p90_delta_pp    = $p90
        p95_delta_pp    = $p95
        delta_gt_0      = $deltaGt0
        delta_gt_5      = $deltaGt5
        delta_gt_10     = $deltaGt10
        delta_lt_minus5 = $deltaLtMinus5
    }
    gates            = [ordered]@{
        L0 = [ordered]@{
            description = "No file exceeds ΔWER > +10pp"
            threshold   = $l0Threshold
            pass        = $l0Pass
            max_delta   = if ($l0Fail.Count -gt 0) { [math]::Round(($l0Fail | Measure-Object -Maximum).Maximum, 1) } else { $null }
        }
        L1 = [ordered]@{
            description = "Fewer than 20% of files exceed ΔWER > +5pp"
            threshold   = $l1Threshold
            pass        = $l1Pass
            fail_pct    = $l1FailPct
        }
    }
}

$summary | ConvertTo-Json -Depth 6 | Set-Content -Path $jsonOut -Encoding utf8
Write-Host "`nResults saved to $csvOut"
Write-Host "Summary saved to $jsonOut"
