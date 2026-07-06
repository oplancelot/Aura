# CI/CD Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create GitHub Actions workflow that builds Aura on tag push and publishes a Velopack release.

**Architecture:** Single workflow file `.github/workflows/release.yml` triggered by `v*` tags, running on `windows-latest`. Builds Rust core, publishes C# UI with `--no-self-contained`, packages with `vpk`, uploads to GitHub Release.

**Tech Stack:** GitHub Actions, Rust, .NET 10, Velopack

## Global Constraints

- Windows x64 only — RID `win-x64`, Rust target `x86_64-pc-windows-msvc`
- Only trigger on tag push matching `v*`
- `--no-self-contained` — publish without bundled .NET runtime
- Velopack `vpk` for installer and update packages
- VAD model `silero_vad.onnx` included in package; GGUF model NOT included (downloaded by user from app)
- GitHub Release assets: `Setup.exe`, `RELEASES`, `*.nupkg`

---

### Task 1: Create release workflow + fix release.ps1

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `scripts/release.ps1`

**Interfaces:**
- Consumes: existing `scripts/release.ps1` steps as reference
- Produces: GitHub Release with Velopack artifacts when tag is pushed

- [ ] **Step 1: Create `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    runs-on: windows-latest

    env:
      DOTNET_CLI_TELEMETRY_OPTOUT: 1
      CARGO_TERM_COLOR: always

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          target: x86_64-pc-windows-msvc

      - name: Setup .NET
        uses: actions/setup-dotnet@v4
        with:
          dotnet-version: 10.0.x

      - name: Build Rust core
        run: cargo build --release
        working-directory: core

      - name: Publish C# UI
        run: dotnet publish --no-self-contained -r win-x64 -o publish
        working-directory: ui/Aura

      - name: Assemble release files
        shell: pwsh
        run: |
          Copy-Item core/target/release/aura_core.dll ui/Aura/publish/
          Copy-Item assets/silero_vad.onnx ui/Aura/publish/

      - name: Install vpk
        run: dotnet tool install -g vpk

      - name: Package with Velopack
        shell: pwsh
        run: |
          $ver = $env:GITHUB_REF_NAME -replace '^v'
          vpk pack --packId Aura --packVersion $ver --packDir ui/Aura/publish --mainExe Aura.exe --releaseDir release-output

      - name: Upload Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            release-output/Setup.exe
            release-output/RELEASES
            release-output/*.nupkg
          generate_release_notes: true
```

- [ ] **Step 2: Fix `scripts/release.ps1` to use `--no-self-contained`**

Change line 28 from `--self-contained` to `--no-self-contained`:

```powershell
dotnet publish $AuraProject -c Release --no-self-contained -r win-x64 -o $PublishDir
```

- [ ] **Step 3: Verify workflow syntax**

Run: `bash -c "if (Test-Path .github/workflows/release.yml) { Write-Host 'workflow created' }"`
Expected: workflow created

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml scripts/release.ps1
git commit -m "ci: add GitHub Actions release workflow (tag-triggered, Velopack)"
```
