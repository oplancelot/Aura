# Phase B: SenseVoice 本地 ASR 引擎集成

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate SenseVoice-Small local ASR model into the Aura pipeline, replacing mock text with real speech recognition output for Final/HardCut chunks.

**Architecture:** SenseVoice.cpp + ggml built as static libraries via CMake. A thin C wrapper exposes `extern "C"` functions for model load/transcribe/free. Rust FFI bindings call them from a safe `SenseVoiceEngine` that is instantiated in `PipelineState` and used in `run_pipeline()` for Final/HardCut chunks.

**Tech Stack:** SenseVoice.cpp, ggml, CMake, MSVC, Rust FFI, cc crate, cmake crate

## Global Constraints

- ASR model path stored separately from VAD model path (`aura_core_set_asr_model_path`)
- Build.rs uses `cmake` crate for SenseVoice.cpp + ggml, `cc` crate for C wrapper
- All existing FFI function signatures remain ABI-compatible
- `PIPELINE` static in exports.rs keeps using same `OnceLock<Mutex<Option<PipelineState>>>` pattern
- No new Rust crate dependencies beyond `cmake` (build-dep)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `core/3rdparty/SenseVoice.cpp/` | **Submodule** | SenseVoice.cpp source + ggml |
| `core/src/ai/sense_voice_capi.h` | **Create** | C wrapper API header (extern "C") |
| `core/src/ai/sense_voice_capi.cc` | **Create** | C wrapper impl bridging to C++ API |
| `core/build.rs` | **Create/Modify** | CMake build + cc compilation |
| `core/Cargo.toml` | **Modify** | Add `cmake` build-dependency |
| `core/src/ai/sense_voice.rs` | **Modify** | Replace stubs with real FFI wrapper |
| `core/src/ai/sense_voice_ffi.rs` | **Create** | Unsafe FFI declarations |
| `core/src/ai/mod.rs` | **Modify** | Add `sense_voice_ffi` module |
| `core/src/ffi/exports.rs` | **Modify** | Add `aura_core_set_asr_model_path`, pass to PipelineState |
| `core/src/ffi/pipeline.rs` | **Modify** | Accept ASR model path, create SenseVoiceEngine, use in run_pipeline |
| `core/.gitignore` | **Modify** | Ignore cmake build artifacts if any |
| `ui/Aura/Interop/AuraCoreBinding.cs` | **Modify** | Add `SetAsrModelPath` DllImport |
| `ui/Aura/App.xaml.cs` | **Modify** | Set ASR model path on startup |
| `scripts/build_all.ps1` | **Modify** | Copy GGUF model to bin dir |

---

### Task 1: Add SenseVoice.cpp submodule + download GGUF model

**Files:**
- Add: `core/3rdparty/SenseVoice.cpp/` (git submodule)
- Add: `models/sense-voice-small-q4_k.gguf` (downloaded model)

- [ ] **Step 1: Add git submodule**

```bash
git submodule add https://github.com/lovemefan/SenseVoice.cpp core/3rdparty/SenseVoice.cpp
git submodule update --init --recursive
```

- [ ] **Step 2: Download GGUF model**

From HuggingFace: https://huggingface.co/lovemefan/sense-voice-gguf/resolve/main/sense-voice-small-q4_k.gguf

Place at: `models/sense-voice-small-q4_k.gguf`

```bash
curl -L -o models/sense-voice-small-q4_k.gguf ^
  https://huggingface.co/lovemefan/sense-voice-gguf/resolve/main/sense-voice-small-q4_k.gguf
```

- [ ] **Step 3: Commit**

```bash
git add .gitmodules core/3rdparty models/sense-voice-small-q4_k.gguf
git commit -m "feat: add SenseVoice.cpp submodule + GGUF model"
```

---

### Task 2: Create C wrapper (header + implementation)

**Files:**
- Create: `core/src/ai/sense_voice_capi.h`
- Create: `core/src/ai/sense_voice_capi.cc`

- [ ] **Step 1: Create `core/src/ai/sense_voice_capi.h`**

```c
#ifndef AURA_SENSE_VOICE_CAPI_H
#define AURA_SENSE_VOICE_CAPI_H

#ifdef __cplusplus
extern "C" {
#endif

void* aura_sense_voice_load(const char* model_path, int use_gpu);
int aura_sense_voice_transcribe(void* ctx, const float* pcm_data,
                                int num_samples, char* out_text,
                                int max_text_len);
void aura_sense_voice_free(void* ctx);

#ifdef __cplusplus
}
#endif

#endif
```

- [ ] **Step 2: Create `core/src/ai/sense_voice_capi.cc`**

```cpp
#include "sense_voice_capi.h"
#include "common.h"
#include "sense-voice.h"
#include <cstring>

struct SenseVoiceHandle {
    sense_voice_context* ctx;
};

extern "C" {

void* aura_sense_voice_load(const char* model_path, int use_gpu) {
    sense_voice_context_params params = sense_voice_context_default_params();
    params.use_gpu = use_gpu;
    params.flash_attn = false;
    params.use_itn = true;

    sense_voice_context* ctx = sense_voice_small_init_from_file_with_params(model_path, params);
    if (!ctx) return nullptr;

    ctx->language_id = sense_voice_lang_id("auto");

    auto* handle = new SenseVoiceHandle{ctx};
    return handle;
}

int aura_sense_voice_transcribe(void* handle_ptr, const float* pcm_data,
                                 int num_samples, char* out_text,
                                 int max_text_len) {
    auto* handle = static_cast<SenseVoiceHandle*>(handle_ptr);
    if (!handle || !handle->ctx) return -1;

    std::vector<double> pcmf32(pcm_data, pcm_data + num_samples);

    sense_voice_full_params wparams = sense_voice_full_default_params(
        SENSE_VOICE_SAMPLING_GREEDY);
    wparams.n_threads = 4;
    wparams.language = "auto";

    handle->ctx->state->duration = float(num_samples) / 16000.0f;

    int ret = sense_voice_full_parallel(handle->ctx, wparams, pcmf32,
                                        num_samples, 1);
    if (ret != 0) return ret;

    std::string result;
    for (size_t i = 4; i < handle->ctx->state->ids.size(); i++) {
        int id = handle->ctx->state->ids[i];
        if (i > 0 && handle->ctx->state->ids[i - 1] == id) continue;
        if (id > 0) {
            result += handle->ctx->vocab.id_to_token[id];
        }
    }

    strncpy(out_text, result.c_str(), max_text_len - 1);
    out_text[max_text_len - 1] = '\0';
    return 0;
}

void aura_sense_voice_free(void* handle_ptr) {
    auto* handle = static_cast<SenseVoiceHandle*>(handle_ptr);
    if (handle) {
        if (handle->ctx) {
            sense_voice_free(handle->ctx);
        }
        delete handle;
    }
}

}
```

- [ ] **Step 3: Commit**

```bash
git add core/src/ai/sense_voice_capi.h core/src/ai/sense_voice_capi.cc
git commit -m "feat: add C wrapper for SenseVoice.cpp API"
```

---

### Task 3: Update build system (build.rs + Cargo.toml)

**Files:**
- Create: `core/build.rs`
- Modify: `core/Cargo.toml`

- [ ] **Step 1: Set up `core/build.rs`**

```rust
fn main() {
    // 1. Build SenseVoice.cpp + ggml via cmake
    let dst = cmake::Config::new("3rdparty/SenseVoice.cpp")
        .very_verbose(true)
        .build();

    println!("cargo:rustc-link-search=native={}/lib", dst.display());
    println!("cargo:rustc-link-lib=static=sense-voice-core");
    println!("cargo:rustc-link-lib=static=ggml");

    // 2. Build C wrapper via cc crate
    let mut build = cc::Build::new();
    build.cpp(true)
        .file("src/ai/sense_voice_capi.cc")
        .include("3rdparty/SenseVoice.cpp/sense-voice/csrc")
        .include("3rdparty/SenseVoice.cpp/ggml/include")
        .compile("aura_sense_voice_capi");

    println!("cargo:rerun-if-changed=src/ai/sense_voice_capi.cc");
    println!("cargo:rerun-if-changed=src/ai/sense_voice_capi.h");
    println!("cargo:rerun-if-changed=3rdparty/SenseVoice.cpp");
}
```

- [ ] **Step 2: Add `cmake` to `core/Cargo.toml` build-dependencies**

```toml
[build-dependencies]
cmake = "0.1"
```

- [ ] **Step 4: Verify cmake build works**

Run: `cargo check -p aura_core` (first run will build SenseVoice.cpp via cmake — takes ~2-3 minutes)
Expected: Clean compile

- [ ] **Step 5: Commit**

```bash
git add core/build.rs core/Cargo.toml
git commit -m "feat: add build.rs with cmake build for SenseVoice.cpp + ggml"
```

---

### Task 4: Create Rust FFI bindings + safe SenseVoiceEngine

**Files:**
- Create: `core/src/ai/sense_voice_ffi.rs`
- Modify: `core/src/ai/sense_voice.rs`
- Modify: `core/src/ai/mod.rs`

**Interfaces:**
- Consumes: C wrapper functions from `sense_voice_capi.h`
- Produces: `SenseVoiceEngine` struct with `transcribe()` method

- [ ] **Step 1: Create `core/src/ai/sense_voice_ffi.rs`**

```rust
//! Unsafe FFI declarations for the SenseVoice C wrapper.
//! Functions are defined in sense_voice_capi.cc.

use std::ffi::c_char;
use std::os::raw::c_int;
use std::ptr;

extern "C" {
    fn aura_sense_voice_load(
        model_path: *const c_char,
        use_gpu: c_int,
    ) -> *mut std::ffi::c_void;

    fn aura_sense_voice_transcribe(
        ctx: *mut std::ffi::c_void,
        pcm_data: *const f32,
        num_samples: c_int,
        out_text: *mut c_char,
        max_text_len: c_int,
    ) -> c_int;

    fn aura_sense_voice_free(ctx: *mut std::ffi::c_void);
}
```

- [ ] **Step 2: Register module in `core/src/ai/mod.rs`**

Add after `pub mod sensevoice;`:
```rust
pub mod sense_voice_ffi;
```

- [ ] **Step 3: Rewrite `core/src/ai/sense_voice.rs`**

Replace the placeholder stub with a real implementation:

```rust
use anyhow::{Context, Result};
use std::ffi::CString;
use std::os::raw::c_int;
use std::ptr;
use std::sync::Mutex;

use super::sense_voice_ffi;

const MAX_TEXT_LEN: usize = 4096;

pub struct SenseVoiceEngine {
    handle: Mutex<*mut std::ffi::c_void>,
}

unsafe impl Send for SenseVoiceEngine {}
unsafe impl Sync for SenseVoiceEngine {}

impl SenseVoiceEngine {
    pub fn new(model_path: &str) -> Result<Self> {
        let c_path = CString::new(model_path)
            .context("Model path contains null byte")?;

        let handle = unsafe {
            sense_voice_ffi::aura_sense_voice_load(c_path.as_ptr(), 0)
        };

        if handle.is_null() {
            anyhow::bail!("Failed to load SenseVoice model from: {}", model_path);
        }

        log::info!("SenseVoice model loaded from: {}", model_path);
        Ok(Self { handle: Mutex::new(handle) })
    }

    pub fn transcribe(&self, pcm_data: &[f32]) -> Result<String> {
        let handle = self.handle.lock().map_err(|e| {
            anyhow::anyhow!("SenseVoice mutex poisoned: {}", e)
        })?;

        let mut out_buf = vec![0u8; MAX_TEXT_LEN];

        let ret = unsafe {
            sense_voice_ffi::aura_sense_voice_transcribe(
                *handle,
                pcm_data.as_ptr(),
                pcm_data.len() as c_int,
                out_buf.as_mut_ptr() as *mut i8,
                MAX_TEXT_LEN as c_int,
            )
        };

        if ret != 0 {
            anyhow::bail!("SenseVoice transcribe failed with error: {}", ret);
        }

        let text = String::from_utf8_lossy(&out_buf)
            .trim_end_matches('\0')
            .to_string();

        Ok(text)
    }
}

impl Drop for SenseVoiceEngine {
    fn drop(&mut self) {
        if let Ok(handle) = self.handle.lock() {
            if !handle.is_null() {
                unsafe {
                    sense_voice_ffi::aura_sense_voice_free(*handle);
                }
                log::info!("SenseVoice model freed");
            }
        }
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p aura_core`
Expected: Clean compile (first run with cmake build, ~2-3 min)

- [ ] **Step 5: Commit**

```bash
git add core/src/ai/sense_voice_ffi.rs core/src/ai/sense_voice.rs core/src/ai/mod.rs
git commit -m "feat: add Rust FFI bindings and safe SenseVoiceEngine wrapper"
```

---

### Task 5: Integrate SenseVoiceEngine into pipeline

**Files:**
- Modify: `core/src/ffi/pipeline.rs`
- Modify: `core/src/ffi/exports.rs`

**Interfaces:**
- Produces: Updated `PipelineState::start(vad_model, asr_model)`, `run_pipeline` with ASR calls

- [ ] **Step 1: Update `PipelineState` in `pipeline.rs`**

Add `sense_voice: Option<SenseVoiceEngine>` field and accept `asr_model_path` in `start()`:

Add import at top:
```rust
use crate::ai::sense_voice::SenseVoiceEngine;
```

Update struct:
```rust
pub struct PipelineState {
    pub capturer: AudioCapturer,
    pub pipeline_thread: JoinHandle<()>,
    pub stop_signal: Arc<AtomicBool>,
    pub ring_buffer: Arc<AudioRingBuffer>,
    sense_voice: Option<SenseVoiceEngine>,
}
```

Update `start()` signature and body:
```rust
pub fn start(
    target_pid: u32,
    include_tree: bool,
    vad_model_path: &str,
    asr_model_path: &str,
) -> anyhow::Result<Self> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let ring_buffer = Arc::new(AudioRingBuffer::new(16_000, 5.0));

    let config = CaptureConfig {
        target_pid,
        include_process_tree: include_tree,
    };
    let mut capturer = AudioCapturer::new(config, Arc::clone(&ring_buffer));
    capturer.start()?;

    // Load SenseVoice ASR model (optional — engine may be Gemini)
    let sense_voice = if !asr_model_path.is_empty() {
        match SenseVoiceEngine::new(asr_model_path) {
            Ok(engine) => {
                log::info!("SenseVoice ASR engine loaded");
                Some(engine)
            }
            Err(e) => {
                log::warn!("Failed to load SenseVoice ASR engine: {:#}", e);
                None
            }
        }
    } else {
        None
    };

    let thread_stop = Arc::clone(&stop_signal);
    let thread_rb = Arc::clone(&ring_buffer);
    let model_path_owned = vad_model_path.to_owned();

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
        sense_voice,
    })
}
```

Actually wait — `SenseVoiceEngine` is not `Send` because it contains `Mutex<*mut c_void>`. The `*mut c_void` is not `Send` by default. I need to add `unsafe impl Send` for `SenseVoiceEngine`, which I already did in the sense_voice.rs code above.

But `PipelineState` needs to be `Send` to be stored in a `Mutex<Option<PipelineState>>`. Since `SenseVoiceEngine` is `Send`, this should work.

- [ ] **Step 2: Pass `sense_voice` to pipeline thread**

The challenge is that `run_pipeline` currently runs in a separate thread, and `SenseVoiceEngine` needs to be accessible from that thread. Options:
A. Wrap `SenseVoiceEngine` in `Arc<Mutex<>>` and pass to thread
B. Move it into the pipeline thread

Since ASR inference only happens on the pipeline thread, option A is cleanest:

```rust
let sense_voice = if !asr_model_path.is_empty() {
    match SenseVoiceEngine::new(asr_model_path) {
        Ok(engine) => {
            log::info!("SenseVoice ASR engine loaded");
            Some(Arc::new(engine))
        }
        Err(e) => {
            log::warn!("Failed to load SenseVoice ASR engine: {:#}", e);
            None
        }
    }
} else {
    None
};
```

But wait, `PipelineState` needs to own the Arc for cleanup. And the thread needs a clone.

Actually, the simplest approach is to use `Arc<SenseVoiceEngine>`:

```rust
pub struct PipelineState {
    pub capturer: AudioCapturer,
    pub pipeline_thread: JoinHandle<()>,
    pub stop_signal: Arc<AtomicBool>,
    pub ring_buffer: Arc<AudioRingBuffer>,
    sense_voice: Option<Arc<SenseVoiceEngine>>,
}
```

And pass `Arc::clone()` to the thread.

Actually, this gets complicated because the SenseVoiceEngine itself already uses internal Mutex. Using Arc on top is fine.

Let me simplify: put the `Arc<SenseVoiceEngine>` in PipelineState, clone the Arc for the thread:

Update `start()`:

```rust
pub fn start(
    target_pid: u32,
    include_tree: bool,
    vad_model_path: &str,
    asr_model_path: &str,
) -> anyhow::Result<Self> {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let ring_buffer = Arc::new(AudioRingBuffer::new(16_000, 5.0));

    let config = CaptureConfig {
        target_pid,
        include_process_tree: include_tree,
    };
    let mut capturer = AudioCapturer::new(config, Arc::clone(&ring_buffer));
    capturer.start()?;

    let sense_voice = if !asr_model_path.is_empty() {
        match SenseVoiceEngine::new(asr_model_path) {
            Ok(engine) => {
                log::info!("SenseVoice ASR engine loaded");
                Some(Arc::new(engine))
            }
            Err(e) => {
                log::warn!("Failed to load SenseVoice ASR engine: {:#}", e);
                None
            }
        }
    } else {
        None
    };

    let thread_sv = sense_voice.as_ref().map(Arc::clone);

    let thread_stop = Arc::clone(&stop_signal);
    let thread_rb = Arc::clone(&ring_buffer);
    let model_path_owned = vad_model_path.to_owned();

    let pipeline_thread = thread::Builder::new()
        .name("aura-pipeline".into())
        .spawn(move || {
            if let Err(e) = run_pipeline(thread_stop, thread_rb, &model_path_owned, thread_sv) {
                log::error!("Pipeline worker exited with error: {:#}", e);
            }
        })
        .expect("Failed to spawn pipeline thread");

    Ok(Self {
        capturer,
        pipeline_thread,
        stop_signal,
        ring_buffer,
        sense_voice,
    })
}
```

Update `run_pipeline` signature:

```rust
fn run_pipeline(
    stop_signal: Arc<AtomicBool>,
    ring_buffer: Arc<AudioRingBuffer>,
    model_path: &str,
    sense_voice: Option<Arc<SenseVoiceEngine>>,
) -> anyhow::Result<()> {
```

Add ASR call in the chunk handling part inside `run_pipeline`:

```rust
if let Some(chunk) = state_machine.feed(&vad_result, &frame) {
    let duration_ms = (chunk.samples.len() as u64 * 1000) / 16_000;
    let latency = pipeline_start.elapsed().as_millis() as i32;

    let (text, is_provisional) = match chunk.chunk_type {
        ChunkType::Provisional => {
            (format!("[~] {}ms speech...", duration_ms), true)
        }
        ChunkType::Final | ChunkType::HardCut => {
            if let Some(ref sv) = sense_voice {
                match sv.transcribe(&chunk.samples) {
                    Ok(asr_text) => {
                        (asr_text, false)
                    }
                    Err(e) => {
                        log::warn!("ASR error: {:#}", e);
                        (format!("[✓] {}ms (ASR failed)", duration_ms), false)
                    }
                }
            } else {
                (format!("[✓] {}ms sentence", duration_ms), false)
            }
        }
    };

    super::exports::emit_translation(&text, is_provisional, latency);
}
```

- [ ] **Step 3: Update `exports.rs` to pass ASR model path**

Add static for ASR model path:

```rust
static ASR_MODEL_PATH: OnceLock<Mutex<String>> = OnceLock::new();

fn asr_model_path_slot() -> &'static Mutex<String> {
    ASR_MODEL_PATH.get_or_init(|| Mutex::new(String::new()))
}
```

Update `aura_core_init`:
```rust
pub unsafe extern "C" fn aura_core_init() -> c_int {
    let _ = env_logger::builder().is_test(false).try_init();
    callback_slot();
    model_path_slot();
    asr_model_path_slot();
    pipeline_slot();
    log::info!("aura_core_init() called");
    0
}
```

Add new export:
```rust
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_asr_model_path(path: *const c_char) {
    if path.is_null() {
        return;
    }
    let path_str = CStr::from_ptr(path).to_string_lossy().into_owned();
    if let Ok(mut slot) = asr_model_path_slot().lock() {
        *slot = path_str;
    }
    log::info!("ASR model path updated");
}
```

Update `aura_core_start` to pass ASR path:
```rust
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

    let vad_path = model_path_slot()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_else(|_| "assets/silero_vad.onnx".to_string());

    let asr_path = asr_model_path_slot()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();

    match PipelineState::start(target_pid, true, &vad_path, &asr_path) {
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
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p aura_core`
Expected: Clean compile

- [ ] **Step 5: Run existing tests**

Run: `cargo test -p aura_core`
Expected: 7/7 tests pass

- [ ] **Step 6: Commit**

```bash
git add core/src/ffi/pipeline.rs core/src/ffi/exports.rs
git commit -m "feat: integrate SenseVoice ASR engine into pipeline"
```

---

### Task 6: Add C# bindings for ASR model path

**Files:**
- Modify: `ui/Aura/Interop/AuraCoreBinding.cs`
- Modify: `ui/Aura/App.xaml.cs`
- Modify: `scripts/build_all.ps1`

- [ ] **Step 1: Add DllImport to `AuraCoreBinding.cs`**

Add after existing `SetModelPath`:
```csharp
[DllImport(DllName, CallingConvention = CallingConvention.Cdecl)]
private static extern void aura_core_set_asr_model_path(
    [MarshalAs(UnmanagedType.LPUTF8Str)] string path);
```

Add public wrapper:
```csharp
public static void SetAsrModelPath(string path) => aura_core_set_asr_model_path(path);
```

- [ ] **Step 2: Set ASR model path in `App.xaml.cs`**

Add after VAD model path setup:
```csharp
var asrModelPath = System.IO.Path.Combine(
    AppDomain.CurrentDomain.BaseDirectory,
    "sense-voice-small-q4_k.gguf");
Interop.AuraCoreBinding.SetAsrModelPath(asrModelPath);
```

- [ ] **Step 3: Update `build_all.ps1` to copy ASR model**

Add after VAD model copy:
```powershell
$asrModelSource = "$ProjectRoot\models\sense-voice-small-q4_k.gguf"
$asrModelDest = "$dllDest\sense-voice-small-q4_k.gguf"
if (Test-Path $asrModelSource) {
    Copy-Item $asrModelSource $asrModelDest -Force
    Write-Host "  ✓ sense-voice-small-q4_k.gguf → $dllDest" -ForegroundColor Green
} else {
    Write-Host "  [!] sense-voice-small-q4_k.gguf not found at $asrModelSource" -ForegroundColor Yellow
}
```

- [ ] **Step 4: Commit**

```bash
git add ui/Aura/Interop/AuraCoreBinding.cs ui/Aura/App.xaml.cs scripts/build_all.ps1
git commit -m "feat(cs): add SetAsrModelPath and copy ASR model in build"
```

---

## Self-Review

- **Spec coverage:**
  - C wrapper API ✓ (Task 2)
  - Build system ✓ (Task 3)
  - Rust FFI + safe wrapper ✓ (Task 4)
  - Pipeline integration ✓ (Task 5)
  - Model path separation ✓ (Task 5 + 6)
  - Sync blocking documented ✓ (spec already updated)

- **Placeholder scan:** No TBD, TODO, or incomplete sections. All code is complete.

- **Type consistency:**
  - `SenseVoiceEngine::new(path)` → `SenseVoiceEngine`
  - `SenseVoiceEngine::transcribe(&[f32])` → `Result<String>`
  - `PipelineState::start(pid, tree, vad_path, asr_path)` → consistent across Tasks 5 and 6
  - `aura_core_set_asr_model_path(path)` → C# `SetAsrModelPath(path)` → correct binding

- **Dependency order:** Task 1 → 2 → 3 → 4 → 5 → 6 (each builds on previous)
