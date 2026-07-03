# Phase A: 真实管线串联 — Capture → VAD → Chunking → Callback

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the mock placeholder data in `aura_core_start` with a real pipeline that captures WASAPI audio, runs VAD + StateMachine, and emits chunk info to the C# overlay via FFI callback.

**Architecture:** The pipeline runs on a dedicated thread spawned by `aura_core_start`. It reads from the lock-free ring buffer, accumulates 512-sample frames, runs Silero VAD, feeds the chunking state machine, and when a chunk is emitted, calls the registered `TranslationCallback` with descriptive text (mock translation until Phase B).

**Tech Stack:** Rust FFI, wasapi-rs, ONNX Runtime (ort), C# P/Invoke

## Global Constraints

- All existing FFI function signatures must remain ABI-compatible (extern "C", `#[no_mangle]`)
- The VAD model file path must be configurable from C# (`aura_core_set_model_path`)
- Pipeline stop must be clean within <500ms (no hanging threads)
- No new Rust crate dependencies

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `core/src/ffi/pipeline.rs` | **Create** | Pipeline state struct, processing loop function |
| `core/src/ffi/mod.rs` | **Modify** | Add `pipeline` module |
| `core/src/ffi/exports.rs` | **Modify** | Replace mock WORKER with PipelineState, add `aura_core_set_model_path` |
| `ui/Aura/Interop/AuraCoreBinding.cs` | **Modify** | Add `SetModelPath` DllImport + public wrapper |
| `ui/Aura/App.xaml.cs` | **Modify** | Set VAD model path on startup |

---

### Task 1: Create pipeline processing module

**Files:**
- Create: `core/src/ffi/pipeline.rs`

**Interfaces:**
- Consumes: `AudioRingBuffer` (Arc), `TranslationCallback` (via global static), `SileroVad::new()`, `ChunkingStateMachine::new()`
- Produces: `PipelineState` struct, `run_pipeline()` function

- [ ] **Step 1: Create `core/src/ffi/pipeline.rs`**

```rust
//! Real-time audio pipeline: WASAPI capture → VAD → chunking → callback.
//!
//! Runs on a dedicated thread spawned by [`super::exports::aura_core_start`].
//! Reads 16 kHz mono f32 samples from the ring buffer, processes them through
//! Silero VAD and the chunking state machine, and emits chunk descriptors to
//! the C# overlay via the global FFI callback.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::audio::capture::{AudioCapturer, CaptureConfig};
use crate::audio::ring_buffer::AudioRingBuffer;
use crate::vad::silero::SileroVad;
use crate::vad::state_machine::{ChunkingConfig, ChunkingStateMachine, ChunkType};

/// Holds all live pipeline state so it can be stopped and cleaned up.
pub struct PipelineState {
    pub capturer: AudioCapturer,
    pub pipeline_thread: JoinHandle<()>,
    pub stop_signal: Arc<AtomicBool>,
    pub ring_buffer: Arc<AudioRingBuffer>,
}

impl PipelineState {
    /// Create and start the full pipeline for a target PID.
    pub fn start(
        target_pid: u32,
        include_tree: bool,
        model_path: &str,
    ) -> anyhow::Result<Self> {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let ring_buffer = Arc::new(AudioRingBuffer::new(16_000, 5.0));

        // Create and start the WASAPI capturer
        let config = CaptureConfig {
            target_pid,
            include_process_tree: include_tree,
        };
        let mut capturer = AudioCapturer::new(config, Arc::clone(&ring_buffer));
        capturer.start()?;

        // Spawn the pipeline processing thread
        let thread_stop = Arc::clone(&stop_signal);
        let thread_rb = Arc::clone(&ring_buffer);
        let model_path_owned = model_path.to_owned();

        let pipeline_thread = thread::Builder::new()
            .name("aura-pipeline".into())
            .spawn(move || {
                if let Err(e) = run_pipeline(thread_stop, thread_rb, &model_path_owned) {
                    log::error!("Pipeline worker exited with error: {:#}", e);
                }
            })
            .expect("Failed to spawn pipeline thread");

        Ok(Self {
            capturer,
            pipeline_thread,
            stop_signal,
            ring_buffer,
        })
    }

    /// Signal stop and wait for clean shutdown.
    ///
    /// Takes ownership so [`JoinHandle::join`] can consume the handle.
    pub fn stop(mut self) -> anyhow::Result<()> {
        // 1. Signal the pipeline processing thread to stop
        self.stop_signal.store(true, Ordering::SeqCst);

        // 2. Wait for pipeline thread to finish
        self.pipeline_thread
            .join()
            .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))?;

        // 3. Stop the WASAPI capturer (signals its internal thread and joins it)
        self.capturer
            .stop()
            .map_err(|e| anyhow::anyhow!("Failed to stop capturer: {}", e))?;

        Ok(())
    }
}

/// The main pipeline processing loop.
///
/// Continuously reads from the ring buffer, accumulates 512-sample VAD frames,
/// runs Silero VAD inference, feeds the chunking state machine, and when a
/// chunk is emitted, dispatches it to the C# overlay via [`emit_translation`].
fn run_pipeline(
    stop_signal: Arc<AtomicBool>,
    ring_buffer: Arc<AudioRingBuffer>,
    model_path: &str,
) -> anyhow::Result<()> {
    log::info!("Pipeline worker started");

    // Load VAD model
    let mut vad = SileroVad::new(model_path)
        .map_err(|e| anyhow::anyhow!("Failed to load VAD model '{}': {}", model_path, e))?;
    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());
    let mut frame_buffer: Vec<f32> = Vec::with_capacity(16_000 * 2); // 2 seconds
    let pipeline_start = Instant::now();

    while !stop_signal.load(Ordering::SeqCst) {
        let available = ring_buffer.available();
        if available >= SileroVad::FRAME_SAMPLES {
            if let Some(samples) = ring_buffer.pull(available) {
                frame_buffer.extend_from_slice(&samples);

                // Process complete VAD frames
                while frame_buffer.len() >= SileroVad::FRAME_SAMPLES {
                    let frame: Vec<f32> =
                        frame_buffer.drain(..SileroVad::FRAME_SAMPLES).collect();

                    let vad_result = vad.process_frame(&frame)?;

                    if let Some(chunk) = state_machine.feed(&vad_result, &frame) {
                        let duration_ms =
                            (chunk.samples.len() as u64 * 1000) / 16_000;
                        let latency = pipeline_start.elapsed().as_millis() as i32;

                        let (text, is_provisional) = match chunk.chunk_type {
                            ChunkType::Provisional => {
                                (format!("[~] {}ms speech...", duration_ms), true)
                            }
                            ChunkType::Final => {
                                (format!("[✓] {}ms sentence", duration_ms), false)
                            }
                            ChunkType::HardCut => {
                                (format!("[✂] {}ms (hard cut)", duration_ms), false)
                            }
                        };

                        // Dispatch to the registered C# callback
                        super::exports::emit_translation(&text, is_provisional, latency);
                    }
                }
            }
        }

        // Brief sleep to avoid busy-waiting while keeping latency low
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Flush any remaining partial frame
    if !frame_buffer.is_empty() {
        let duration_ms = (frame_buffer.len() as u64 * 1000) / 16_000;
        let text = format!("[✓] {}ms (flush)", duration_ms);
        super::exports::emit_translation(&text, false, 0);
    }

    log::info!("Pipeline worker stopped");
    Ok(())
}
```

- [ ] **Step 2: Add `pipeline` module in mod.rs**

Edit `core/src/ffi/mod.rs`:

```rust
pub mod exports;
pub mod pipeline;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p aura_core`
Expected: Clean compile (warning about unused imports is fine, they'll be used in Task 2)

- [ ] **Step 4: Commit**

```bash
git add core/src/ffi/pipeline.rs core/src/ffi/mod.rs
git commit -m "feat: add pipeline processing module"
```

---

### Task 2: Rewrite FFI exports to use real pipeline

**Files:**
- Modify: `core/src/ffi/exports.rs` — replace mock WORKER with PipelineState, add `aura_core_set_model_path`, make `emit_translation` public

**Interfaces:**
- Consumes: `PipelineState` from pipeline.rs
- Produces: Updated FFI exports (`aura_core_start`, `aura_core_stop`, `aura_core_set_model_path`)

- [ ] **Step 1: Modify `core/src/ffi/exports.rs`**

Replace the entire file content. The key changes:
1. Add `static MODEL_PATH: OnceLock<Mutex<String>>` for VAD model path
2. Replace `static WORKER: OnceLock<Mutex<Option<JoinHandle<()>>>>` with `static PIPELINE: OnceLock<Mutex<Option<PipelineState>>>`
3. Remove `static IS_RUNNING: AtomicBool` (PipelineState carries its own stop_signal)
4. Rewrite `aura_core_start` to call `PipelineState::start()`
5. Rewrite `aura_core_stop` to call `PipelineState::stop()`
6. Add `aura_core_set_model_path` export
7. Make `emit_translation` public (`pub(crate)`)

```rust
//! C ABI exports for consumption by C# via P/Invoke (DllImport).
//!
//! These functions form the contract between the Rust core DLL and the C# UI.
//! All exported functions use `extern "C"` and `#[no_mangle]` to ensure stable
//! symbol names.
//!
//! # Lifecycle
//! 1. C# calls `aura_core_init()` to initialise the pipeline
//! 2. C# calls `aura_core_register_callback()` to set the translation text callback
//! 3. C# calls `aura_core_set_model_path()` to point to the Silero VAD onnx file
//! 4. C# calls `aura_core_start(pid)` to begin capturing a target process
//! 5. When audio chunks are processed, the registered callback is invoked from Rust
//! 6. C# calls `aura_core_stop()` to halt the pipeline
//! 7. C# calls `aura_core_destroy()` to free all resources

use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::pipeline::PipelineState;

/// Callback function pointer type for delivering translation results to C#.
pub type TranslationCallback =
    unsafe extern "C" fn(text: *const c_char, is_provisional: c_int, latency_ms: c_int);

// ── Global state ───────────────────────────────────────────────────────

static CALLBACK: OnceLock<Mutex<Option<TranslationCallback>>> = OnceLock::new();
static MODEL_PATH: OnceLock<Mutex<String>> = OnceLock::new();
static PIPELINE: OnceLock<Mutex<Option<PipelineState>>> = OnceLock::new();

fn callback_slot() -> &'static Mutex<Option<TranslationCallback>> {
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn model_path_slot() -> &'static Mutex<String> {
    MODEL_PATH.get_or_init(|| Mutex::new("assets/silero_vad.onnx".to_string()))
}

fn pipeline_slot() -> &'static Mutex<Option<PipelineState>> {
    PIPELINE.get_or_init(|| Mutex::new(None))
}

/// Dispatch translation text to the registered C# callback.
///
/// Safe to call from any thread. Does nothing if no callback is registered.
pub(crate) fn emit_translation(text: &str, is_provisional: bool, latency_ms: i32) {
    let callback = callback_slot().lock().ok().and_then(|slot| *slot);
    let Some(callback) = callback else {
        return;
    };

    let Ok(c_text) = CString::new(text) else {
        log::warn!("Skipping translation callback because text contains a null byte");
        return;
    };

    unsafe {
        callback(
            c_text.as_ptr(),
            if is_provisional { 1 } else { 0 },
            latency_ms,
        );
    }
}

// ── Exported functions ─────────────────────────────────────────────────

/// Initialise the Aura core pipeline.
///
/// Must be called once before any other function.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_init() -> c_int {
    let _ = env_logger::builder().is_test(false).try_init();
    callback_slot();
    model_path_slot();
    pipeline_slot();
    log::info!("aura_core_init() called");
    0
}

/// Set the path to the Silero VAD ONNX model file.
///
/// Must be called before `aura_core_start()`. Pass a null-terminated UTF-8 path.
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_model_path(path: *const c_char) {
    if path.is_null() {
        return;
    }
    let path_str = CStr::from_ptr(path).to_string_lossy().into_owned();
    if let Ok(mut slot) = model_path_slot().lock() {
        *slot = path_str;
    }
    log::info!("VAD model path updated");
}

/// Register a callback function that will be called whenever
/// a translation result (provisional or final) is ready.
#[no_mangle]
pub unsafe extern "C" fn aura_core_register_callback(cb: TranslationCallback) {
    if let Ok(mut slot) = callback_slot().lock() {
        *slot = Some(cb);
    }
    log::info!("Translation callback registered");
}

/// Start the audio capture and translation pipeline targeting the given PID.
///
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_start(target_pid: u32) -> c_int {
    log::info!("aura_core_start(pid={})", target_pid);

    let mut pipeline = match pipeline_slot().lock() {
        Ok(guard) => guard,
        Err(_) => return -1,
    };

    if pipeline.is_some() {
        log::warn!("aura_core_start() ignored because pipeline is already running");
        return 0;
    }

    let model_path = model_path_slot()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "assets/silero_vad.onnx".to_string());

    match PipelineState::start(target_pid, true, &model_path) {
        Ok(state) => {
            *pipeline = Some(state);
            0
        }
        Err(e) => {
            log::error!("Failed to start pipeline: {:#}", e);
            -1
        }
    }
}

/// Stop the audio capture and translation pipeline.
///
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_stop() -> c_int {
    log::info!("aura_core_stop()");

    let mut pipeline = match pipeline_slot().lock() {
        Ok(guard) => guard,
        Err(_) => return -1,
    };

    if let Some(mut state) = pipeline.take() {
        if let Err(e) = state.stop() {
            log::error!("Pipeline stop error: {:#}", e);
            return -1;
        }
    }

    0
}

/// Destroy the Aura core pipeline and free all resources.
///
/// After this call, no other function may be called without re-init.
#[no_mangle]
pub unsafe extern "C" fn aura_core_destroy() {
    log::info!("aura_core_destroy()");
    let _ = aura_core_stop();
    if let Ok(mut slot) = callback_slot().lock() {
        *slot = None;
    }
}

/// Set the AI engine to use. Pass a null-terminated UTF-8 engine name.
///
/// Supported values: "gemini", "sensevoice"
/// Returns 0 on success, -1 if the engine name is unknown.
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_engine(engine_name: *const c_char) -> c_int {
    if engine_name.is_null() {
        return -1;
    }
    let name = CStr::from_ptr(engine_name).to_string_lossy();
    log::info!("aura_core_set_engine(\"{}\")", name);

    match name.as_ref() {
        "gemini" | "sensevoice" => 0,
        _ => {
            log::error!("Unknown engine: {}", name);
            -1
        }
    }
}

/// Set the API key for cloud-based engines.
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_api_key(api_key: *const c_char) {
    if api_key.is_null() {
        return;
    }
    let _key = CStr::from_ptr(api_key).to_string_lossy();
    log::info!("API key updated (length={})", _key.len());
}

/// Set the target translation language (ISO 639-1 code).
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_target_lang(lang: *const c_char) {
    if lang.is_null() {
        return;
    }
    let lang_str = CStr::from_ptr(lang).to_string_lossy();
    log::info!("Target language set to: {}", lang_str);
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p aura_core`
Expected: Clean compile (warnings about unused imports like `JoinHandle`, `Duration`, etc. are fine)

- [ ] **Step 3: Run existing unit tests**

Run: `cargo test -p aura_core`
Expected: All tests pass (resampler, capture, VAD tests)

- [ ] **Step 4: Commit**

```bash
git add core/src/ffi/exports.rs
git commit -m "feat: wire real capture→VAD→chunking pipeline in FFI exports"
```

---

### Task 3: Add `SetModelPath` to C# bindings

**Files:**
- Modify: `ui/Aura/Interop/AuraCoreBinding.cs` — add `aura_core_set_model_path` DllImport + public wrapper

- [ ] **Step 1: Add DllImport and public wrapper to `AuraCoreBinding.cs`**

Add after the existing DllImports (before the `// ── Public API ──` section):

```csharp
[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
private static extern void aura_core_set_model_path(
    [MarshalAs(UnmanagedType.LPUTF8Str)] string path);
```

Add in the Public API section:

```csharp
/// <summary>Set the path to the Silero VAD ONNX model file.</summary>
public static void SetModelPath(string path) => aura_core_set_model_path(path);
```

- [ ] **Step 2: Commit**

```bash
git add ui/Aura/Interop/AuraCoreBinding.cs
git commit -m "feat(cs): add SetModelPath to AuraCoreBinding"
```

---

### Task 4: Set VAD model path in C# startup

**Files:**
- Modify: `ui/Aura/App.xaml.cs` — set model path after init, before start

- [ ] **Step 1: Set model path in `App.xaml.cs`**

Edit the `OnStartup` method to set the model path:

```csharp
protected override void OnStartup(StartupEventArgs e)
{
    base.OnStartup(e);

    // 1. Initialise the Rust core
    int result = Interop.AuraCoreBinding.Init();
    if (result != 0)
    {
        MessageBox.Show("Failed to initialise Aura core engine.", "Aura Error",
            MessageBoxButton.OK, MessageBoxImage.Error);
        Shutdown(1);
        return;
    }

    // 2. Set VAD model path (relative to the executable directory)
    var modelPath = System.IO.Path.Combine(
        AppDomain.CurrentDomain.BaseDirectory,
        "silero_vad.onnx");
    Interop.AuraCoreBinding.SetModelPath(modelPath);

    // 3. Start the overlay renderer (transparent OSD window)
    _overlay = new OverlayRenderer.TranslationOverlay();
    _overlay.Start();

    // 4. Register the translation callback
    Interop.AuraCoreBinding.RegisterCallback(_overlay.OnTranslationReceived);

    // 5. Set up system tray icon
    _trayManager = new WindowManager.TrayIconManager();
    _trayManager.Initialize();
}
```

- [ ] **Step 2: Update build script to copy VAD model alongside the DLL**

Edit `scripts/build_all.ps1` to also copy the VAD model file:

After the DLL copy step (after line 39), add:

```powershell
# Copy VAD model to C# output directory
$vadModelSource = "$ProjectRoot\core\assets\silero_vad.onnx"
$vadModelDest = "$dllDest\silero_vad.onnx"
if (Test-Path $vadModelSource) {
    Copy-Item $vadModelSource $vadModelDest -Force
    Write-Host "  ✓ silero_vad.onnx → $dllDest" -ForegroundColor Green
} else {
    Write-Host "  [!] silero_vad.onnx not found at $vadModelSource" -ForegroundColor Yellow
}
```

Wait — `$dllDest` points to `ui/Aura/bin/`. But the Rust build step uses `$dllDest` which is derived from the debug/release path. Let me re-check the build script.

Looking at the build script:
```powershell
Push-Location "$ProjectRoot\core"
if ($Release) {
    cargo build --release
    $dllSource = "target\release\aura_core.dll"
} else {
    cargo build
    $dllSource = "target\debug\aura_core.dll"
}
$dllDest = "$ProjectRoot\ui\Aura\bin"
Copy-Item $dllSource "$dllDest\aura_core.dll" -Force
```

So `$dllDest` is always `$ProjectRoot\ui\Aura\bin`. This is the directory where both the DLL and model should end up.

- [ ] **Step 3: Commit**

```bash
git add ui/Aura/App.xaml.cs scripts/build_all.ps1
git commit -m "feat: configure VAD model path in C# startup and build"
```

---

## Self-Review

- **Spec coverage:** All items covered — pipeline module (Task 1), FFI wiring (Task 2), C# bindings (Task 3), startup config (Task 4)
- **Placeholder scan:** No TBD/TODO in the plan content. All code is complete and compilable.
- **Type consistency:** `PipelineState::start(pid, tree, model_path)` → `PipelineState::stop()` — consistent across Tasks 1 and 2. `emit_translation(text, bool, i32)` matches the callback type in exports.
- **Dependency order:** Task 1 → Task 2 → Task 3 → Task 4 (each builds on previous)
