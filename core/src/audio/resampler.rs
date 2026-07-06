//! Audio sample rate converter (resampler).
//!
//! WASAPI typically delivers audio at the device's native rate (e.g. 48 kHz),
//! but Silero VAD and most ASR models expect 16 kHz mono input.
//!
//! Uses `rubato::SincFixedIn` for high-quality sinc resampling with >100 dB
//! stopband attenuation, eliminating aliasing artifacts that degrade ASR accuracy.

use rubato::{Resampler as RubatoResampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// High-quality real-time resampler (mono) using sinc interpolation.
///
/// Wraps `rubato::SincFixedIn` to convert from `source_rate` to `target_rate`
/// with minimal aliasing.
pub struct Resampler {
    resampler: SincFixedIn<f32>,
    /// Input chunk size expected by rubato.
    chunk_size: usize,
    /// Leftover samples from previous `process()` calls that didn't fill a chunk.
    leftover: Vec<f32>,
}

impl Resampler {
    /// Create a new resampler.
    ///
    /// # Arguments
    /// * `source_rate` – Input sample rate (e.g. 48000)
    /// * `target_rate` – Output sample rate (e.g. 16000)
    pub fn new(source_rate: u32, target_rate: u32) -> Self {
        let resample_ratio = target_rate as f64 / source_rate as f64;

        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };

        // Use a reasonable chunk size for real-time streaming.
        // 480 samples @ 48kHz = 10ms, matching typical WASAPI packet sizes.
        let chunk_size = 480;

        let resampler = SincFixedIn::<f32>::new(
            resample_ratio,
            1.0, // max_relative_ratio (no dynamic adjustment needed)
            params,
            chunk_size,
            1, // mono
        )
        .expect("Failed to create sinc resampler");

        Self {
            resampler,
            chunk_size,
            leftover: Vec::with_capacity(chunk_size),
        }
    }

    /// Resample a buffer of f32 samples from source rate to target rate.
    ///
    /// Returns a new `Vec<f32>` at the target sample rate.
    /// Maintains internal buffer for seamless concatenation across calls.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        // Accumulate with any leftover from previous call
        self.leftover.extend_from_slice(input);

        let mut output = Vec::new();

        // Process complete chunks
        while self.leftover.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.leftover.drain(..self.chunk_size).collect();
            let input_buf = vec![chunk];

            match self.resampler.process(&input_buf, None) {
                Ok(result) => {
                    if !result.is_empty() && !result[0].is_empty() {
                        output.extend_from_slice(&result[0]);
                    }
                }
                Err(e) => {
                    log::warn!("Resampler error: {}", e);
                }
            }
        }

        output
    }

    /// Reset the resampler's internal state.
    pub fn reset(&mut self) {
        self.resampler.reset();
        self.leftover.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_3x_preserves_length_ratio() {
        let mut resampler = Resampler::new(48000, 16000);
        // 480 samples @ 48kHz = 10ms → should produce ~160 samples @ 16kHz
        let input: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0).sin()).collect();
        let output = resampler.process(&input);
        // Allow tolerance for sinc filter latency (initial chunks produce fewer samples)
        assert!(
            (output.len() as i32 - 160).abs() <= 15,
            "Expected ~160 samples, got {}",
            output.len()
        );
    }

    #[test]
    fn multiple_calls_produce_continuous_output() {
        let mut resampler = Resampler::new(48000, 16000);
        let mut total_output = 0;
        // Feed 10 chunks of 480 samples (= 4800 samples @ 48kHz = 100ms)
        for _ in 0..10 {
            let input: Vec<f32> = (0..480).map(|i| (i as f32 / 480.0).sin()).collect();
            let output = resampler.process(&input);
            total_output += output.len();
        }
        // 4800 @ 48kHz → ~1600 @ 16kHz, allow tolerance for sinc filter latency
        assert!(
            (total_output as i32 - 1600).abs() <= 15,
            "Expected ~1600 total samples, got {}",
            total_output
        );
    }
}
