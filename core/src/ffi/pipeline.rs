use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::ai::sensevoice::SenseVoiceEngine;
use crate::audio::capture::{AudioCapturer, CaptureConfig};
use crate::audio::ring_buffer::{AudioConsumer, audio_ring_buffer};
use crate::vad::silero::SileroVad;
use crate::vad::state_machine::{ChunkingConfig, ChunkingStateMachine, ChunkType};

#[allow(dead_code)]
pub enum PipelineState {
    Live {
        capturer: AudioCapturer,
        pipeline_thread: JoinHandle<()>,
        stop_signal: Arc<AtomicBool>,
        sense_voice: Option<Arc<SenseVoiceEngine>>,
    },
    SelfTest {
        pipeline_thread: JoinHandle<()>,
        stop_signal: Arc<AtomicBool>,
    },
}

impl PipelineState {
    pub fn start(
        target_pid: u32,
        include_tree: bool,
        vad_model_path: &str,
        asr_model_path: &str,
    ) -> anyhow::Result<Self> {
        let stop_signal = Arc::new(AtomicBool::new(false));

        if target_pid == 0 {
            log::info!("Starting self-test mode (simulated subtitles)");
            let thread_stop = Arc::clone(&stop_signal);
            let pipeline_thread = thread::Builder::new()
                .name("aura-pipeline".into())
                .spawn(move || {
                    run_self_test(thread_stop);
                })
                .expect("Failed to spawn pipeline thread");

            return Ok(Self::SelfTest {
                pipeline_thread,
                stop_signal,
            });
        }

        let (producer, consumer) = audio_ring_buffer(16_000, 5.0);

        let config = CaptureConfig {
            target_pid,
            include_process_tree: include_tree,
        };
        let mut capturer = AudioCapturer::new(config, producer);
        capturer.start()?;

        let sense_voice = if !asr_model_path.is_empty() {
            match SenseVoiceEngine::new(asr_model_path) {
                Ok(engine) => {
                    log::info!("SenseVoice ASR engine loaded");
                    Some(Arc::new(engine))
                }
                Err(e) => {
                    log::warn!("Failed to load SenseVoice ASR engine: {:#}", e);
                    None
                }
            }
        } else {
            None
        };

        let thread_sv = sense_voice.as_ref().map(Arc::clone);
        let thread_stop = Arc::clone(&stop_signal);
        let model_path_owned = vad_model_path.to_owned();

        let pipeline_thread = thread::Builder::new()
            .name("aura-pipeline".into())
            .spawn(move || {
                if let Err(e) = run_pipeline(thread_stop, consumer, &model_path_owned, thread_sv)
                {
                    log::error!("Pipeline worker exited with error: {:#}", e);
                }
            })
            .expect("Failed to spawn pipeline thread");

        Ok(Self::Live {
            capturer,
            pipeline_thread,
            stop_signal,
            sense_voice,
        })
    }

    pub fn stop(self) -> anyhow::Result<()> {
        match self {
            Self::SelfTest { pipeline_thread, stop_signal } => {
                stop_signal.store(true, Ordering::SeqCst);
                pipeline_thread
                    .join()
                    .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))?;
                Ok(())
            }
            Self::Live { mut capturer, pipeline_thread, stop_signal, .. } => {
                stop_signal.store(true, Ordering::SeqCst);
                pipeline_thread
                    .join()
                    .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))?;
                capturer
                    .stop()
                    .map_err(|e| anyhow::anyhow!("Failed to stop capturer: {}", e))?;
                Ok(())
            }
        }
    }
}

fn run_self_test(stop_signal: Arc<AtomicBool>) {
    log::info!("Self-test pipeline started");
    let phrases = [
        "Hello, this is a self-test of the Aura real-time translation system.",
        "The audio capture and speech recognition pipeline is working correctly.",
        "Subtitles should appear in the overlay window on your screen.",
        "You can now test with a real application like Edge or Chrome.",
        "Select a target process from the dropdown menu and click Start Translation.",
    ];

    for phrase in phrases.iter().cycle() {
        if stop_signal.load(Ordering::SeqCst) { break; }

        // Charity-by-character typing effect via provisional updates (~50 chars per step)
        let chars: Vec<char> = phrase.chars().collect();
        let total = chars.len();
        let mut pos = 0;
        let step = 2;
        let default_metrics = super::exports::TranslationMetrics::default();
        while pos < total {
            if stop_signal.load(Ordering::SeqCst) { break; }
            let end = (pos + step).min(total);
            let partial: String = chars[..end].iter().collect();
            let latency = (end as i32) * 10;
            super::exports::emit_translation(&partial, true, latency, default_metrics);
            pos = end;
            thread::sleep(Duration::from_millis(30));
        }

        if stop_signal.load(Ordering::SeqCst) { break; }

        // Final — full sentence, committed
        let ms = (total as i32) * 10 + 50;
        super::exports::emit_translation(phrase, false, ms, default_metrics);

        // Pause to let user read the complete sentence
        for _ in 0..24 {
            if stop_signal.load(Ordering::SeqCst) { break; }
            thread::sleep(Duration::from_millis(250));
        }
    }

    log::info!("Self-test pipeline stopped");
}

// ── Diagnostic helpers ──────────────────────────────────────────────

/// Logs VAD probability and decision to logs/vad_*.csv for offline analysis.
struct VadLogger {
    file: std::fs::File,
    start: Instant,
}

impl VadLogger {
    fn new() -> std::io::Result<Self> {
        let dir = Path::new("logs");
        let _ = fs::create_dir_all(dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = dir.join(format!("vad_{}.csv", ts));
        let mut file = fs::File::create(&path)?;
        writeln!(file, "elapsed_ms,probability,is_speech")?;
        log::info!("VAD debug log: {}", path.display());
        Ok(Self { file, start: Instant::now() })
    }

    fn log(&mut self, prob: f32, is_speech: bool) {
        let elapsed = self.start.elapsed().as_secs_f64() * 1000.0;
        let _ = writeln!(self.file, "{:.1},{:.4},{}", elapsed, prob, is_speech as u8);
    }
}

/// Saves the first N seconds of captured audio to logs/capture_dump_*.raw
/// for offline diagnosis of capture quality.
struct CaptureDumper {
    max_samples: usize,
    buffer: Vec<f32>,
    saved: bool,
}

impl CaptureDumper {
    fn new(duration_secs: f32) -> Self {
        Self {
            max_samples: (16000.0 * duration_secs) as usize,
            buffer: Vec::with_capacity((16000.0 * duration_secs) as usize),
            saved: false,
        }
    }

    fn feed(&mut self, samples: &[f32]) {
        if self.saved {
            return;
        }
        let remaining = self.max_samples - self.buffer.len();
        let take = samples.len().min(remaining);
        self.buffer.extend_from_slice(&samples[..take]);
        if self.buffer.len() >= self.max_samples {
            self.save();
        }
    }

    fn save(&mut self) {
        let dir = Path::new("logs");
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let path = dir.join(format!("capture_dump_{}.raw", ts));
        let bytes: Vec<u8> = self.buffer
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        if fs::write(&path, &bytes).is_ok() {
            log::info!(
                "Capture dump saved: {} ({} samples, {:.1}s)",
                path.display(),
                self.buffer.len(),
                self.buffer.len() as f64 / 16000.0
            );
        }
        self.saved = true;
    }
}

fn run_pipeline(
    stop_signal: Arc<AtomicBool>,
    mut consumer: AudioConsumer,
    model_path: &str,
    sense_voice: Option<Arc<SenseVoiceEngine>>,
) -> anyhow::Result<()> {
    log::info!("Pipeline worker started");

    let mut vad = SileroVad::new(model_path)
        .map_err(|e| anyhow::anyhow!("Failed to load VAD model '{}': {}", model_path, e))?;

    // Warm-up: one silence frame to eliminate ONNX first-inference latency
    let warmup_frame = vec![0.0f32; SileroVad::AUDIO_SAMPLES];
    if let Err(e) = vad.process_frame(&warmup_frame) {
        log::warn!("VAD warm-up inference failed (non-fatal): {:#}", e);
    }
    vad.reset_state();

    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());
    let mut frame_buffer: Vec<f32> = Vec::with_capacity(16_000 * 2);
    let pipeline_start = Instant::now();

    // Diagnostic logging
    let mut vad_logger = VadLogger::new().ok();
    let mut capture_dumper = CaptureDumper::new(10.0);

    while !stop_signal.load(Ordering::SeqCst) {
        let available = consumer.available();
        if available >= SileroVad::AUDIO_SAMPLES {
            if let Some(samples) = consumer.pull(available) {
                frame_buffer.extend_from_slice(&samples);

                while frame_buffer.len() >= SileroVad::AUDIO_SAMPLES {
                    let frame: Vec<f32> =
                        frame_buffer.drain(..SileroVad::AUDIO_SAMPLES).collect();

                    let vad_result = vad.process_frame(&frame)?;

                    // Diagnostic: log VAD probability and dump captured audio
                    if let Some(ref mut logger) = vad_logger {
                        logger.log(vad_result.probability, vad_result.is_speech);
                    }
                    capture_dumper.feed(&frame);

                    if let Some(chunk) = state_machine.feed(&vad_result, &frame) {
                        let t_chunk_ready = Instant::now();
                        let duration_ms =
                            (chunk.samples.len() as u64 * 1000) / 16_000;
                        let latency = pipeline_start.elapsed().as_millis() as i32;

                        let (text, is_provisional, asr_inference_ms) = match chunk.chunk_type {
                            ChunkType::Provisional => {
                                let metrics = super::exports::TranslationMetrics {
                                    audio_duration_ms: duration_ms as u32,
                                    asr_inference_ms: 0,
                                    rust_total_ms: 0,
                                };
                                (format!("[~] {}ms speech...", duration_ms), true, metrics)
                            }
                            ChunkType::Final | ChunkType::HardCut => {
                                // Reset VAD RNN state between utterances to
                                // prevent hidden state leakage across segments
                                vad.reset_state();
                                let t_asr_start = Instant::now();
                                let result = if let Some(ref sv) = sense_voice {
                                    match sv.transcribe(&chunk.samples) {
                                        Ok(asr_text) if !asr_text.is_empty() => {
                                            (asr_text, false)
                                        }
                                        Ok(_) => {
                                            (format!("[✓] {}ms (no speech)", duration_ms), false)
                                        }
                                        Err(e) => {
                                            log::warn!("ASR error: {:#}", e);
                                            (format!("[✓] {}ms (ASR failed)", duration_ms), false)
                                        }
                                    }
                                } else {
                                    (format!("[✓] {}ms sentence", duration_ms), false)
                                };
                                let t_asr_end = Instant::now();
                                let t_callback = Instant::now();
                                let asr_ms = t_asr_end.duration_since(t_asr_start).as_millis() as u32;
                                let total_ms = t_callback.duration_since(t_chunk_ready).as_millis() as u32;
                                let metrics = super::exports::TranslationMetrics {
                                    audio_duration_ms: duration_ms as u32,
                                    asr_inference_ms: asr_ms,
                                    rust_total_ms: total_ms,
                                };
                                (result.0, result.1, metrics)
                            }
                        };

                        super::exports::emit_translation(&text, is_provisional, latency, asr_inference_ms);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if !frame_buffer.is_empty() {
        let duration_ms = (frame_buffer.len() as u64 * 1000) / 16_000;
        let text = format!("[✓] {}ms (flush)", duration_ms);
        let metrics = super::exports::TranslationMetrics::default();
        super::exports::emit_translation(&text, false, 0, metrics);
    }

    log::info!("Pipeline worker stopped");
    Ok(())
}
