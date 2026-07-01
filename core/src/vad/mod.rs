//! Voice Activity Detection (VAD) and audio chunking module.
//!
//! Responsible for:
//! - Running Silero VAD inference on 32ms frames (512 samples @ 16 kHz)
//! - Maintaining a chunking state machine that produces Provisional and Final audio chunks
//! - Enforcing hard-cut limits to prevent OOM on extremely long utterances

pub mod silero;
pub mod state_machine;

pub use silero::SileroVad;
pub use state_machine::{ChunkingStateMachine, AudioChunk, ChunkType};
