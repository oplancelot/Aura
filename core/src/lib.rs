//! # Aura Core
//!
//! Real-time game voice translation pipeline.
//!
//! ## Architecture
//! ```text
//! WASAPI Capture → Ring Buffer → Silero VAD → Chunking State Machine → AI Engine → FFI Callback
//! ```
//!
//! ## Modules
//! - [`audio`] – Process-level audio capture via WASAPI loopback
//! - [`vad`]   – Voice Activity Detection (Silero) and chunking state machine
//! - [`ai`]    – Translation engines (cloud Gemini / local SenseVoice)
//! - [`ffi`]   – C ABI exports for C# P/Invoke consumption

pub mod audio;
pub mod vad;
pub mod ai;
pub mod ffi;
