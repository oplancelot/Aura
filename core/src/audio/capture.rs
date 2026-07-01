//! WASAPI process-level loopback audio capture.
//!
//! Uses `ActivateAudioInterfaceAsync` with `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`
//! and `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE` to capture audio from
//! a specific process (e.g. Discord) and all its child processes, excluding game audio.
//!
//! # Latency
//! Physical capture latency is ~10-15 ms (one device period in shared mode).

use anyhow::Result;
use std::sync::Arc;

use super::ring_buffer::AudioRingBuffer;

/// Configuration for audio capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Target process ID (e.g. Discord main process).
    pub target_pid: u32,
    /// Whether to include the entire process tree (recommended for Chromium-based apps).
    pub include_process_tree: bool,
}

/// Captures audio from a specific Windows process via WASAPI loopback.
pub struct AudioCapturer {
    config: CaptureConfig,
    ring_buffer: Arc<AudioRingBuffer>,
    is_capturing: bool,
}

impl AudioCapturer {
    /// Create a new capturer targeting the given process.
    ///
    /// The captured PCM samples will be pushed into the provided `ring_buffer`.
    pub fn new(config: CaptureConfig, ring_buffer: Arc<AudioRingBuffer>) -> Self {
        Self {
            config,
            ring_buffer,
            is_capturing: false,
        }
    }

    /// Start the capture loop on a background thread.
    ///
    /// Internally calls `ActivateAudioInterfaceAsync` with:
    /// - `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`
    /// - `ProcessLoopbackParams.TargetProcessId = self.config.target_pid`
    /// - `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE`
    pub fn start(&mut self) -> Result<()> {
        if self.is_capturing {
            anyhow::bail!("Capture already running");
        }

        log::info!(
            "Starting WASAPI process loopback capture for PID {} (tree={})",
            self.config.target_pid,
            self.config.include_process_tree
        );

        // TODO: Phase 1 implementation
        // 1. Call ActivateAudioInterfaceAsync with VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK
        // 2. Configure AUDIOCLIENT_ACTIVATION_PARAMS with target PID
        // 3. Initialize IAudioClient in shared mode
        // 4. Spawn callback thread that reads packets and writes to ring_buffer
        // 5. Resample from device rate (48kHz) to 16kHz before writing

        self.is_capturing = true;
        Ok(())
    }

    /// Stop the capture loop and release audio resources.
    pub fn stop(&mut self) -> Result<()> {
        if !self.is_capturing {
            return Ok(());
        }

        log::info!("Stopping WASAPI capture");
        // TODO: Signal the capture thread to exit, join, release COM objects
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
