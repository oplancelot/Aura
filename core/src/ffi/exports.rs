use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::{Mutex, OnceLock};

use super::pipeline::PipelineState;

/// Per-chunk timing metrics passed through FFI to C# for unified CSV logging.
/// Zero I/O on the Rust hot path — metrics are computed from Instant deltas
/// and shipped in-band alongside the translation text.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct TranslationMetrics {
    pub audio_duration_ms: u32,  // length of the audio chunk in ms
    pub asr_inference_ms: u32,   // [T5] pure ASR inference time
    pub rust_total_ms: u32,      // [T4→T6] chunk ready → callback
}

pub type TranslationCallback = unsafe extern "C" fn(
    text: *const c_char,
    is_provisional: c_int,
    latency_ms: c_int,
    metrics: TranslationMetrics,
);

static CALLBACK: OnceLock<Mutex<Option<TranslationCallback>>> = OnceLock::new();
static MODEL_PATH: OnceLock<Mutex<String>> = OnceLock::new();
static ASR_MODEL_PATH: OnceLock<Mutex<String>> = OnceLock::new();
static PIPELINE: OnceLock<Mutex<Option<PipelineState>>> = OnceLock::new();

fn callback_slot() -> &'static Mutex<Option<TranslationCallback>> {
    CALLBACK.get_or_init(|| Mutex::new(None))
}

fn model_path_slot() -> &'static Mutex<String> {
    MODEL_PATH.get_or_init(|| Mutex::new("assets/silero_vad.onnx".to_string()))
}

fn asr_model_path_slot() -> &'static Mutex<String> {
    ASR_MODEL_PATH.get_or_init(|| Mutex::new(String::new()))
}

fn pipeline_slot() -> &'static Mutex<Option<PipelineState>> {
    PIPELINE.get_or_init(|| Mutex::new(None))
}

pub(crate) fn emit_translation(text: &str, is_provisional: bool, latency_ms: i32, metrics: TranslationMetrics) {
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
            metrics,
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn aura_core_init() -> c_int {
    let _ = env_logger::builder().is_test(false).try_init();
    callback_slot();
    model_path_slot();
    asr_model_path_slot();
    pipeline_slot();
    log::info!("aura_core_init() called");
    0
}

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

#[no_mangle]
pub unsafe extern "C" fn aura_core_register_callback(cb: TranslationCallback) {
    if let Ok(mut slot) = callback_slot().lock() {
        *slot = Some(cb);
    }
    log::info!("Translation callback registered");
}

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

#[no_mangle]
pub unsafe extern "C" fn aura_core_stop() -> c_int {
    log::info!("aura_core_stop()");

    let mut pipeline = match pipeline_slot().lock() {
        Ok(guard) => guard,
        Err(_) => return -1,
    };

    if let Some(state) = pipeline.take() {
        if let Err(e) = state.stop() {
            log::error!("Pipeline stop error: {:#}", e);
            return -1;
        }
    }

    0
}

#[no_mangle]
pub unsafe extern "C" fn aura_core_destroy() {
    log::info!("aura_core_destroy()");
    let _ = aura_core_stop();
    if let Ok(mut slot) = callback_slot().lock() {
        *slot = None;
    }
}

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

#[no_mangle]
pub unsafe extern "C" fn aura_core_set_api_key(api_key: *const c_char) {
    if api_key.is_null() {
        return;
    }
    let _key = CStr::from_ptr(api_key).to_string_lossy();
    log::info!("API key updated (length={})", _key.len());
}

#[no_mangle]
pub unsafe extern "C" fn aura_core_set_target_lang(lang: *const c_char) {
    if lang.is_null() {
        return;
    }
    let lang_str = CStr::from_ptr(lang).to_string_lossy();
    log::info!("Target language set to: {}", lang_str);
}
