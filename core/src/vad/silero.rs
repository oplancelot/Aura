//! Silero VAD ONNX inference wrapper.
//!
//! Silero VAD is a lightweight neural-network-based voice activity detector that
//! runs efficiently on CPU via ONNX Runtime.  It expects exactly **512 samples**
//! per call at **16 kHz** (= 32 ms frame), and returns a probability score [0, 1]
//! indicating how likely the frame contains human speech.
//!
//! # Hysteresis
//! - Speech ON  threshold: probability > 0.5
//! - Speech OFF threshold: probability < 0.35 (sustained over multiple frames)

use anyhow::Result;
use ort::{session::builder::GraphOptimizationLevel, session::Session, value::Tensor};

/// Silero VAD model wrapper using ONNX Runtime.
pub struct SileroVad {
    session: Session,
    /// Internal RNN state [2, 1, 128].
    state: Vec<f32>,
    /// Hysteresis state: tracks whether we are currently in a speech segment.
    is_speaking: bool,
}

/// Result of a single VAD frame inference.
#[derive(Debug, Clone, Copy)]
pub struct VadResult {
    /// Speech probability in [0.0, 1.0].
    pub probability: f32,
    /// Whether this frame is classified as speech (after applying thresholds).
    pub is_speech: bool,
}

impl SileroVad {
    /// Required number of samples per frame at 16 kHz.
    pub const FRAME_SAMPLES: usize = 512;
    /// Sampling rate expected by Silero (16 kHz).
    pub const SAMPLE_RATE: usize = 16000;
    /// Speech ON probability threshold.
    /// Must exceed this to transition from silence → speech.
    pub const THRESHOLD_ON: f32 = 0.10;
    /// Speech OFF probability threshold.
    /// Must drop below this to transition from speech → silence.
    pub const THRESHOLD_OFF: f32 = 0.05;

    /// Load the Silero VAD ONNX model from the given path.
    ///
    /// # Arguments
    /// * `model_path` – Path to `silero_vad.onnx`
    pub fn new(model_path: &str) -> Result<Self> {
        log::info!("Loading Silero VAD model from: {}", model_path);

        let session = Session::builder()
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level1)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .with_intra_threads(1)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .commit_from_file(model_path)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        Ok(Self {
            session,
            state: vec![0.0f32; 2 * 1 * 128],
            is_speaking: false,
        })
    }

    /// Run inference on a single 32ms frame (exactly 512 samples at 16 kHz).
    ///
    /// Returns the speech probability and binary classification.
    ///
    /// # Panics
    /// Panics if `frame.len() != 512`.
    pub fn process_frame(&mut self, frame: &[f32]) -> Result<VadResult> {
        assert_eq!(
            frame.len(),
            Self::FRAME_SAMPLES,
            "Silero VAD requires exactly {} samples per frame, got {}",
            Self::FRAME_SAMPLES,
            frame.len()
        );

        let outputs = self.session.run(ort::inputs![
            "input" => Tensor::from_array((vec![1, Self::FRAME_SAMPLES], frame.to_vec())).unwrap(),
            "sr" => Tensor::from_array((Vec::<i64>::new(), vec![Self::SAMPLE_RATE as i64])).unwrap(),
            "state" => Tensor::from_array((vec![2, 1, 128], self.state.clone())).unwrap(),
        ]).map_err(|e| anyhow::anyhow!("{e}"))?;

        let output_data = outputs["output"].try_extract_tensor::<f32>().map_err(|e| anyhow::anyhow!("{e}"))?;
        let probability = output_data.1[0];

        // Update state
        let state_data = outputs["stateN"].try_extract_tensor::<f32>().map_err(|e| anyhow::anyhow!("{e}"))?;
        self.state.copy_from_slice(state_data.1);

        // Hysteresis: use different thresholds for onset vs. offset
        let is_speech = if self.is_speaking {
            // Currently speaking — stay speaking until probability drops below OFF threshold
            probability > Self::THRESHOLD_OFF
        } else {
            // Currently silent — only start speaking when probability exceeds ON threshold
            probability > Self::THRESHOLD_ON
        };
        self.is_speaking = is_speech;

        Ok(VadResult {
            probability,
            is_speech,
        })
    }

    /// Reset the model's internal RNN state (call between speakers or after silence).
    pub fn reset_state(&mut self) {
        self.state.fill(0.0);
        self.is_speaking = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silero_vad_init_and_zeros() {
        let _ = ort::init().with_name("aura").commit(); // Ignore if already initialized

        let mut vad = SileroVad::new("assets/silero_vad.onnx").unwrap();
        let frame = vec![0.0f32; 512];
        let result = vad.process_frame(&frame).unwrap();
        
        // Zeros should not be speech
        assert!(!result.is_speech);
        // Probability should be low
        assert!(result.probability < 0.2);
    }
}
