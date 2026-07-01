//! Google Gemini 2.5 Flash Native Audio Live API client.
//!
//! Communicates over a persistent WebSocket connection, streaming raw PCM audio
//! and receiving translated text in real-time.  This is the **default** cloud
//! translation engine.
//!
//! # Protocol
//! - Transport: WebSocket (wss://)
//! - Input: 16 kHz PCM f32 audio chunks, base64-encoded
//! - Output: Streaming text responses
//! - Latency: ~200ms first-token

use anyhow::Result;
use std::time::Instant;

use super::translator::{TranslationEngine, TranslationRequest, TranslationResult};

/// Configuration for the Gemini Live API client.
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    /// API key for authentication.
    pub api_key: String,
    /// Model identifier (e.g. "gemini-2.5-flash").
    pub model: String,
    /// System prompt instructing the model to translate.
    pub system_prompt: String,
    /// Target language for translation.
    pub target_lang: String,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "gemini-2.5-flash".to_string(),
            system_prompt: concat!(
                "You are a real-time voice translator for gamers. ",
                "Translate the spoken audio to the target language. ",
                "Output ONLY the translated text, no explanations. ",
                "Preserve gaming terminology and proper nouns."
            ).to_string(),
            target_lang: "zh".to_string(),
        }
    }
}

/// Gemini Live API translation engine.
pub struct GeminiLiveEngine {
    config: GeminiConfig,
    is_connected: bool,
    // TODO: WebSocket connection handle
    // ws_stream: Option<...>,
}

impl GeminiLiveEngine {
    pub fn new(config: GeminiConfig) -> Self {
        Self {
            config,
            is_connected: false,
        }
    }
}

impl TranslationEngine for GeminiLiveEngine {
    fn name(&self) -> &str {
        "Gemini 2.5 Flash Native Audio"
    }

    async fn initialize(&mut self) -> Result<()> {
        log::info!("Connecting to Gemini Live API (model: {})", self.config.model);

        // TODO: Phase 3 implementation
        // 1. Build WebSocket URL with API key
        // 2. Establish wss:// connection via tokio-tungstenite
        // 3. Send initial setup message with system_prompt and target_lang
        // 4. Spawn background task for reading responses

        self.is_connected = true;
        Ok(())
    }

    async fn translate(&self, request: TranslationRequest) -> Result<TranslationResult> {
        let start = Instant::now();

        // TODO: Phase 3 implementation
        // 1. Encode audio samples as base64 PCM
        // 2. Send audio message over WebSocket
        // 3. Await text response (with timeout)
        // 4. Parse and return translated text

        let latency = start.elapsed().as_millis() as u64;

        Ok(TranslationResult {
            detected_lang: None,
            source_text: String::new(),
            translated_text: "[Gemini placeholder]".to_string(),
            latency_ms: latency,
            is_provisional: request.is_provisional,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        log::info!("Disconnecting from Gemini Live API");
        // TODO: Close WebSocket gracefully
        self.is_connected = false;
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.is_connected
    }
}
