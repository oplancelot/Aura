//! Lock-free SPSC (Single Producer, Single Consumer) ring buffer.
//!
//! Bridges the WASAPI capture callback thread (producer) and the VAD processing
//! thread (consumer) without any mutex contention.  The capture callback must
//! never block — any allocation or lock would cause audio glitches (buffer
//! underruns).

use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Consumer, Split, Observer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared overflow flag between producer and consumer.
type OverflowFlag = Arc<AtomicBool>;

/// Producer half of the audio ring buffer (capture thread).
///
/// This side pushes samples and must never block.
pub struct AudioProducer {
    inner: ringbuf::HeapProd<f32>,
    overflowed: OverflowFlag,
}

/// Consumer half of the audio ring buffer (pipeline/VAD thread).
///
/// This side pulls samples for downstream processing.
pub struct AudioConsumer {
    inner: ringbuf::HeapCons<f32>,
    overflowed: OverflowFlag,
}

/// Create a new SPSC audio ring buffer pair.
///
/// # Capacity
/// Capacity = `sample_rate × duration_secs` samples.
/// Default usage: 16000 × 5 = 80 000 samples (5 seconds at 16 kHz).
pub fn audio_ring_buffer(sample_rate: u32, duration_secs: f32) -> (AudioProducer, AudioConsumer) {
    let capacity = (sample_rate as f32 * duration_secs) as usize;
    let rb = HeapRb::<f32>::new(capacity);
    let (producer, consumer) = rb.split();
    let overflowed = Arc::new(AtomicBool::new(false));

    (
        AudioProducer {
            inner: producer,
            overflowed: Arc::clone(&overflowed),
        },
        AudioConsumer {
            inner: consumer,
            overflowed,
        },
    )
}

impl AudioProducer {
    /// Push samples from the capture callback thread.
    ///
    /// If the buffer is full, the oldest samples are silently dropped
    /// and the `overflowed` flag is set.
    pub fn push(&mut self, samples: &[f32]) {
        let written = self.inner.push_slice(samples);
        if written < samples.len() {
            let was_overflowed = self.overflowed.swap(true, Ordering::Relaxed);
            if !was_overflowed {
                log::warn!(
                    "Ring buffer overflow: dropped {} samples (further overflow logs suppressed until cleared)",
                    samples.len() - written
                );
            }
        }
    }
}

impl AudioConsumer {
    /// Pull exactly `count` samples for VAD processing.
    ///
    /// Returns `None` if fewer than `count` samples are available.
    pub fn pull(&mut self, count: usize) -> Option<Vec<f32>> {
        if self.inner.occupied_len() < count {
            return None;
        }
        let mut buf = vec![0.0f32; count];
        self.inner.pop_slice(&mut buf);
        Some(buf)
    }

    /// Returns the number of samples currently buffered.
    pub fn available(&self) -> usize {
        self.inner.occupied_len()
    }

    /// Check and clear the overflow flag.
    pub fn take_overflow_flag(&self) -> bool {
        self.overflowed.swap(false, Ordering::Relaxed)
    }
}
