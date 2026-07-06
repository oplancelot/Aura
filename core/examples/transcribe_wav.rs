//! Offline WAV transcription tool: read a WAV file, run ASR, output text.
//!
//! Usage:
//!   cargo run --release --example transcribe_wav -- <wav_path>
//!
//! If the WAV file has a corresponding entry in LJSpeech/metadata.csv,
//! the reference text is auto-extracted for comparison.
//!
//! Examples:
//!   cargo run --release --example transcribe_wav -- "../OpenSLR/LJSpeech/wavs/LJ001-0001.wav"
//!   cargo run --release --example transcribe_wav -- "../LibriSpeech/.../84-121123-0000.flac"
//!
//! Cross-referencing:
//!   - LJSpeech: filename prefix (e.g. LJ001-0001) is matched against metadata.csv column 1
//!   - LibriSpeech: filename prefix (e.g. 84-121123-0000) is matched against .trans.txt

use std::time::Instant;
use std::path::Path;

use aura_core::ai::sensevoice::SenseVoiceEngine;

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Warn)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <wav_path> [--reference \"ref text\"]", args[0]);
        std::process::exit(1);
    }

    let wav_path = &args[1];
    let wav_file = Path::new(wav_path);

    // Auto-extract reference text from metadata.csv or .trans.txt
    let reference = extract_reference(wav_file);

    // Resolve model paths
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asr_model = manifest_dir.join("..").join("models").join("sense-voice-small-q4_k.gguf");
    if !asr_model.exists() {
        eprintln!("ASR model not found at: {}", asr_model.display());
        std::process::exit(1);
    }

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
    println!("Input: {:.1}s at 16 kHz ({} samples)", pcm.len() as f64 / 16000.0, pcm.len());

    // Load SenseVoice ASR model
    let sv = SenseVoiceEngine::new(asr_model.to_str().unwrap())
        .expect("Failed to load SenseVoice model");

    // Process entire audio in fixed 30s chunks with 2s overlap
    let chunk_len = 30 * 16000;   // 30 seconds at 16 kHz
    let hop_len  = 28 * 16000;    // 28 second hop = 2s overlap

    let mut pos = 0;
    let mut total_asr_ms = 0u64;
    let mut full_text = String::new();

    println!("\n--- Transcription ---");
    let start = Instant::now();

    while pos < pcm.len() {
        let end = (pos + chunk_len).min(pcm.len());
        let chunk = &pcm[pos..end];
        let ts = pos as f64 / 16000.0;
        let chunk_sec = chunk.len() as f64 / 16000.0;

        if chunk_sec < 0.5 {
            break; // skip tiny trailing fragments
        }

        let t0 = Instant::now();
        match sv.transcribe(chunk) {
            Ok(text) if !text.is_empty() => {
                let elapsed = t0.elapsed();
                total_asr_ms += elapsed.as_millis() as u64;
                println!("[{:>6.1}s] {}", ts, text);
                if !full_text.is_empty() { full_text.push(' '); }
                full_text.push_str(&text);
            }
            Ok(_) => {
                println!("[{:>6.1}s] [-] (no speech in {:.1}s chunk)", ts, chunk_sec);
            }
            Err(e) => {
                println!("[{:>6.1}s] [!] ASR error: {}", ts, e);
            }
        }

        pos += hop_len;
    }

    let total_sec = start.elapsed().as_secs_f64();
    let audio_len = pcm.len() as f64 / 16000.0;
    println!("\n--- Summary ---");
    println!("Audio: {:.1}s | Processing: {:.1}s | ASR: {}ms total | RTF: {:.2}x",
        audio_len, total_sec, total_asr_ms, total_sec / audio_len);

    println!("\n=== Full transcription ===");
    println!("{}", full_text);

    if let Some(ref_text) = reference {
        println!("\n=== Reference ===");
        println!("{}", ref_text);
        let wer = simple_wer(&full_text, &ref_text);
        println!("WER: {:.1}%", wer);
    }
}

/// Extract reference text from LJSpeech metadata.csv or LibriSpeech .trans.txt
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

/// Simple word error rate (Levenshtein at word level).
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
