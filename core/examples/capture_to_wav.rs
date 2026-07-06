//! Manual integration test: capture audio from a real process and save to WAV.
//!
//! Usage:
//!   cargo run --example capture_to_wav -- <PID> [duration_secs]
//!
//! Example:
//!   1. Open a music player, browser, or Discord
//!   2. Find its PID via Task Manager (Details tab)
//!   3. Run: cargo run --example capture_to_wav -- 12345 5
//!   4. Play audio in the target app during the capture window
//!   5. Check the generated `capture_output.wav` file

use std::time::{Duration, Instant};

use aura_core::audio::{AudioCapturer, audio_ring_buffer};
use aura_core::audio::capture::CaptureConfig;

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <PID> [duration_secs]", args[0]);
        eprintln!();
        eprintln!("  PID           - Target process ID (find via Task Manager)");
        eprintln!("  duration_secs - Capture duration in seconds (default: 5)");
        eprintln!();
        eprintln!("Example:");
        eprintln!("  {} 12345 5", args[0]);
        std::process::exit(1);
    }

    let target_pid: u32 = args[1].parse().expect("PID must be a number");
    let duration_secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    println!("=== Aura Capture Test ===");
    println!("Target PID:  {}", target_pid);
    println!("Duration:    {} seconds", duration_secs);
    println!("Output:      capture_output.wav (16kHz mono)");
    println!();

    // Create ring buffer: 16kHz, 2 seconds capacity (we drain it frequently)
    let (producer, mut consumer) = audio_ring_buffer(16000, 2.0);

    // Create capturer
    let config = CaptureConfig {
        target_pid,
        include_process_tree: true,
    };
    let mut capturer = AudioCapturer::new(config, producer);

    // Start capture
    println!("[*] Starting capture... Play audio in the target app now!");
    capturer.start().expect("Failed to start capture");

    // Continuously drain ring buffer into a Vec during capture
    let mut all_samples: Vec<f32> = Vec::new();
    let start = Instant::now();

    while start.elapsed() < Duration::from_secs(duration_secs) {
        // Drain whatever is available in the ring buffer
        let available = consumer.available();
        if available > 0 {
            if let Some(samples) = consumer.pull(available) {
                all_samples.extend_from_slice(&samples);
            }
        }

        let elapsed = start.elapsed().as_secs();
        let remaining = duration_secs.saturating_sub(elapsed);
        print!(
            "\r[*] Capturing... {}s remaining, {} samples collected   ",
            remaining,
            all_samples.len()
        );

        // Sleep briefly to avoid busy-waiting, but drain frequently enough
        std::thread::sleep(Duration::from_millis(50));
    }

    // Final drain
    let available = consumer.available();
    if available > 0 {
        if let Some(samples) = consumer.pull(available) {
            all_samples.extend_from_slice(&samples);
        }
    }
    println!();

    // Stop capture
    println!("[*] Stopping capture...");
    capturer.stop().expect("Failed to stop capture");

    // Check overflow
    if consumer.take_overflow_flag() {
        println!("[!] WARNING: Some ring buffer overflow occurred (minor data loss)");
    }

    let total_samples = all_samples.len();
    println!(
        "[*] Total samples captured: {} ({:.2}s at 16kHz)",
        total_samples,
        total_samples as f64 / 16000.0
    );

    if total_samples == 0 {
        println!("[!] No audio captured. Make sure:");
        println!("    - The target PID is correct");
        println!("    - The target app is actually producing audio");
        println!("    - You're running on Windows 10+ with WASAPI support");
        return;
    }

    // Write to WAV using hound
    let wav_path = "capture_output.wav";
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(wav_path, spec).expect("Failed to create WAV file");

    for &sample in &all_samples {
        writer.write_sample(sample).expect("Failed to write sample");
    }
    writer.finalize().expect("Failed to finalize WAV");

    println!("[OK] Saved to: {}", wav_path);
    println!("[OK] Play it:  start {}", wav_path);
}
