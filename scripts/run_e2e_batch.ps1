# LJSpeech E2E 管线测试
# 编译后遍历 WAV 文件运行 e2e_transcribe_wav
# Usage: .\scripts\run_e2e_batch.ps1 [max_files]
# 输出: e2e_batch_results.csv + terminal summary

param(
    [int]$MaxFiles = 0
)

$wavDir = "OpenSLR/LJSpeech/wavs"
$example = "core\target\release\examples\e2e_transcribe_wav.exe"

# Build once
Write-Host "Building e2e_transcribe_wav..."
Push-Location core
cargo build --release --example e2e_transcribe_wav 2>&1
Pop-Location

if (-not (Test-Path $example)) {
    Write-Host "ERROR: e2e_transcribe_wav.exe not found at $example"
    exit 1
}

$wavs = Get-ChildItem "$wavDir/*.wav"
if ($MaxFiles -gt 0) {
    $wavs = $wavs | Select-Object -First $MaxFiles
}
$totalCount = $wavs.Count

$results = @()
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
$tested = 0

Write-Host "Testing $totalCount files...`n"

foreach ($wav in $wavs) {
    $name = $wav.BaseName
    Write-Progress -Activity "E2E Testing" -Status "$name ($tested/$totalCount)" -PercentComplete (($tested / $totalCount) * 100)

    $output = & $example $wav.FullName 2>$null

    $wer = $null
    $asrMs = 0.0
    $procTime = 0.0
    $audioTime = 0.0
    $chunks = 0
    $final = 0
    $hardCut = 0
    $provisional = 0
    $minChunk = 0.0; $avgChunk = 0.0; $maxChunk = 0.0

    # Parse summary lines
    # Example summary: "Audio: 6.0s | Processing: 0.9s | ASR: 720ms total | RTF: 0.15x"
    foreach ($line in $output) {
        if ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
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
        }
        $totalWER += $wer
        $totalAsrMs += $asrMs
        $totalProcessTime += $procTime
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
        $tested++
    } else {
        Write-Host "  (no reference)"
    }
}

Write-Host "`n=== E2E Batch Summary ==="
Write-Host "Files tested: $tested / $totalCount"
if ($tested -gt 0) {
    Write-Host "Avg WER: $([math]::Round($totalWER / $tested, 1))%"
    Write-Host "Avg ASR: $([math]::Round($totalAsrMs / $tested, 0))ms"
    Write-Host "Avg Processing: $([math]::Round($totalProcessTime / $tested, 2))s"
    Write-Host "Total ASR time: $([math]::Round($totalAsrMs / 1000, 1))s"

    Write-Host "`n=== Segmentation Quality ($tested files) ==="
    Write-Host "Total chunks: $totalChunks  (Final: $totalFinal | HardCut: $totalHardCut | Provisional: $totalProvisional)"
    Write-Host "Files with >1 chunk: $multiChunkFiles ($([math]::Round($multiChunkFiles / $tested * 100, 0))%)"
    $meanAvgChunk = [math]::Round($sumAvgChunk / $tested, 2)
    $meanMinChunk = [math]::Round($sumMinChunk / $tested, 2)
    $meanMaxChunk = [math]::Round($sumMaxChunk / $tested, 2)
    $gMin = if ($globalMinChunk -lt [double]::MaxValue) { [math]::Round($globalMinChunk, 2) } else { 0 }
    $gMax = if ($globalMaxChunk -gt [double]::MinValue) { [math]::Round($globalMaxChunk, 2) } else { 0 }
    Write-Host "Mean of per-file avg/min/max chunk: ${meanAvgChunk}s / ${meanMinChunk}s / ${meanMaxChunk}s"
    Write-Host "Global min/max chunk: ${gMin}s / ${gMax}s"
}

$results | Export-Csv "e2e_batch_results.csv" -NoTypeInformation
Write-Host "`nResults saved to e2e_batch_results.csv"
