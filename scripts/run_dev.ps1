# Aura – Development Mode Launcher
# Builds in debug mode and launches the application.

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot

Write-Host "🚀 Aura Dev Mode" -ForegroundColor Cyan

# Build first
& "$PSScriptRoot\build_all.ps1"

# Run the C# application
Write-Host "`nLaunching Aura..." -ForegroundColor Yellow
Push-Location "$ProjectRoot\ui"
dotnet run --project Aura\Aura.csproj
Pop-Location
