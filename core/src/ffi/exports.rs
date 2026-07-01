//! C ABI exports for consumption by C# via P/Invoke (DllImport).
//!
//! These functions form the contract between the Rust core DLL and the C# UI.
//! All exported functions use `extern "C"` and `#[no_mangle]` to ensure stable
//! symbol names.
//!
//! # Lifecycle
//! 1. C# calls `aura_core_init()` to initialise the pipeline
//! 2. C# calls `aura_core_register_callback()` to set the translation text callback
//! 3. C# calls `aura_core_start(pid)` to begin capturing a target process
//! 4. When translation text arrives, the registered callback is invoked from Rust
//! 5. C# calls `aura_core_stop()` to halt the pipeline
//! 6. C# calls `aura_core_destroy()` to free all resources

use std::ffi::{c_char, c_int, CStr};

/// Callback function pointer type for delivering translation results to C#.
///
/// # Parameters
/// * `text`           – UTF-8 null-terminated translated text
/// * `is_provisional` – 1 if provisional (partial), 0 if final
/// * `latency_ms`     – End-to-end latency in milliseconds
pub type TranslationCallback =
    unsafe extern "C" fn(text: *const c_char, is_provisional: c_int, latency_ms: c_int);

// ── Global state (behind a Mutex for safety) ───────────────────────────

static mut CALLBACK: Option<TranslationCallback> = None;

// ── Exported functions ─────────────────────────────────────────────────

/// Initialise the Aura core pipeline.
///
/// Must be called once before any other function.
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_init() -> c_int {
    // TODO: Phase 4 implementation
    // 1. Initialise logging
    // 2. Create the tokio runtime
    // 3. Load configuration
    // 4. Load Silero VAD model
    log::info!("aura_core_init() called");
    0
}

/// Register a callback function that will be called whenever
/// a translation result (provisional or final) is ready.
#[no_mangle]
pub unsafe extern "C" fn aura_core_register_callback(cb: TranslationCallback) {
    CALLBACK = Some(cb);
    log::info!("Translation callback registered");
}

/// Start the audio capture and translation pipeline targeting the given PID.
///
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_start(target_pid: u32) -> c_int {
    log::info!("aura_core_start(pid={})", target_pid);

    // TODO: Phase 4 implementation
    // 1. Create AudioCapturer with target_pid
    // 2. Wire up: Capture → RingBuffer → VAD → StateMachine → AI Engine
    // 3. Start the capture loop
    // 4. Start the VAD processing loop
    // 5. On translation result, invoke CALLBACK

    0
}

/// Stop the audio capture and translation pipeline.
///
/// Returns 0 on success, -1 on error.
#[no_mangle]
pub unsafe extern "C" fn aura_core_stop() -> c_int {
    log::info!("aura_core_stop()");
    // TODO: Stop all background tasks, flush buffers
    0
}

/// Destroy the Aura core pipeline and free all resources.
///
/// After this call, no other function may be called without re-init.
#[no_mangle]
pub unsafe extern "C" fn aura_core_destroy() {
    log::info!("aura_core_destroy()");
    CALLBACK = None;
    // TODO: Drop all owned resources, shut down tokio runtime
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

    // TODO: Switch the active TranslationEngine implementation
    match name.as_ref() {
        "gemini" | "sensevoice" => 0,
        _ => {
            log::error!("Unknown engine: {}", name);
            -1
        }
    }
}

/// Set the API key for cloud-based engines.
///
/// Pass a null-terminated UTF-8 API key string.
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_api_key(api_key: *const c_char) {
    if api_key.is_null() {
        return;
    }
    let _key = CStr::from_ptr(api_key).to_string_lossy();
    log::info!("API key updated (length={})", _key.len());
    // TODO: Forward to the active engine configuration
}

/// Set the target translation language (ISO 639-1 code).
#[no_mangle]
pub unsafe extern "C" fn aura_core_set_target_lang(lang: *const c_char) {
    if lang.is_null() {
        return;
    }
    let lang_str = CStr::from_ptr(lang).to_string_lossy();
    log::info!("Target language set to: {}", lang_str);
    // TODO: Update translation engine target language
}
