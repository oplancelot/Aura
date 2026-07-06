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
        while pos < total {
            if stop_signal.load(Ordering::SeqCst) { break; }
            let end = (pos + step).min(total);
            let partial: String = chars[..end].iter().collect();
            let latency = (end as i32) * 10;
            super::exports::emit_translation(&partial, true, latency);
            pos = end;
            thread::sleep(Duration::from_millis(30));
        }

        if stop_signal.load(Ordering::SeqCst) { break; }

        // Final — full sentence, committed
        let ms = (total as i32) * 10 + 50;
        super::exports::emit_translation(phrase, false, ms);

        // Pause to let user read the complete sentence
        for _ in 0..24 {
            if stop_signal.load(Ordering::SeqCst) { break; }
            thread::sleep(Duration::from_millis(250));
        }
    }

    log::info!("Self-test pipeline stopped");
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
    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());
    let mut frame_buffer: Vec<f32> = Vec::with_capacity(16_000 * 2);
    let pipeline_start = Instant::now();

    while !stop_signal.load(Ordering::SeqCst) {
        let available = consumer.available();
        if available >= SileroVad::FRAME_SAMPLES {
            if let Some(samples) = consumer.pull(available) {
                frame_buffer.extend_from_slice(&samples);

                while frame_buffer.len() >= SileroVad::FRAME_SAMPLES {
                    let frame: Vec<f32> =
                        frame_buffer.drain(..SileroVad::FRAME_SAMPLES).collect();

                    let vad_result = vad.process_frame(&frame)?;

                    if let Some(chunk) = state_machine.feed(&vad_result, &frame) {
                        let duration_ms =
                            (chunk.samples.len() as u64 * 1000) / 16_000;
                        let latency = pipeline_start.elapsed().as_millis() as i32;

                        let (text, is_provisional) = match chunk.chunk_type {
                            ChunkType::Provisional => {
                                (format!("[~] {}ms speech...", duration_ms), true)
                            }
                            ChunkType::Final | ChunkType::HardCut => {
                                // Reset VAD RNN state between utterances to
                                // prevent hidden state leakage across segments
                                vad.reset_state();
                                if let Some(ref sv) = sense_voice {
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
                                }
                            }
                        };

                        super::exports::emit_translation(&text, is_provisional, latency);
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    if !frame_buffer.is_empty() {
        let duration_ms = (frame_buffer.len() as u64 * 1000) / 16_000;
        let text = format!("[✓] {}ms (flush)", duration_ms);
        super::exports::emit_translation(&text, false, 0);
    }

    log::info!("Pipeline worker stopped");
    Ok(())
}
