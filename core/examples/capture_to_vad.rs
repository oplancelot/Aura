//! Manual integration test: capture audio and stream through VAD and state machine.
//!
//! Usage:
//!   cargo run --example capture_to_vad -- <PID> [duration_secs]
//!
//! Example:
//!   1. Open a music player, browser, or Discord
//!   2. Find its PID via Task Manager (Details tab)
//!   3. Run: cargo run --example capture_to_vad -- 12345 30
//!   4. Speak or play audio with pauses to see chunks being emitted

use std::time::{Duration, Instant};

use aura_core::audio::capture::CaptureConfig;
use aura_core::audio::{AudioCapturer, audio_ring_buffer};
use aura_core::vad::silero::SileroVad;
use aura_core::vad::state_machine::{ChunkingConfig, ChunkingStateMachine, ChunkType};

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Info)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <PID> [duration_secs]", args[0]);
        std::process::exit(1);
    }

    let target_pid: u32 = args[1].parse().expect("PID must be a number");
    let duration_secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);

    println!("=== Aura VAD + State Machine Test ===");
    println!("Target PID:  {}", target_pid);
    println!("Duration:    {} seconds", duration_secs);
    println!();

    // Initialize VAD and State Machine
    println!("[*] Loading Silero VAD ONNX model...");
    let mut vad = SileroVad::new("assets/silero_vad.onnx").expect("Failed to load VAD model");
    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());

    // Create ring buffer
    let (producer, mut consumer) = audio_ring_buffer(16000, 2.0);

    // Create capturer
    let config = CaptureConfig {
        target_pid,
        include_process_tree: true,
    };
    let mut capturer = AudioCapturer::new(config, producer);

    // Start capture
    println!("[*] Starting capture... Speak/Play audio with pauses now!");
    capturer.start().expect("Failed to start capture");

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create("vad_debug.wav", spec).expect("Failed to create WAV file");

    let mut chunk_buffer: Vec<f32> = Vec::new();
    let start = Instant::now();
    let mut last_print = Instant::now();
    let mut is_speaking = false;
    let mut max_prob = 0.0f32;
    let mut max_amp = 0.0f32;
    let mut chunk_counter = 0;

    while start.elapsed() < Duration::from_secs(duration_secs) {
        let available = consumer.available();
        if available > 0 {
            if let Some(samples) = consumer.pull(available) {
                chunk_buffer.extend_from_slice(&samples);

                // Process every 512 samples
                while chunk_buffer.len() >= SileroVad::AUDIO_SAMPLES {
                    let frame: Vec<f32> = chunk_buffer.drain(..SileroVad::AUDIO_SAMPLES).collect();
                    
                    for &s in &frame {
                        writer.write_sample(s).unwrap();
                    }

                    let vad_result = vad.process_frame(&frame).expect("VAD inference failed");

                    if vad_result.probability > max_prob {
                        max_prob = vad_result.probability;
                    }
                    for &s in &frame {
                        if s.abs() > max_amp {
                            max_amp = s.abs();
                        }
                    }

                    if vad_result.is_speech && !is_speaking {
                        is_speaking = true;
                        println!("\n[🎤] Speech Started! (Prob: {:.2})", vad_result.probability);
                    } else if !vad_result.is_speech && is_speaking {
                        is_speaking = false;
                        println!("\n[🔇] Silence... (Prob: {:.2})", vad_result.probability);
                    }

                    if let Some(chunk) = state_machine.feed(&vad_result, &frame) {
                        let duration_ms = chunk.samples.len() as f64 / 16.0;
                        match chunk.chunk_type {
                            ChunkType::Provisional => {
                                println!("  => [⚡ PROVISIONAL] chunk emitted: {:.0} ms", duration_ms);
                            }
                            ChunkType::Final => {
                                chunk_counter += 1;
                                let filename = format!("chunk_{:03}_final.wav", chunk_counter);
                                let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
                                let mut cw = hound::WavWriter::create(&filename, spec).unwrap();
                                for &s in &chunk.samples { cw.write_sample(s).unwrap(); }
                                cw.finalize().unwrap();
                                println!("  => [✅ FINAL] sentence chunk emitted: {:.0} ms (Saved to {})", duration_ms, filename);
                                println!("---------------------------------------------------");
                            }
                            ChunkType::HardCut => {
                                chunk_counter += 1;
                                let filename = format!("chunk_{:03}_hardcut.wav", chunk_counter);
                                let spec = hound::WavSpec { channels: 1, sample_rate: 16000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
                                let mut cw = hound::WavWriter::create(&filename, spec).unwrap();
                                for &s in &chunk.samples { cw.write_sample(s).unwrap(); }
                                cw.finalize().unwrap();
                                println!("  => [✂️ HARD CUT] chunk emitted: {:.0} ms (Saved to {})", duration_ms, filename);
                                println!("---------------------------------------------------");
                            }
                        }
                    }
                }
            }
        }

        if last_print.elapsed() > Duration::from_secs(1) {
            let elapsed = start.elapsed().as_secs();
            let remaining = duration_secs.saturating_sub(elapsed);
            println!("[*] Capturing... {}s remaining (Max prob: {:.4}, Max Amp: {:.4})", remaining, max_prob, max_amp);
            last_print = Instant::now();
            max_prob = 0.0;
            max_amp = 0.0;
        }

        // Sleep less than the 32ms frame size to poll actively
        std::thread::sleep(Duration::from_millis(10));
    }

    println!("\n[*] Stopping capture...");
    capturer.stop().expect("Failed to stop capture");
    writer.finalize().expect("Failed to write WAV");
    println!("[OK] Saved VAD input audio to 'vad_debug.wav' for debugging!");
    println!("[OK] Done!");
}
