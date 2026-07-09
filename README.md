# Aura ✨

**Real-time AI translation subtitles overlay for any application.**

Captures audio from any app (browser, game, chat, media player), transcribes and translates it via AI, and displays transparent subtitles on top of your screen — with zero mouse interference.

## Architecture

```
WASAPI Capture → Ring Buffer → Silero VAD → Chunking State Machine → AI Engine → OSD Overlay
     10ms            ↕              32ms           200-800ms           70-200ms      <10ms
                 Lock-free                    Provisional/Final
```

**Total end-to-end latency: 300–550 ms**

## Tech Stack

| Layer         | Language | Key Libraries                                     |
| :------------ | :------- | :------------------------------------------------ |
| Core Pipeline | Rust     | wasapi-rs, ort (ONNX Runtime), tokio, tungstenite |
| UI / OSD      | C#       | GameOverlay.Net (Direct2D), WPF                   |
| Bridge        | C ABI    | `#[no_mangle] extern "C"` ↔ P/Invoke              |

## Project Structure

```
aura/
├── core/           # Rust core DLL (audio → VAD → ASR → FFI)
├── ui/             # C# WPF application (OSD overlay + settings)
├── assets/         # AI model weights (Silero VAD)
├── scripts/        # Build, dev & test scripts
└── docs/           # Architecture & design documentation
```

## Quick Start

### Prerequisites

- Rust toolchain (1.75+)
- .NET 10 SDK
- Windows 10 Build 20348+ (for WASAPI process loopback)

### Build

```powershell
cd core && cargo build --release
cd ../ui && dotnet build Aura.sln -c Release
```

Model files (`silero_vad.onnx` in repo, `sense-voice-small-q4_k.gguf` downloaded from HuggingFace) are copied automatically via csproj links. Start Aura from `ui/Aura/bin/Release/net10.0-windows/Aura.exe`.

## Controls

- **Ctrl+Shift+L** — Toggle overlay between Combat Mode (click-through) and Configuration Mode (draggable)

## License

[MIT](LICENSE)
