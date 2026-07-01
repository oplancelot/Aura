# Aura – Full Build Script
# Builds both the Rust core library and the C# UI application.

param(
    [switch]$Release,
    [switch]$SkipRust,
    [switch]$SkipCSharp
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Aura Build System" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

# ── Step 1: Build Rust core ──
if (-not $SkipRust) {
    Write-Host "`n[1/2] Building Rust core library..." -ForegroundColor Yellow
    Push-Location "$ProjectRoot\core"

    if ($Release) {
        cargo build --release
        $dllSource = "target\release\aura_core.dll"
    } else {
        cargo build
        $dllSource = "target\debug\aura_core.dll"
    }

    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Rust build failed!" -ForegroundColor Red
        Pop-Location
        exit 1
    }

    # Copy DLL to C# output directory
    $dllDest = "$ProjectRoot\ui\Aura\bin"
    if (-not (Test-Path $dllDest)) { New-Item -ItemType Directory -Path $dllDest -Force | Out-Null }
    Copy-Item $dllSource "$dllDest\aura_core.dll" -Force
    Write-Host "  ✓ aura_core.dll → $dllDest" -ForegroundColor Green

    Pop-Location
}

# ── Step 2: Build C# UI ──
if (-not $SkipCSharp) {
    Write-Host "`n[2/2] Building C# UI application..." -ForegroundColor Yellow
    Push-Location "$ProjectRoot\ui"

    if ($Release) {
        dotnet build Aura.sln -c Release
    } else {
        dotnet build Aura.sln -c Debug
    }

    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: C# build failed!" -ForegroundColor Red
        Pop-Location
        exit 1
    }

    Pop-Location
}

Write-Host "`n========================================" -ForegroundColor Green
Write-Host "  Build Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Green
