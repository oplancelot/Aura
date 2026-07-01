//! Unified translation engine trait.
//!
//! All AI backends (Gemini, SenseVoice, future providers) implement this trait
//! so the pipeline can swap engines at runtime via configuration.

use anyhow::Result;
use std::fmt;

/// A request to translate an audio chunk.
#[derive(Debug, Clone)]
pub struct TranslationRequest {
    /// PCM audio samples (f32, 16 kHz, mono).
    pub audio: Vec<f32>,
    /// Source language hint (ISO 639-1, e.g. "en", "ja"). `None` for auto-detect.
    pub source_lang: Option<String>,
    /// Target language (ISO 639-1, e.g. "zh", "en").
    pub target_lang: String,
    /// Whether this is a provisional (partial) or final chunk.
    pub is_provisional: bool,
}

/// The result of a translation.
#[derive(Debug, Clone)]
pub struct TranslationResult {
    /// Detected source language (ISO 639-1).
    pub detected_lang: Option<String>,
    /// Recognised source text (original language).
    pub source_text: String,
    /// Translated text in the target language.
    pub translated_text: String,
    /// Processing latency in milliseconds (from request submission to result).
    pub latency_ms: u64,
    /// Whether this result corresponds to a provisional chunk.
    pub is_provisional: bool,
}

/// Trait that all translation engine backends must implement.
///
/// Engines can be stateful (e.g. maintaining a WebSocket connection) or
/// stateless (e.g. running a local model per-chunk).
#[allow(async_fn_in_trait)]
pub trait TranslationEngine: Send + Sync {
    /// Human-readable name of this engine (e.g. "Gemini 2.5 Flash").
    fn name(&self) -> &str;

    /// Initialize the engine (connect to server, load model, etc.).
    async fn initialize(&mut self) -> Result<()>;

    /// Translate an audio chunk.
    ///
    /// Implementations should measure and report latency in the result.
    async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult>;

    /// Gracefully shut down the engine (close connections, unload model).
    async fn shutdown(&mut self) -> Result<()>;

    /// Check if the engine is ready to accept requests.
    fn is_ready(&self) -> bool;
}

impl fmt::Display for TranslationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_provisional {
            write!(f, "[~] {}", self.translated_text)
        } else {
            write!(f, "[✓] {}", self.translated_text)
        }
    }
}
