//! Unit tests for audio capture data processing logic.

use super::*;

/// Helper: encode a stereo f32 frame (left, right) into bytes and push to deque.
fn push_stereo_frame(queue: &mut VecDeque<u8>, left: f32, right: f32) {
    for b in left.to_le_bytes() {
        queue.push_back(b);
    }
    for b in right.to_le_bytes() {
        queue.push_back(b);
    }
}

#[test]
fn stereo_to_mono_mixdown() {
    // Feed 480 stereo frames (10ms at 48kHz) with known L/R values
    let mut queue = VecDeque::new();
    let num_frames = 480;
    for _i in 0..num_frames {
        let left = 0.8_f32;
        let right = 0.2_f32;
        push_stereo_frame(&mut queue, left, right);
    }

    let ring_buffer = Arc::new(AudioRingBuffer::new(TARGET_SAMPLE_RATE, 5.0));
    let mut resampler = Resampler::new(DEVICE_SAMPLE_RATE, TARGET_SAMPLE_RATE);
    let blockalign = 8; // f32 stereo = 8 bytes

    process_sample_queue(&mut queue, blockalign, &mut resampler, &ring_buffer);

    // Queue should be fully consumed
    assert_eq!(queue.len(), 0);

    // Ring buffer should contain ~160 samples (480 / 3 ratio)
    let available = ring_buffer.available();
    assert!(
        (available as i32 - 160).abs() <= 1,
        "Expected ~160 samples, got {}",
        available
    );

    // Pull and verify values are close to (0.8 + 0.2) / 2 = 0.5
    if let Some(samples) = ring_buffer.pull(available) {
        for (i, &s) in samples.iter().enumerate() {
            assert!(
                (s - 0.5).abs() < 0.05,
                "Sample {} = {}, expected ~0.5",
                i, s
            );
        }
    }
}

#[test]
fn empty_queue_is_noop() {
    let mut queue = VecDeque::new();
    let ring_buffer = Arc::new(AudioRingBuffer::new(TARGET_SAMPLE_RATE, 5.0));
    let mut resampler = Resampler::new(DEVICE_SAMPLE_RATE, TARGET_SAMPLE_RATE);

    process_sample_queue(&mut queue, 8, &mut resampler, &ring_buffer);

    assert_eq!(ring_buffer.available(), 0);
}

#[test]
fn partial_frame_preserved_in_queue() {
    // Push 2.5 frames worth of bytes (20 bytes = 2 full frames + 4 leftover)
    let mut queue = VecDeque::new();
    push_stereo_frame(&mut queue, 1.0, 1.0);
    push_stereo_frame(&mut queue, 0.5, 0.5);
    // Add 4 extra bytes (half a frame)
    for b in 0.3_f32.to_le_bytes() {
        queue.push_back(b);
    }

    let ring_buffer = Arc::new(AudioRingBuffer::new(TARGET_SAMPLE_RATE, 5.0));
    let mut resampler = Resampler::new(DEVICE_SAMPLE_RATE, TARGET_SAMPLE_RATE);

    process_sample_queue(&mut queue, 8, &mut resampler, &ring_buffer);

    // 4 leftover bytes should remain in queue
    assert_eq!(queue.len(), 4, "Partial frame bytes should be preserved");
}

#[test]
fn resample_ratio_is_correct() {
    // Feed exactly 4800 frames = 100ms at 48kHz → expect ~1600 samples at 16kHz
    let mut queue = VecDeque::new();
    let num_frames = 4800;
    for i in 0..num_frames {
        let t = i as f32 / DEVICE_SAMPLE_RATE as f32;
        let val = (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        push_stereo_frame(&mut queue, val, val);
    }

    let ring_buffer = Arc::new(AudioRingBuffer::new(TARGET_SAMPLE_RATE, 5.0));
    let mut resampler = Resampler::new(DEVICE_SAMPLE_RATE, TARGET_SAMPLE_RATE);

    process_sample_queue(&mut queue, 8, &mut resampler, &ring_buffer);

    let available = ring_buffer.available();
    assert!(
        (available as i32 - 1600).abs() <= 1,
        "Expected ~1600 samples for 100ms at 16kHz, got {}",
        available
    );
}
