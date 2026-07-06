param(
    [Parameter(Mandatory)]
    [string]$Version,
    [string]$RepoRoot = "$PSScriptRoot\.."
)

$ErrorActionPreference = "Stop"

# --- Configuration ---
$AssetsDir = Join-Path $RepoRoot "assets"
$CoreReleaseDir = Join-Path $RepoRoot "core" "target" "release"
$AuraProject = Join-Path $RepoRoot "ui" "Aura"
$PublishDir = Join-Path $RepoRoot "releases" "_publish"
$ReleaseDir = Join-Path $RepoRoot "releases" "v$Version"

# --- Step 1: Build Rust core ---
Write-Host "[1/5] Building Rust core (release)..." -ForegroundColor Cyan
Push-Location (Join-Path $RepoRoot "core")
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}
finally { Pop-Location }

# --- Step 2: Publish Aura (self-contained) ---
Write-Host "[2/5] Publishing Aura (self-contained)..." -ForegroundColor Cyan
if (Test-Path $PublishDir) { Remove-Item $PublishDir -Recurse -Force }
dotnet publish $AuraProject -c Release --no-self-contained -r win-x64 -o $PublishDir
if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed" }

# Copy Rust core DLL
Copy-Item (Join-Path $CoreReleaseDir "aura_core.dll") $PublishDir

# Copy VAD model (not the GGUF — user downloads separately)
Copy-Item (Join-Path $AssetsDir "silero_vad.onnx") $PublishDir

# --- Step 3: Build updater (Updater.exe for the installer) ---
Write-Host "[3/5] Building Velopack release..." -ForegroundColor Cyan
vpk pack `
    --packId "Aura" `
    --packVersion $Version `
    --packDir $PublishDir `
    --mainExe "Aura.exe" `
    --releaseDir $ReleaseDir

if ($LASTEXITCODE -ne 0) { throw "vpk pack failed" }

# --- Step 4: Cleanup ---
Write-Host "[4/5] Cleaning up..." -ForegroundColor Cyan
Remove-Item $PublishDir -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "[5/5] Done!" -ForegroundColor Green
Write-Host "`nRelease directory: $ReleaseDir" -ForegroundColor Green
Write-Host "Contents:" -ForegroundColor Cyan
Get-ChildItem $ReleaseDir | ForEach-Object { "  $($_.Name)" }
