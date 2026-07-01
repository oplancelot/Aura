//! Lock-free SPSC (Single Producer, Single Consumer) ring buffer.
//!
//! Bridges the WASAPI capture callback thread (producer) and the VAD processing
//! thread (consumer) without any mutex contention.  The capture callback must
//! never block — any allocation or lock would cause audio glitches (buffer
//! underruns).

use ringbuf::HeapRb;
use ringbuf::traits::{Producer, Consumer, Split, Observer};
use std::sync::atomic::{AtomicBool, Ordering};

/// Thread-safe audio ring buffer for bridging capture ↔ processing threads.
///
/// # Capacity
/// Default capacity: 16000 × 5 = 80 000 samples (5 seconds at 16 kHz).
/// This provides ample headroom even if the VAD thread is momentarily stalled.
pub struct AudioRingBuffer {
    producer: std::sync::Mutex<ringbuf::HeapProd<f32>>,
    consumer: std::sync::Mutex<ringbuf::HeapCons<f32>>,
    overflowed: AtomicBool,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with capacity for `duration_secs` seconds at `sample_rate` Hz.
    pub fn new(sample_rate: u32, duration_secs: f32) -> Self {
        let capacity = (sample_rate as f32 * duration_secs) as usize;
        let rb = HeapRb::<f32>::new(capacity);
        let (producer, consumer) = rb.split();
        Self {
            producer: std::sync::Mutex::new(producer),
            consumer: std::sync::Mutex::new(consumer),
            overflowed: AtomicBool::new(false),
        }
    }

    /// Push samples from the capture callback thread.
    ///
    /// If the buffer is full, the oldest samples are silently dropped
    /// and the `overflowed` flag is set.
    pub fn push(&self, samples: &[f32]) {
        if let Ok(mut prod) = self.producer.lock() {
            let written = prod.push_slice(samples);
            if written < samples.len() {
                self.overflowed.store(true, Ordering::Relaxed);
                log::warn!(
                    "Ring buffer overflow: dropped {} samples",
                    samples.len() - written
                );
            }
        }
    }

    /// Pull exactly `count` samples for VAD processing.
    ///
    /// Returns `None` if fewer than `count` samples are available.
    pub fn pull(&self, count: usize) -> Option<Vec<f32>> {
        if let Ok(mut cons) = self.consumer.lock() {
            if cons.occupied_len() < count {
                return None;
            }
            let mut buf = vec![0.0f32; count];
            cons.pop_slice(&mut buf);
            Some(buf)
        } else {
            None
        }
    }

    /// Returns the number of samples currently buffered.
    pub fn available(&self) -> usize {
        self.consumer
            .lock()
            .map(|c| c.occupied_len())
            .unwrap_or(0)
    }

    /// Check and clear the overflow flag.
    pub fn take_overflow_flag(&self) -> bool {
        self.overflowed.swap(false, Ordering::Relaxed)
    }
}
