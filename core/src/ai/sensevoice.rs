//! SenseVoice-Small local inference engine (optional / fallback).
//!
//! Uses SenseVoice.cpp (ggml backend) for fully offline, non-autoregressive
//! speech recognition.  Processes 10s of audio in ~70ms on CPU.
//!
//! # Note
//! This engine performs ASR only (speech → source text).  A separate local
//! MT step may be needed for translation, or the source text can be sent
//! to a lightweight cloud translation API.

use anyhow::Result;
use std::time::Instant;

use super::translator::{TranslationEngine, TranslationRequest, TranslationResult};

/// Configuration for SenseVoice local inference.
#[derive(Debug, Clone)]
pub struct SenseVoiceConfig {
    /// Path to the quantised ggml model file.
    pub model_path: String,
    /// Number of CPU threads to use for inference.
    pub num_threads: u32,
}

impl Default for SenseVoiceConfig {
    fn default() -> Self {
        Self {
            model_path: "models/sensevoice-small-q8.ggml".to_string(),
            num_threads: 4,
        }
    }
}

/// SenseVoice local translation engine.
pub struct SenseVoiceEngine {
    config: SenseVoiceConfig,
    is_loaded: bool,
    // TODO: FFI handle to SenseVoice.cpp context
}

impl SenseVoiceEngine {
    pub fn new(config: SenseVoiceConfig) -> Self {
        Self {
            config,
            is_loaded: false,
        }
    }
}

impl TranslationEngine for SenseVoiceEngine {
    fn name(&self) -> &str {
        "SenseVoice-Small (Local)"
    }

    async fn initialize(&mut self) -> Result<()> {
        log::info!(
            "Loading SenseVoice model from: {} ({} threads)",
            self.config.model_path,
            self.config.num_threads
        );

        // TODO: Phase 3 (optional) implementation
        // 1. Load ggml model via FFI
        // 2. Warm up with a dummy inference

        self.is_loaded = true;
        Ok(())
    }

    async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult> {
        let start = Instant::now();

        // TODO: Phase 3 implementation
        // 1. Prepare audio tensor
        // 2. Run non-autoregressive forward pass
        // 3. Decode output tokens to text
        // 4. (Optional) send source text to lightweight MT API

        let latency = start.elapsed().as_millis() as u64;

        Ok(TranslationResult {
            detected_lang: None,
            source_text: String::new(),
            translated_text: "[SenseVoice placeholder]".to_string(),
            latency_ms: latency,
            is_provisional: request.is_provisional,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        log::info!("Unloading SenseVoice model");
        // TODO: Free ggml context
        self.is_loaded = false;
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.is_loaded
    }
}
