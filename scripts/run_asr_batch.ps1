# LJSpeech 批量 ASR 测试
# 先编译，然后遍历 WAV 文件运行 transcribe_wav
# Usage: .\scripts\run_asr_batch.ps1 [max_files]

param(
    [int]$MaxFiles = 0
)

$wavDir = "OpenSLR/LJSpeech/wavs"
$example = "core\target\release\examples\transcribe_wav.exe"

# Build once
Write-Host "Building transcribe_wav..."
Push-Location core
cargo build --release --example transcribe_wav 2>&1 | Out-Null
Pop-Location

if (-not (Test-Path $example)) {
    Write-Host "ERROR: transcribe_wav.exe not found at $example"
    exit 1
}

$wavs = Get-ChildItem "$wavDir/*.wav"
if ($MaxFiles -gt 0) {
    $wavs = $wavs | Select-Object -First $MaxFiles
}
$totalCount = $wavs.Count

$results = @()
$totalWER = 0.0
$totalTime = 0.0
$tested = 0

Write-Host "Testing $totalCount files...`n"

foreach ($wav in $wavs) {
    $name = $wav.BaseName
    Write-Progress -Activity "ASR Testing" -Status "$name ($tested/$totalCount)" -PercentComplete (($tested / $totalCount) * 100)

    # Capture only stdout, ignore stderr (model loading noise)
    $output = & $example $wav.FullName 2>$null

    $hyp = ""
    $ref = ""
    $wer = $null
    $timeSec = 0.0
    $hasRef = $false

    foreach ($line in $output) {
        if ($line -match "^=== Full transcription ===") { $hasRef = $true }
        elseif ($line -match "^=== Reference ===") { }
        elseif ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
        elseif ($line -match "^Audio: .+ Processing: ([\d.]+)s") { $timeSec = [double]$Matches[1] }
        elseif ($line -match "^\[.*\] (?![-\!\[])" -and $hasRef -eq $false) {
            if ($hyp -eq "") { $hyp = $line -replace '^\[\s*[\d.]+\s*s\] ', '' }
        }
    }

    # Also capture lines after the === markers
    $inHyp = $false; $inRef = $false
    foreach ($line in $output) {
        if ($line -match "^=== Full transcription ===") { $inHyp = $true; $inRef = $false; continue }
        if ($line -match "^=== Reference ===") { $inRef = $true; $inHyp = $false; continue }
        if ($inHyp -and $line.Trim() -ne "") { $hyp = $line.Trim() }
        if ($inRef -and $line.Trim() -ne "") { $ref = $line.Trim() }
        if ($inHyp -and $line -match "^WER:") { break }
    }

    Write-Host "[$($tested+1)/$totalCount] $name" -NoNewline
    if ($wer -ne $null) {
        Write-Host "  WER: $wer%  time: ${timeSec}s"
        $results += [PSCustomObject]@{
            File = $name
            WER = $wer
            Time = $timeSec
            Hyp = $hyp
            Ref = $ref
        }
        $totalWER += $wer
        $totalTime += $timeSec
        $tested++
    } else {
        Write-Host "  (no reference)"
    }
}

Write-Host "`n=== Batch Summary ==="
Write-Host "Files tested: $tested / $totalCount"
if ($tested -gt 0) {
    Write-Host "Avg WER: $([math]::Round($totalWER / $tested, 1))%"
    Write-Host "Avg time: $([math]::Round($totalTime / $tested, 2))s"
    Write-Host "Total ASR time: $([math]::Round($totalTime, 1))s"
}

$results | Export-Csv "asr_batch_results.csv" -NoTypeInformation
Write-Host "Results saved to asr_batch_results.csv"
