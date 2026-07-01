//! Audio capture and buffering module.
//!
//! Responsible for:
//! - WASAPI process-level loopback capture (targeting Discord / TeamSpeak PID tree)
//! - Lock-free ring buffer bridging the capture callback thread and the VAD processing thread
//! - Real-time resampling from device sample rate (typically 48 kHz) to 16 kHz for AI models

pub mod capture;
pub mod resampler;
pub mod ring_buffer;

pub use capture::AudioCapturer;
pub use resampler::Resampler;
pub use ring_buffer::AudioRingBuffer;
