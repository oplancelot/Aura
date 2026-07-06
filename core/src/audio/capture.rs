//! WASAPI process-level loopback audio capture.
//!
//! Uses `ActivateAudioInterfaceAsync` with `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`
//! and `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE` to capture audio from
//! a specific process (e.g. Discord) and all its child processes.
//!
//! # Latency
//! Physical capture latency is ~10-15 ms (one device period in shared mode).

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use super::resampler::Resampler;
use super::ring_buffer::AudioProducer;

/// Configuration for audio capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target process ID (e.g. Discord main process).
    pub target_pid: u32,
    /// Whether to include the entire process tree (recommended for Chromium-based apps).
    pub include_process_tree: bool,
}

/// The sample rate WASAPI typically delivers (device native rate).
const DEVICE_SAMPLE_RATE: u32 = 48000;
/// The number of channels requested from WASAPI (stereo).
const DEVICE_CHANNELS: u16 = 2;
/// The target sample rate for downstream AI models (Silero VAD, ASR).
const TARGET_SAMPLE_RATE: u32 = 16000;
/// Timeout in milliseconds for waiting on the WASAPI event handle.
/// Kept short so `stop_signal` is checked frequently.
const EVENT_WAIT_TIMEOUT_MS: u32 = 100;

/// Captures audio from a specific Windows process via WASAPI loopback.
pub struct AudioCapturer {
    config: CaptureConfig,
    /// Producer half of the ring buffer, moved into the capture thread on start.
    producer: Option<AudioProducer>,
    is_capturing: bool,
    /// Handle to the capture worker thread.
    capture_thread: Option<JoinHandle<()>>,
    /// Signal to stop the capture loop (shared with worker thread).
    stop_signal: Arc<AtomicBool>,
}

impl AudioCapturer {
    /// Create a new capturer targeting the given process.
    ///
    /// The captured PCM samples will be pushed into the provided `producer`.
    pub fn new(config: CaptureConfig, producer: AudioProducer) -> Self {
        Self {
            config,
            producer: Some(producer),
            is_capturing: false,
            capture_thread: None,
            stop_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the capture loop on a background thread.
    ///
    /// Internally spawns a capture thread that uses process-specific WASAPI loopback
    /// to capture audio from the target PID. Audio is captured at 48 kHz stereo,
    /// mixed down to mono, resampled to 16 kHz, and pushed into the ring buffer.
    pub fn start(&mut self) -> Result<()> {
        if self.is_capturing {
            anyhow::bail!("Capture already running");
        }

        log::info!(
            "Starting WASAPI process loopback capture for PID {} (tree={})",
            self.config.target_pid,
            self.config.include_process_tree
        );

        self.stop_signal.store(false, Ordering::SeqCst);

        let stop_signal = Arc::clone(&self.stop_signal);
        let mut producer = self.producer.take()
            .ok_or_else(|| anyhow::anyhow!("AudioProducer already consumed (capture already started?)"))?;
        let target_pid = self.config.target_pid;
        let include_tree = self.config.include_process_tree;

        let handle = thread::Builder::new()
            .name("aura-capture".into())
            .spawn(move || {
                if let Err(e) = capture_worker(stop_signal, &mut producer, target_pid, include_tree) {
                    log::error!("Capture worker exited with error: {:#}", e);
                }
            })
            .context("Failed to spawn capture thread")?;

        self.capture_thread = Some(handle);
        self.is_capturing = true;
        Ok(())
    }

    /// Stop the capture loop and release audio resources.
    pub fn stop(&mut self) -> Result<()> {
        if !self.is_capturing {
            return Ok(());
        }

        log::info!("Stopping WASAPI capture");
        self.stop_signal.store(true, Ordering::SeqCst);

        if let Some(handle) = self.capture_thread.take() {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("Capture thread panicked"))?;
        }

        self.is_capturing = false;
        Ok(())
    }

    /// Returns `true` if capture is currently active.
    pub fn is_capturing(&self) -> bool {
        self.is_capturing
    }
}

impl Drop for AudioCapturer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

// ── Capture worker (runs on a dedicated thread) ─────────────────────────

/// The main capture loop running on a background thread.
///
/// This function:
/// 1. Initializes COM (MTA) for the current thread
/// 2. Attempts process-specific loopback; falls back to device-level loopback
/// 3. Configures event-driven shared-mode capture at 48 kHz / f32 / stereo
/// 4. Reads audio packets, converts stereo → mono, resamples 48 → 16 kHz
/// 5. Pushes 16 kHz mono f32 samples into the ring buffer
fn capture_worker(
    stop_signal: Arc<AtomicBool>,
    producer: &mut AudioProducer,
    target_pid: u32,
    include_tree: bool,
) -> Result<()> {
    use wasapi::*;

    // Step 1: Initialize COM for this thread (MTA)
    initialize_mta().ok().context("COM MTA initialization failed")?;
    log::debug!("COM MTA initialized on capture thread");

    // Step 2: Try process-specific loopback, fall back to device loopback
    let mut audio_client = match AudioClient::new_application_loopback_client(target_pid, include_tree) {
        Ok(client) => {
            log::info!("Process loopback client created for PID {}", target_pid);
            client
        }
        Err(e) => {
            log::warn!("Process loopback for PID {} failed ({}), falling back to device loopback", target_pid, e);
            let enumerator = DeviceEnumerator::new()
                .context("Failed to create DeviceEnumerator")?;
            let device = enumerator.get_default_device(&Direction::Render)
                .context("Failed to get default render device")?;
            log::info!("Using default render device for loopback capture");
            device.get_iaudioclient()
                .context("Failed to get IAudioClient from render device")?
        }
    };

    // Step 3: Configure capture format — f32, 48 kHz, stereo with autoconvert
    let desired_format = WaveFormat::new(
        32,                                    // bits per sample
        32,                                    // valid bits per sample
        &SampleType::Float,
        DEVICE_SAMPLE_RATE as usize,           // 48000
        DEVICE_CHANNELS as usize,              // 2 (stereo)
        None,                                  // channel mask (auto)
    );
    let blockalign = desired_format.get_blockalign() as usize;
    log::debug!(
        "Desired capture format: {:?}, blockalign={}",
        desired_format,
        blockalign
    );

    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };
    audio_client
        .initialize_client(&desired_format, &Direction::Capture, &mode)
        .context("Failed to initialize audio client")?;
    log::debug!("Audio client initialized in EventsShared mode");

    // Step 4: Get event handle for event-driven capture
    let h_event = audio_client
        .set_get_eventhandle()
        .context("Failed to get event handle")?;

    // Step 5: Get capture client
    let capture_client = audio_client
        .get_audiocaptureclient()
        .context("Failed to get audio capture client")?;

    // Step 6: Prepare resampler (48 kHz → 16 kHz)
    let mut resampler = Resampler::new(DEVICE_SAMPLE_RATE, TARGET_SAMPLE_RATE);

    // Byte queue for accumulating raw capture data
    let mut sample_queue: VecDeque<u8> = VecDeque::with_capacity(
        blockalign * DEVICE_SAMPLE_RATE as usize, // ~1 second buffer
    );

    // Step 7: Start the audio stream
    audio_client
        .start_stream()
        .context("Failed to start audio stream")?;
    log::info!("WASAPI capture stream started");

    // Step 8: Event loop — read packets until stop signal
    while !stop_signal.load(Ordering::SeqCst) {
        // Wait for audio engine to signal new data (with timeout for stop check)
        if h_event.wait_for_event(EVENT_WAIT_TIMEOUT_MS).is_err() {
            // Timeout — no data available, loop back to check stop_signal
            continue;
        }

        // Read all available packets into the byte queue
        match capture_client.read_from_device_to_deque(&mut sample_queue) {
            Ok(_info) => {}
            Err(e) => {
                log::warn!("Error reading from capture device: {}", e);
                continue;
            }
        }

        process_sample_queue(
            &mut sample_queue,
            blockalign,
            &mut resampler,
            producer,
        );
    }

    // Step 9: Cleanup — stop stream
    audio_client.stop_stream().ok();
    log::info!("WASAPI capture stream stopped");
    Ok(())
}



/// Process accumulated bytes in the sample queue.
///
/// Extracts complete frames, converts stereo f32 to mono,
/// resamples from 48 kHz to 16 kHz, and pushes into the ring buffer.
fn process_sample_queue(
    sample_queue: &mut VecDeque<u8>,
    blockalign: usize,
    resampler: &mut Resampler,
    producer: &mut AudioProducer,
) {
    // Each frame = blockalign bytes (8 bytes for f32 stereo: 4 bytes × 2 channels)
    let bytes_per_sample = std::mem::size_of::<f32>(); // 4
    let num_channels = DEVICE_CHANNELS as usize; // 2
    let frame_size = bytes_per_sample * num_channels; // 8

    debug_assert_eq!(blockalign, frame_size, "Unexpected blockalign");

    // Calculate how many complete frames we have
    let available_frames = sample_queue.len() / frame_size;
    if available_frames == 0 {
        return;
    }

    // Extract frames and convert to mono f32
    let mut mono_samples = Vec::with_capacity(available_frames);

    for _ in 0..available_frames {
        // Read one frame (2 × f32 = 8 bytes)
        let mut frame_bytes = [0u8; 8];
        for (i, byte) in frame_bytes.iter_mut().enumerate() {
            *byte = sample_queue.pop_front().unwrap_or(0);
            let _ = i;
        }

        let left = f32::from_le_bytes([frame_bytes[0], frame_bytes[1], frame_bytes[2], frame_bytes[3]]);
        let right = f32::from_le_bytes([frame_bytes[4], frame_bytes[5], frame_bytes[6], frame_bytes[7]]);

        // Mix down to mono
        mono_samples.push((left + right) * 0.5);
    }

    // Resample from 48 kHz to 16 kHz
    let resampled = resampler.process(&mono_samples);

    // Push into ring buffer for downstream VAD processing
    if !resampled.is_empty() {
        producer.push(&resampled);
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;

