//! Audio sample rate converter (resampler).
//!
//! WASAPI typically delivers audio at the device's native rate (e.g. 48 kHz),
//! but Silero VAD and most ASR models expect 16 kHz mono input.
//! This module provides a simple linear interpolation resampler for real-time use.

/// A simple real-time resampler that converts from `source_rate` to `target_rate`.
pub struct Resampler {
    source_rate: u32,
    target_rate: u32,
    /// Fractional sample position for interpolation continuity across calls.
    phase: f64,
}

impl Resampler {
    /// Create a new resampler.
    ///
    /// # Arguments
    /// * `source_rate` – Input sample rate (e.g. 48000)
    /// * `target_rate` – Output sample rate (e.g. 16000)
    pub fn new(source_rate: u32, target_rate: u32) -> Self {
        Self {
            source_rate,
            target_rate,
            phase: 0.0,
        }
    }

    /// Resample a buffer of f32 samples from source rate to target rate.
    ///
    /// Returns a new `Vec<f32>` at the target sample rate.
    /// Maintains internal phase for seamless concatenation across calls.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.source_rate == self.target_rate {
            return input.to_vec();
        }

        let ratio = self.source_rate as f64 / self.target_rate as f64;
        let output_len = ((input.len() as f64 - self.phase) / ratio).ceil() as usize;
        let mut output = Vec::with_capacity(output_len);

        while self.phase < input.len() as f64 - 1.0 {
            let idx = self.phase as usize;

            if ratio >= 2.0 {
                // Crude anti-aliasing: average the samples in the decimation window
                let window = ratio as usize;
                let mut sum = 0.0;
                let mut count = 0;
                for i in 0..window {
                    if idx + i < input.len() {
                        sum += input[idx + i] as f64;
                        count += 1;
                    }
                }
                output.push((sum / count as f64) as f32);
            } else {
                let frac = self.phase - idx as f64;
                // Linear interpolation
                let sample = input[idx] as f64 * (1.0 - frac) + input[idx + 1] as f64 * frac;
                output.push(sample as f32);
            }

            self.phase += ratio;
        }

        // Wrap phase for next call
        self.phase -= input.len() as f64;
        if self.phase < 0.0 {
            self.phase = 0.0;
        }

        output
    }

    /// Reset the resampler's internal state.
    pub fn reset(&mut self) {
        self.phase = 0.0;
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
        // Allow ±1 sample tolerance due to interpolation boundary
        assert!((output.len() as i32 - 160).abs() <= 1);
    }

    #[test]
    fn passthrough_when_same_rate() {
        let mut resampler = Resampler::new(16000, 16000);
        let input = vec![0.1, 0.2, 0.3, 0.4];
        let output = resampler.process(&input);
        assert_eq!(input, output);
    }
}
