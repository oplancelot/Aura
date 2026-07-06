//! VAD diagnostic tool: read a WAV/raw file, run Silero VAD, output per-frame probabilities.
//!
//! Usage:
//!   cargo run --release --example test_vad -- <file.wav>
//!   cargo run --release --example test_vad -- <file.raw> 16000
//!
//! Output: CSV to stdout, one line per 32ms frame.
//!
//! For raw files: 16kHz mono f32, no header.

use std::path::Path;
use std::time::Instant;

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Warn)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <file.wav>", args[0]);
        eprintln!("       {} <file.raw> <sample_rate>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let pcm = if path.ends_with(".wav") {
        load_wav(path)
    } else {
        load_raw(path, args.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(16000))
    };

    let sample_rate = 16000;
    println!("Input: {} samples ({:.1}s at {} Hz)", pcm.len(), pcm.len() as f64 / sample_rate as f64, sample_rate);

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("assets").join("silero_vad.onnx");
    if !model_path.exists() {
        eprintln!("VAD model not found at: {}", model_path.display());
        std::process::exit(1);
    }

    let mut vad = aura_core::vad::silero::SileroVad::new(model_path.to_str().unwrap())
        .unwrap_or_else(|e| { eprintln!("Failed to load VAD model: {}", e); std::process::exit(1); });

    let frame_size = 512;
    let mut total_frames = 0;
    let mut speech_frames = 0;
    let mut max_prob = 0.0f32;
    let start = Instant::now();

    println!();
    println!("=== VAD Results ===");
    println!("elapsed_ms\tprobability\tis_speech");
    println!("----------\t-----------\t---------");

    for chunk in pcm.chunks(frame_size) {
        if chunk.len() < frame_size {
            break;
        }
        let frame: Vec<f32> = chunk.to_vec();
        match vad.process_frame(&frame) {
            Ok(result) => {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                println!("{:.1}\t{:.4}\t{}", elapsed, result.probability, result.is_speech as u8);
                total_frames += 1;
                if result.is_speech {
                    speech_frames += 1;
                }
                if result.probability > max_prob {
                    max_prob = result.probability;
                }
            }
            Err(e) => {
                eprintln!("VAD error at frame {}: {}", total_frames, e);
            }
        }
    }

    println!();
    println!("=== Summary ===");
    println!("Total frames: {} ({:.1}s)", total_frames, total_frames as f64 * 0.032);
    println!("Speech frames: {} ({:.1}%)", speech_frames, speech_frames as f64 / total_frames as f64 * 100.0);
    println!("Max probability: {:.4}", max_prob);
}

fn load_wav(path: &str) -> Vec<f32> {
    let mut reader = hound::WavReader::open(path).expect("Failed to open WAV");
    let spec = reader.spec();
    eprintln!("WAV: {} channels, {} Hz, {:?}", spec.channels, spec.sample_rate, spec.sample_format);

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
        }
        hound::SampleFormat::Int => {
            let max = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 / max).collect()
        }
    };

    // Downmix to mono
    let mono: Vec<f32> = if spec.channels == 2 {
        samples.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
    } else {
        samples
    };

    // Resample to 16 kHz
    if spec.sample_rate != 16000 {
        let ratio = spec.sample_rate as f64 / 16000.0;
        (0..)
            .map(|i| (i as f64 * ratio) as usize)
            .take_while(|&idx| idx < mono.len())
            .map(|idx| mono[idx])
            .collect()
    } else {
        mono
    }
}

fn load_raw(path: &str, sample_rate: u32) -> Vec<f32> {
    let data = std::fs::read(path).expect("Failed to read raw file");
    let samples: Vec<f32> = data.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    eprintln!("RAW: {} samples ({} bytes, {} Hz)", samples.len(), data.len(), sample_rate);

    // Resample to 16 kHz if needed
    if sample_rate != 16000 {
        let ratio = sample_rate as f64 / 16000.0;
        (0..)
            .map(|i| (i as f64 * ratio) as usize)
            .take_while(|&idx| idx < samples.len())
            .map(|idx| samples[idx])
            .collect()
    } else {
        samples
    }
}
