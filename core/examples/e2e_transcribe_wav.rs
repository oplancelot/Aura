use std::time::{Duration, Instant};
use std::path::Path;

use aura_core::vad::{SileroVad, ChunkingStateMachine, ChunkType};
use aura_core::vad::state_machine::ChunkingConfig;
use aura_core::ai::sensevoice::SenseVoiceEngine;

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Warn)
        .init();

    let args: Vec<String> = std::env::args().collect();
    // Default is Accuracy mode (no per-frame sleep). Pass --realtime to
    // simulate ~16ms frame pacing for latency-oriented runs.
    let realtime = args.iter().any(|a| a == "--realtime");
    let wav_path = args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str());
    let Some(wav_path) = wav_path else {
        eprintln!("Usage: {} <wav_path> [--realtime]", args[0]);
        eprintln!("  default: Accuracy mode (fast, no frame sleep)");
        eprintln!("  --realtime: sleep ~16ms per VAD frame (simulates live capture)");
        std::process::exit(1);
    };
    let wav_file = Path::new(wav_path);
    let mode_name = if realtime { "realtime" } else { "accuracy" };
    println!("Mode: {mode_name}");

    // Auto-extract reference text from metadata.csv or .trans.txt
    let reference = extract_reference(wav_file);

    // Resolve model paths
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vad_model = manifest_dir.join("..").join("assets").join("silero_vad.onnx");
    let asr_model = manifest_dir.join("..").join("assets").join("sense-voice-small-q4_k.gguf");

    if !vad_model.exists() {
        eprintln!("VAD model not found at: {}", vad_model.display());
        eprintln!("Download the Silero VAD model to continue.");
        std::process::exit(1);
    }
    if !asr_model.exists() {
        eprintln!("ASR model not found at: {}", asr_model.display());
        eprintln!("Download a SenseVoice GGUF model to continue.");
        std::process::exit(1);
    }

    // Initialize ONNX Runtime
    let _ = ort::init().with_name("aura").commit();

    // Read WAV file
    let mut reader = hound::WavReader::open(wav_path)
        .expect("Failed to open WAV file");
    let spec = reader.spec();
    println!("WAV: {} channels, {} Hz, {:?} format",
        spec.channels, spec.sample_rate, spec.sample_format);

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect()
        }
        hound::SampleFormat::Int => {
            let max = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>()
                .map(|s| s.unwrap_or(0) as f32 / max)
                .collect()
        }
    };
    let audio_sec = samples.len() as f64 / spec.sample_rate as f64;
    println!("Read {} samples ({:.1}s)", samples.len(), audio_sec);

    // Downmix to mono
    let mono: Vec<f32> = if spec.channels == 2 {
        samples.chunks(2).map(|c| (c[0] + c[1]) * 0.5).collect()
    } else {
        samples
    };

    // Resample to 16 kHz by simple decimation
    let pcm: Vec<f32> = if spec.sample_rate != 16000 {
        let ratio = spec.sample_rate as f64 / 16000.0;
        (0..)
            .map(|i| (i as f64 * ratio) as usize)
            .take_while(|&idx| idx < mono.len())
            .map(|idx| mono[idx])
            .collect()
    } else {
        mono
    };
    let pcm_len = pcm.len();
    let audio_duration = pcm_len as f64 / 16000.0;
    println!("Input: {:.1}s at 16 kHz ({} samples)", audio_duration, pcm_len);

    // --- VAD ---
    let mut vad = SileroVad::new(vad_model.to_str().unwrap())
        .expect("Failed to load Silero VAD model");

    // Warm-up frame (same as real pipeline)
    let warmup = vec![0.0f32; SileroVad::AUDIO_SAMPLES];
    let _ = vad.process_frame(&warmup);
    vad.reset_state();

    let mut state_machine = ChunkingStateMachine::new(ChunkingConfig::default());
    let sv = SenseVoiceEngine::new(asr_model.to_str().unwrap())
        .expect("Failed to load SenseVoice model");

    // --- Pipeline loop ---
    let mut pos = 0;
    let mut chunk_index = 0u32;
    let mut e2e_text = String::new();
    let mut total_asr_ms = 0u64;

    // Segmentation tracking
    let mut chunk_durations: Vec<f64> = Vec::new();
    let mut final_count = 0u32;
    let mut hardcut_count = 0u32;
    let mut provisional_count = 0u32;
    let start = Instant::now();

    println!("\n--- Utterance breakdown ---");

    while pos + SileroVad::AUDIO_SAMPLES <= pcm_len && chunk_index < 10000 {
        let frame = &pcm[pos..pos + SileroVad::AUDIO_SAMPLES];
        let vad_result = vad.process_frame(frame)
            .expect("VAD inference failed");
        if realtime {
            // Simulate real-time frame interval (~32ms audio @ 16kHz / ~16ms wall pacing)
            std::thread::sleep(Duration::from_millis(16));
        }

        if let Some(chunk) = state_machine.feed(&vad_result, frame) {
            chunk_index += 1;
            let chunk_sec = chunk.samples.len() as f64 / 16000.0;
            chunk_durations.push(chunk_sec);

            match chunk.chunk_type {
                ChunkType::Provisional => {
                    provisional_count += 1;
                    println!("#{chunk_index:4}  Provisional  {chunk_sec:6.1}s chunk  {asr:>6}  [preview] \"{snippet}...\"",
                        asr = "0ms ASR".to_string(),
                        snippet = &preview_text(&chunk.samples, 40));
                }
                ChunkType::Final => {
                    final_count += 1;
                    asr_and_log(&sv, &mut vad, chunk_index, "Final", &chunk.samples, chunk_sec, &mut e2e_text, &mut total_asr_ms);
                }
                ChunkType::HardCut => {
                    hardcut_count += 1;
                    asr_and_log(&sv, &mut vad, chunk_index, "HardCut", &chunk.samples, chunk_sec, &mut e2e_text, &mut total_asr_ms);
                }
            }
        }

        pos += SileroVad::AUDIO_SAMPLES;
    }

    // Flush: feed silence frames to flush any in-progress utterance
    let silence_frame = vec![0.0f32; SileroVad::AUDIO_SAMPLES];
    for _ in 0..20 {
        if realtime {
            std::thread::sleep(Duration::from_millis(16));
        }
        let Ok(silence_result) = vad.process_frame(&silence_frame) else { break };
        if let Some(chunk) = state_machine.feed(&silence_result, &silence_frame) {
            match chunk.chunk_type {
                ChunkType::Final | ChunkType::HardCut => {
                    let chunk_sec = chunk.samples.len() as f64 / 16000.0;
                    chunk_durations.push(chunk_sec);
                    chunk_index += 1;
                    final_count += 1;
                    asr_and_log(&sv, &mut vad, chunk_index, "Final(flush)", &chunk.samples, chunk_sec, &mut e2e_text, &mut total_asr_ms);
                    break;
                }
                ChunkType::Provisional => {
                    // continue feeding silence for Final
                }
            }
        }
    }

    let total_sec = start.elapsed().as_secs_f64();

    // Compute segmentation stats
    let min_chunk = chunk_durations.iter().cloned().fold(f64::MAX, f64::min);
    let max_chunk = chunk_durations.iter().cloned().fold(f64::MIN, f64::max);
    let avg_chunk = if !chunk_durations.is_empty() {
        chunk_durations.iter().sum::<f64>() / chunk_durations.len() as f64
    } else {
        0.0
    };
    let total_chunks = final_count + hardcut_count + provisional_count;

    println!("\n--- Summary ---");
    println!("Audio: {:.1}s | Processing: {:.1}s | ASR: {}ms total | RTF: {:.2}x",
        audio_duration, total_sec, total_asr_ms, total_sec / audio_duration);

    println!("\n=== E2E Transcript ===");
    println!("{}", e2e_text);

    if let Some(ref_text) = reference {
        println!("\n=== Reference ===");
        println!("{}", ref_text);
        let wer = simple_wer(&e2e_text, &ref_text);
        println!("WER: {:.1}%", wer);
    }

    println!("\n=== Segmentation Quality ===");
    println!("Total chunks: {}  (Final: {}, HardCut: {}, Provisional: {})",
        total_chunks, final_count, hardcut_count, provisional_count);
    if !chunk_durations.is_empty() {
        println!("Avg chunk: {:.1}s  |  Min: {:.1}s  |  Max: {:.1}s",
            avg_chunk, min_chunk, max_chunk);
    }
}

fn asr_and_log(
    sv: &SenseVoiceEngine,
    vad: &mut SileroVad,
    chunk_index: u32,
    label: &str,
    samples: &[f32],
    chunk_sec: f64,
    e2e_text: &mut String,
    total_asr_ms: &mut u64,
) {
    let t0 = Instant::now();
    match sv.transcribe(samples) {
        Ok(text) if !text.is_empty() => {
            let elapsed = t0.elapsed();
            *total_asr_ms += elapsed.as_millis() as u64;
            if !e2e_text.is_empty() { e2e_text.push(' '); }
            e2e_text.push_str(&text);
            println!("#{chunk_index:4}  {label:<12}  {chunk_sec:6.1}s chunk  {:>3}ms ASR  \"{}\"", elapsed.as_millis(), text);
        }
        Ok(_) => {
            println!("#{chunk_index:4}  {label:<12}  {chunk_sec:6.1}s chunk     0ms ASR  (no speech)");
        }
        Err(e) => {
            println!("#{chunk_index:4}  {label:<12}  {chunk_sec:6.1}s chunk     0ms ASR  [!] ASR error: {e}");
        }
    }
    vad.reset_state();
}

fn preview_text(samples: &[f32], _max_chars: usize) -> String {
    // Simple heuristic: energy-based estimate of speech content
    let energy: f32 = samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32;
    if energy < 0.001 {
        return "(silence)".to_string();
    }
    format!("(energy={:.4})", energy)
}

fn extract_reference(wav_path: &Path) -> Option<String> {
    let filename = wav_path.file_stem()?.to_str()?;

    // Try LJSpeech metadata.csv (column 0 = filename, column 1 = ref text)
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lj_csv = manifest_dir.join("..").join("OpenSLR").join("LJSpeech").join("metadata.csv");
    if lj_csv.exists() {
        if let Ok(content) = std::fs::read_to_string(&lj_csv) {
            for line in content.lines() {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 2 && parts[0] == filename {
                    return Some(parts[1].to_string());
                }
            }
        }
    }

    // Try LibriSpeech .trans.txt (same dir, named by speaker)
    if let Some(parent) = wav_path.parent() {
        let parent_trans = parent.with_extension("trans.txt");
        if parent_trans.exists() {
            if let Ok(content) = std::fs::read_to_string(&parent_trans) {
                for line in content.lines() {
                    if line.starts_with(filename) {
                        let text = line.splitn(2, ' ').nth(1).unwrap_or("").to_string();
                        return Some(text);
                    }
                }
            }
        }
    }

    None
}

fn simple_wer(hyp: &str, ref_: &str) -> f64 {
    let hyp_lower = hyp.to_lowercase();
    let ref_lower = ref_.to_lowercase();
    let hyp_words: Vec<&str> = hyp_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let ref_words: Vec<&str> = ref_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    let h = hyp_words.len();
    let r = ref_words.len();
    if r == 0 { return if h == 0 { 0.0 } else { 100.0 }; }

    let mut dp = vec![vec![0usize; r + 1]; h + 1];
    for i in 0..=h { dp[i][0] = i; }
    for j in 0..=r { dp[0][j] = j; }
    for i in 1..=h {
        for j in 1..=r {
            let cost = if hyp_words[i-1] == ref_words[j-1] { 0 } else { 1 };
            dp[i][j] = (dp[i-1][j] + 1)
                .min(dp[i][j-1] + 1)
                .min(dp[i-1][j-1] + cost);
        }
    }
    dp[h][r] as f64 / r as f64 * 100.0
}
