# CI/CD Pipeline Design

## Overview

Automate Aura builds and releases via GitHub Actions. Tag push → build → package → publish GitHub Release.

## Constraints

- Windows x64 only (WASAPI + WPF)
- Release on tag push (`v*`)
- `--no-self-contained` — user must have .NET runtime installed
- Velopack `vpk` for packaging/updates

## Workflow

**File:** `.github/workflows/release.yml`

**Trigger:** `push: tags: ['v*']`

**Runner:** `windows-latest`

### Steps

1. **Checkout** — `actions/checkout@v4`
2. **Setup Rust** — `actions-rs/toolchain@v1`, target `x86_64-pc-windows-msvc`
3. **Setup .NET** — `actions/setup-dotnet@v4`
4. **Build Rust core** — `cargo build --release` in `core/`
5. **Publish C# UI** — `dotnet publish --no-self-contained -r win-x64` in `ui/Aura/`
6. **Assemble** — copy `aura_core.dll` + `silero_vad.onnx` into publish dir
7. **Install vpk** — `dotnet tool install -g vpk`
8. **Package** — `vpk pack` with `--framework net10.0-x64-desktop`
9. **Upload Release** — `softprops/action-gh-release` with:
   - `Setup.exe`
   - `RELEASES`
   - `Aura-{version}.nupkg`
   - `Aura-{version}-delta.nupkg` (if applicable)

### Artifacts

| File | Purpose |
|------|---------|
| `Setup.exe` | Velopack installer — first-time install |
| `RELEASES` | Update manifest — Velopack checks this for new versions |
| `Aura-{version}.nupkg` | Full package — included in Release for reference |
| `Aura-{version}-delta.nupkg` | Delta update — only if previous release exists |

### Velopack Update URL

The `UpdateManager` in `UpdateChecker.cs` uses `GithubSource("https://github.com/oplancelot/Aura")`. Velopack will fetch `RELEASES` from the GitHub Release assets to determine available updates.

## Manual Release

```powershell
git tag v0.1.0
git push origin v0.1.0   # triggers Actions, 产出 Release
```

## Future Considerations

- Add `ci.yml` for PR builds (compile check only, no release)
- Add code signing certificate for `Setup.exe`
- Support `arm64` if needed
