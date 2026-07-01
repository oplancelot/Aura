# Aura ✨

**Real-time AI voice translation overlay for gamers.**

Captures audio from Discord / TeamSpeak, translates it via AI, and displays transparent subtitles on top of your game — with zero mouse interference.

## Architecture

```
WASAPI Capture → Ring Buffer → Silero VAD → Chunking State Machine → AI Engine → OSD Overlay
     10ms            ↕              32ms           200-800ms           70-200ms      <10ms
                 Lock-free                    Provisional/Final
```

**Total end-to-end latency: 300–550 ms**

## Tech Stack

| Layer | Language | Key Libraries |
|:---|:---|:---|
| Core Pipeline | Rust | wasapi-rs, ort (ONNX Runtime), tokio, tungstenite |
| UI / OSD | C# | GameOverlay.Net (Direct2D), WPF |
| Bridge | C ABI | `#[no_mangle] extern "C"` ↔ P/Invoke |

## Project Structure

```
aura/
├── core/           # Rust core DLL (audio → VAD → AI → FFI)
├── ui/             # C# WPF application (OSD overlay + settings)
├── models/         # AI model weights (Silero VAD, SenseVoice)
├── tests/          # Integration tests & audio samples
├── scripts/        # Build & dev scripts
└── docs/           # Architecture documentation
```

## Quick Start

### Prerequisites
- Rust toolchain (1.75+)
- .NET 10 SDK
- Windows 10 Build 20348+ (for WASAPI process loopback)

### Build
```powershell
.\scripts\build_all.ps1
```

### Run (Dev)
```powershell
.\scripts\run_dev.ps1
```

## Controls
- **Ctrl+Shift+L** — Toggle overlay between Combat Mode (click-through) and Configuration Mode (draggable)

## License
TBD
