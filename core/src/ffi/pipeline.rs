use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::audio::capture::{AudioCapturer, CaptureConfig};
use crate::audio::ring_buffer::AudioRingBuffer;
use crate::vad::silero::SileroVad;
use crate::vad::state_machine::{ChunkingConfig, ChunkingStateMachine, ChunkType};

pub struct PipelineState {
    pub capturer: AudioCapturer,
    pub pipeline_thread: JoinHandle<()>,
    pub stop_signal: Arc<AtomicBool>,
    pub ring_buffer: Arc<AudioRingBuffer>,
}

impl PipelineState {
    pub fn start(
        target_pid: u32,
        include_tree: bool,
        model_path: &str,
    ) -> anyhow::Result<Self> {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let ring_buffer = Arc::new(AudioRingBuffer::new(16_000, 5.0));

        let config = CaptureConfig {
            target_pid,
            include_process_tree: include_tree,
        };
        let mut capturer = AudioCapturer::new(config, Arc::clone(&ring_buffer));
        capturer.start()?;

        let thread_stop = Arc::clone(&stop_signal);
        let thread_rb = Arc::clone(&ring_buffer);
        let model_path_owned = model_path.to_owned();

        let pipeline_thread = thread::Builder::new()
            .name("aura-pipeline".into())
            .spawn(move || {
                if let Err(e) = run_pipeline(thread_stop, thread_rb, &model_path_owned) {
                    log::error!("Pipeline worker exited with error: {:#}", e);
                }
            })
            .expect("Failed to spawn pipeline thread");

        Ok(Self {
            capturer,
            pipeline_thread,
            stop_signal,
            ring_buffer,
        })
    }

    pub fn stop(mut self) -> anyhow::Result<()> {
        self.stop_signal.store(true, Ordering::SeqCst);

        self.pipeline_thread
            .join()
            .map_err(|_| anyhow::anyhow!("Pipeline thread panicked"))?;

        self.capturer
            .stop()
            .map_err(|e| anyhow::anyhow!("Failed to stop capturer: {}", e))?;

        Ok(())
    }
}

fn run_pipeline(
    stop_signal: Arc<AtomicBool>,
    ring_buffer: Arc<AudioRingBuffer>,
    model_path: &str,
) -> anyhow::Result<()> {
    log::info!("Pipeline worker started");

    let mut vad = SileroVad::new(model_path)
        .map_err(|e| anyhow::anyhow!("Failed to load VAD model '{}': {}", model_path, e))?;
    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());
    let mut frame_buffer: Vec<f32> = Vec::with_capacity(16_000 * 2);
    let pipeline_start = Instant::now();

    while !stop_signal.load(Ordering::SeqCst) {
        let available = ring_buffer.available();
        if available >= SileroVad::FRAME_SAMPLES {
            if let Some(samples) = ring_buffer.pull(available) {
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
                            ChunkType::Final => {
                                (format!("[✓] {}ms sentence", duration_ms), false)
                            }
                            ChunkType::HardCut => {
                                (format!("[✂] {}ms (hard cut)", duration_ms), false)
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
