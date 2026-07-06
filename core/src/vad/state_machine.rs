//! Chunking state machine for intelligent audio segmentation.
//!
//! This state machine sits between the VAD and the AI engine, deciding *when*
//! to submit audio chunks for translation.  It implements three rules:
//!
//! 1. **Short-sentence close** – VAD detects silence > 800 ms → emit Final Chunk
//! 2. **Long-sentence provisional** – continuous speech > 2s → emit Provisional Chunk
//!    every 200 ms for instant visual feedback
//! 3. **Hard cut** – continuous speech > 28s → force-split with 2s overlap to
//!    prevent GPU OOM

use std::time::{Duration, Instant};

use super::silero::VadResult;

// ── Configuration ──────────────────────────────────────────────────────

/// Timing parameters for the chunking state machine.
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Silence duration to confirm sentence end (default: 800 ms).
    pub silence_close_ms: u64,
    /// Continuous speech threshold before provisional decoding starts (default: 2000 ms).
    pub provisional_start_ms: u64,
    /// Interval between provisional chunk emissions (default: 200 ms).
    pub provisional_interval_ms: u64,
    /// Hard maximum for continuous speech before forced split (default: 28 000 ms).
    pub hard_cut_ms: u64,
    /// Overlap duration at hard-cut boundaries (default: 2000 ms).
    pub hard_cut_overlap_ms: u64,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            silence_close_ms: 1200,
            provisional_start_ms: 2000,
            provisional_interval_ms: 200,
            hard_cut_ms: 28_000,
            hard_cut_overlap_ms: 2000,
        }
    }
}

// ── Chunk types ────────────────────────────────────────────────────────

/// Type of audio chunk emitted by the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkType {
    /// Tentative decode for immediate visual feedback (will be overwritten).
    Provisional,
    /// Definitive decode after confirmed sentence boundary.
    Final,
    /// Forced split due to hard time limit, with overlap data appended.
    HardCut,
}

/// An audio chunk ready for AI translation.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    /// The type of this chunk.
    pub chunk_type: ChunkType,
    /// PCM samples (f32, 16 kHz, mono).
    pub samples: Vec<f32>,
}

// ── State machine ──────────────────────────────────────────────────────

/// Internal state of the chunking FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Waiting for speech onset.
    Silence,
    /// Speech is active, accumulating samples.
    SpeechActive,
    /// Speech active and exceeding provisional threshold – emitting partials.
    ProvisionalEmitting,
}

/// The main chunking state machine.
///
/// Feed it VAD results and audio samples frame-by-frame; it emits
/// [`AudioChunk`]s when appropriate.
pub struct ChunkingStateMachine {
    config: ChunkingConfig,
    state: State,
    /// Accumulated audio samples for the current utterance.
    buffer: Vec<f32>,
    /// Timestamp when current speech segment started.
    speech_start: Option<Instant>,
    /// Timestamp of last provisional emission.
    last_provisional: Option<Instant>,
    /// Number of consecutive silence frames (for hysteresis).
    silence_frame_count: u32,
}

impl ChunkingStateMachine {
    pub fn new(config: ChunkingConfig) -> Self {
        Self {
            config,
            state: State::Silence,
            buffer: Vec::with_capacity(16000 * 30), // Pre-alloc for 30s
            speech_start: None,
            last_provisional: None,
            silence_frame_count: 0,
        }
    }

    /// Feed a VAD result and the corresponding audio frame into the state machine.
    ///
    /// Returns `Some(AudioChunk)` if the state machine decides to emit a chunk,
    /// or `None` if the frame was absorbed without triggering emission.
    pub fn feed(&mut self, vad: &VadResult, frame_samples: &[f32]) -> Option<AudioChunk> {
        match self.state {
            State::Silence => {
                if vad.is_speech {
                    // Speech onset
                    self.state = State::SpeechActive;
                    self.speech_start = Some(Instant::now());
                    self.silence_frame_count = 0;
                    self.buffer.clear();
                    self.buffer.extend_from_slice(frame_samples);
                    log::debug!("State → SpeechActive");
                }
                None
            }

            State::SpeechActive => {
                self.buffer.extend_from_slice(frame_samples);

                if !vad.is_speech {
                    self.silence_frame_count += 1;
                    let silence_ms = self.silence_frame_count as u64 * 32; // 32ms per frame
                    if silence_ms >= self.config.silence_close_ms {
                        return Some(self.emit_final());
                    }
                } else {
                    self.silence_frame_count = 0;
                }

                // Check hard cut
                if let Some(chunk) = self.check_hard_cut() {
                    return Some(chunk);
                }

                // Check provisional threshold
                if let Some(start) = self.speech_start {
                    if start.elapsed() > Duration::from_millis(self.config.provisional_start_ms) {
                        self.state = State::ProvisionalEmitting;
                        self.last_provisional = Some(Instant::now());
                        log::debug!("State → ProvisionalEmitting");
                        return Some(self.emit_provisional());
                    }
                }

                None
            }

            State::ProvisionalEmitting => {
                self.buffer.extend_from_slice(frame_samples);

                if !vad.is_speech {
                    self.silence_frame_count += 1;
                    let silence_ms = self.silence_frame_count as u64 * 32;
                    if silence_ms >= self.config.silence_close_ms {
                        return Some(self.emit_final());
                    }
                } else {
                    self.silence_frame_count = 0;
                }

                // Check hard cut
                if let Some(chunk) = self.check_hard_cut() {
                    return Some(chunk);
                }

                // Emit provisional at interval
                if let Some(last) = self.last_provisional {
                    if last.elapsed()
                        > Duration::from_millis(self.config.provisional_interval_ms)
                    {
                        self.last_provisional = Some(Instant::now());
                        return Some(self.emit_provisional());
                    }
                }

                None
            }
        }
    }

    // ── Internal helpers ────────────────────────────────────────────

    fn emit_provisional(&self) -> AudioChunk {
        log::debug!("Emitting Provisional chunk ({} samples)", self.buffer.len());
        AudioChunk {
            chunk_type: ChunkType::Provisional,
            samples: self.buffer.clone(),
        }
    }

    fn emit_final(&mut self) -> AudioChunk {
        log::debug!("Emitting Final chunk ({} samples)", self.buffer.len());
        let chunk = AudioChunk {
            chunk_type: ChunkType::Final,
            samples: std::mem::take(&mut self.buffer),
        };
        self.reset();
        chunk
    }

    fn check_hard_cut(&mut self) -> Option<AudioChunk> {
        if let Some(start) = self.speech_start {
            if start.elapsed() > Duration::from_millis(self.config.hard_cut_ms) {
                log::warn!("Hard cut triggered at {}s", self.config.hard_cut_ms / 1000);

                let overlap_samples =
                    (self.config.hard_cut_overlap_ms as usize * 16000) / 1000;
                let overlap = if self.buffer.len() > overlap_samples {
                    self.buffer[self.buffer.len() - overlap_samples..].to_vec()
                } else {
                    vec![]
                };

                let chunk = AudioChunk {
                    chunk_type: ChunkType::HardCut,
                    samples: std::mem::take(&mut self.buffer),
                };

                // Seed the next buffer with overlap for context continuity
                self.buffer = overlap;
                self.speech_start = Some(Instant::now());
                self.silence_frame_count = 0;
                self.state = State::SpeechActive;

                return Some(chunk);
            }
        }
        None
    }

    fn reset(&mut self) {
        self.state = State::Silence;
        self.speech_start = None;
        self.last_provisional = None;
        self.silence_frame_count = 0;
        self.buffer.clear();
        log::debug!("State → Silence (reset)");
    }
}
