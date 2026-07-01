# Aura Architecture

See [implementation_plan.md](../../implementation_plan.md) for the full architecture breakdown.

## Data Flow

```
Discord Process (PID tree)
    ↓ WASAPI Process Loopback (~10-15ms)
Audio Capture Thread (48kHz → 16kHz resampling)
    ↓ Lock-free Ring Buffer (SPSC)
VAD Processing Thread (Silero VAD, 32ms frames)
    ↓ Chunking State Machine (Provisional / Final / HardCut)
AI Translation (Gemini WebSocket / SenseVoice local)
    ↓ FFI Callback (extern "C")
C# Overlay Renderer (Direct2D, GameOverlay.Net)
    ↓ WS_EX_TRANSPARENT + WS_EX_LAYERED
Game Screen (transparent, click-through subtitles)
```

## Module Dependency Graph

```
core::audio  ──→  core::vad  ──→  core::ai
                                      │
                                 core::ffi
                                      │
                              ui::Interop (P/Invoke)
                                      │
                          ui::OverlayRenderer (Direct2D)
                          ui::WindowManager (Hotkeys)
                          ui::Views (Settings WPF)
```
