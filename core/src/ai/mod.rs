//! AI translation engine module.
//!
//! Provides a unified [`TranslationEngine`] trait that abstracts over different
//! AI backends (cloud or local).  New engines can be added by implementing
//! this trait — the rest of the pipeline doesn't care about the backend.
//!
//! ## Current implementations
//! - [`gemini_live`] – Google Gemini 2.5 Flash Native Audio Live API (cloud, default)
//! - [`sensevoice`] – SenseVoice-Small via ggml (local, optional)

pub mod gemini_live;
pub mod sensevoice;
pub mod translator;

pub use translator::{TranslationEngine, TranslationRequest, TranslationResult};
