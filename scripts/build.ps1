param(
    [string]$RepoRoot = "$PSScriptRoot\.."
)

$ErrorActionPreference = "Stop"

$AssetsDir = Join-Path $RepoRoot "assets"
$CoreReleaseDir = Join-Path $RepoRoot "core" "target" "release"
$AuraProject = Join-Path $RepoRoot "ui" "Aura"
$PublishDir = Join-Path $AuraProject "publish"

Write-Host "[1/3] Building Rust core (release)..." -ForegroundColor Cyan
Push-Location (Join-Path $RepoRoot "core")
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
finally { Pop-Location }

Write-Host "[2/3] Publishing Aura..." -ForegroundColor Cyan
if (Test-Path $PublishDir) { Remove-Item $PublishDir -Recurse -Force }
dotnet publish $AuraProject -c Release --no-self-contained -r win-x64 -o $PublishDir
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed" }

Write-Host "[3/3] Copying runtime assets..." -ForegroundColor Cyan
Copy-Item (Join-Path $CoreReleaseDir "aura_core.dll") $PublishDir
Copy-Item (Join-Path $AssetsDir "silero_vad.onnx") $PublishDir
$gguf = Join-Path $AssetsDir "sense-voice-small-q4_k.gguf"
if (Test-Path $gguf) { Copy-Item $gguf $PublishDir }

Write-Host "`nDone! Run Aura.exe from:" -ForegroundColor Green
Write-Host "  $PublishDir" -ForegroundColor Cyan
