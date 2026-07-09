use std::ffi::CString;
use std::os::raw::c_int;
use std::sync::Mutex;

use anyhow::{Context, Result};

use super::sense_voice_ffi;

const MAX_TEXT_LEN: usize = 4096;

pub struct SenseVoiceEngine {
    handle: Mutex<*mut std::ffi::c_void>,
    num_threads: c_int,
}

unsafe impl Send for SenseVoiceEngine {}
unsafe impl Sync for SenseVoiceEngine {}

impl SenseVoiceEngine {
    pub fn new(model_path: &str) -> Result<Self> {
        Self::with_threads(model_path, 0)
    }

    pub fn with_threads(model_path: &str, n_threads: c_int) -> Result<Self> {
        let c_path = CString::new(model_path).context("Model path contains null byte")?;

        let handle = unsafe { sense_voice_ffi::aura_sense_voice_load(c_path.as_ptr(), 0) };

        if handle.is_null() {
            anyhow::bail!("Failed to load SenseVoice model from: {}", model_path);
        }

        let threads = if n_threads <= 0 { 4 } else { n_threads };
        log::info!("SenseVoice model loaded from: {}  ({} threads)", model_path, threads);
        Ok(Self {
            handle: Mutex::new(handle),
            num_threads: threads,
        })
    }

    pub fn transcribe(&self, pcm_data: &[f32]) -> Result<String> {
        let handle = self
            .handle
            .lock()
            .map_err(|e| anyhow::anyhow!("SenseVoice mutex poisoned: {}", e))?;

        let mut out_buf = vec![0u8; MAX_TEXT_LEN];

        let ret = unsafe {
            sense_voice_ffi::aura_sense_voice_transcribe(
                *handle,
                pcm_data.as_ptr(),
                pcm_data.len() as c_int,
                out_buf.as_mut_ptr() as *mut i8,
                MAX_TEXT_LEN as c_int,
                self.num_threads,
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
