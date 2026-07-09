# E2E Transcribe: VAD+ASR Pipeline Accuracy Test — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `e2e_transcribe_wav` example and `run_e2e_batch.ps1` script to measure E2E pipeline WER and segmentation quality on LJSpeech.

**Architecture:** A new example that feeds WAV audio through the real SileroVAD → ChunkingStateMachine → SenseVoiceEngine pipeline (identical to Aura's runtime), collects Final/HardCut utterances, computes WER against reference, and outputs segmentation quality metrics. A batch PowerShell script wraps it for LJSpeech dataset.

**Tech Stack:** Rust (`aura_core` crate: `vad::silero::SileroVad`, `vad::state_machine::ChunkingStateMachine`, `ai::sensevoice::SenseVoiceEngine`), PowerShell 7, hound (WAV I/O)

## Global Constraints

- Must not modify existing `transcribe_wav.rs`, `run_asr_batch.ps1`, or any pipeline code
- VAD model path: `CARGO_MANIFEST_DIR/../assets/silero_vad.onnx`
- ASR model path: `CARGO_MANIFEST_DIR/../models/sense-voice-small-q4_k.gguf`
- Only Final + HardCut chunks contribute to E2E text and WER
- Provisional chunks logged with `[preview]` prefix, not sent to ASR
- Terminal output format must be parseable by batch script (WER / ASR / Processing patterns)

---

### Task 1: `core/examples/e2e_transcribe_wav.rs`

**Files:**
- Create: `core/examples/e2e_transcribe_wav.rs`

**Interfaces:**
- Consumes: `aura_core::vad::SileroVad`, `aura_core::vad::ChunkingStateMachine`, `aura_core::vad::ChunkingConfig`, `aura_core::vad::AudioChunk`, `aura_core::vad::ChunkType`, `aura_core::ai::SenseVoiceEngine`
- Produces: Standalone binary that processes a WAV file and outputs E2E transcription + WER + segmentation stats to stdout

- [ ] **Step 1: Write the full example**

```rust
use std::time::Instant;
use std::path::Path;

use aura_core::vad::{SileroVad, ChunkingStateMachine, ChunkingConfig, AudioChunk, ChunkType};
use aura_core::ai::sensevoice::SenseVoiceEngine;

fn main() {
    env_logger::Builder::new()
        .filter_level(log::LevelFilter::Warn)
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <wav_path>", args[0]);
        std::process::exit(1);
    }

    let wav_path = &args[1];
    let wav_file = Path::new(wav_path);

    // Auto-extract reference text from metadata.csv or .trans.txt
    let reference = extract_reference(wav_file);

    // Resolve model paths
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let vad_model = manifest_dir.join("..").join("assets").join("silero_vad.onnx");
    let asr_model = manifest_dir.join("..").join("models").join("sense-voice-small-q4_k.gguf");

    if !vad_model.exists() {
        eprintln!("VAD model not found at: {}", vad_model.display());
        std::process::exit(1);
    }
    if !asr_model.exists() {
        eprintln!("ASR model not found at: {}", asr_model.display());
        std::process::exit(1);
    }

    // Initialize ONNX Runtime
    let _ort = ort::init().with_name("aura").commit()
        .unwrap_or_else(|e| { log::warn!("ORT init: {e}"); });

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
    let mut flush_used = false;

    let start = Instant::now();

    println!("\n--- Utterance breakdown ---");

    while pos + SileroVad::AUDIO_SAMPLES <= pcm_len && chunk_index < 100 {
        let frame = &pcm[pos..pos + SileroVad::AUDIO_SAMPLES];
        let vad_result = vad.process_frame(frame)
            .expect("VAD inference failed");

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
                    let t0 = Instant::now();
                    match sv.transcribe(&chunk.samples) {
                        Ok(text) if !text.is_empty() => {
                            let elapsed = t0.elapsed();
                            total_asr_ms += elapsed.as_millis() as u64;
                            if !e2e_text.is_empty() { e2e_text.push(' '); }
                            e2e_text.push_str(&text);
                            println!("#{chunk_index:4}  Final        {chunk_sec:6.1}s chunk  {:>3}ms ASR  \"{}\"", elapsed.as_millis(), text);
                        }
                        Ok(_) => {
                            println!("#{chunk_index:4}  Final        {chunk_sec:6.1}s chunk     0ms ASR  (no speech)");
                        }
                        Err(e) => {
                            println!("#{chunk_index:4}  Final        {chunk_sec:6.1}s chunk     0ms ASR  [!] ASR error: {e}");
                        }
                    }
                    vad.reset_state();
                }
                ChunkType::HardCut => {
                    hardcut_count += 1;
                    let t0 = Instant::now();
                    match sv.transcribe(&chunk.samples) {
                        Ok(text) if !text.is_empty() => {
                            let elapsed = t0.elapsed();
                            total_asr_ms += elapsed.as_millis() as u64;
                            if !e2e_text.is_empty() { e2e_text.push(' '); }
                            e2e_text.push_str(&text);
                            println!("#{chunk_index:4}  HardCut      {chunk_sec:6.1}s chunk  {:>3}ms ASR  \"{}\"", elapsed.as_millis(), text);
                        }
                        Ok(_) => {
                            println!("#{chunk_index:4}  HardCut      {chunk_sec:6.1}s chunk     0ms ASR  (no speech)");
                        }
                        Err(e) => {
                            println!("#{chunk_index:4}  HardCut      {chunk_sec:6.1}s chunk     0ms ASR  [!] ASR error: {e}");
                        }
                    }
                    vad.reset_state();
                }
            }
        }

        pos += SileroVad::AUDIO_SAMPLES;
    }

    // Flush: if any remaining audio didn't trigger Final/HardCut
    if pos < pcm_len {
        let remaining = &pcm[pos..];
        if remaining.len() >= 16000 { // at least 1 second
            flush_used = true;
            chunk_index += 1;
            let chunk_sec = remaining.len() as f64 / 16000.0;
            chunk_durations.push(chunk_sec);
            final_count += 1;

            let t0 = Instant::now();
            match sv.transcribe(remaining) {
                Ok(text) if !text.is_empty() => {
                    let elapsed = t0.elapsed();
                    total_asr_ms += elapsed.as_millis() as u64;
                    if !e2e_text.is_empty() { e2e_text.push(' '); }
                    e2e_text.push_str(&text);
                    println!("#{chunk_index:4}  Final(flush){chunk_sec:6.1}s chunk  {:>3}ms ASR  \"{}\"", elapsed.as_millis(), text);
                }
                Ok(_) => {
                    println!("#{chunk_index:4}  Final(flush){chunk_sec:6.1}s chunk     0ms ASR  (no speech)");
                }
                Err(e) => {
                    println!("#{chunk_index:4}  Final(flush){chunk_sec:6.1}s chunk     0ms ASR  [!] ASR error: {e}");
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
    if flush_used {
        println!("  (includes 1 flush chunk)");
    }
    if !chunk_durations.is_empty() {
        println!("Avg chunk: {:.1}s  |  Min: {:.1}s  |  Max: {:.1}s",
            avg_chunk, min_chunk, max_chunk);
    }
}

fn preview_text(samples: &[f32], max_chars: usize) -> String {
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
```

- [ ] **Step 2: Build the example**

Run: `cargo build --release --example e2e_transcribe_wav`
Expected: Build succeeds, binary at `target/release/examples/e2e_transcribe_wav.exe`

- [ ] **Step 3: Test on a single LJSpeech file**

Run: `.\target\release\examples\e2e_transcribe_wav.exe "..\OpenSLR\LJSpeech\wavs\LJ001-0001.wav"`
Expected: Output shows utterance breakdown, E2E transcript, reference, WER, and segmentation quality

- [ ] **Step 4: Commit**

```bash
git add core/examples/e2e_transcribe_wav.rs
git commit -m "feat: add e2e_transcribe_wav example for VAD+ASR pipeline accuracy test"
```

---

### Task 2: `scripts/run_e2e_batch.ps1`

**Files:**
- Create: `scripts/run_e2e_batch.ps1`

**Interfaces:**
- Consumes: `e2e_transcribe_wav.exe` binary, LJSpeech WAVs at `OpenSLR/LJSpeech/wavs/`
- Produces: `e2e_batch_results.csv` with accuracy + segmentation columns, terminal summary

- [ ] **Step 1: Write the batch script**

```powershell
# LJSpeech E2E 管线测试
# 编译后遍历 WAV 文件运行 e2e_transcribe_wav
# Usage: .\scripts\run_e2e_batch.ps1 [max_files]
# 输出: e2e_batch_results.csv + terminal summary

param(
    [int]$MaxFiles = 0
)

$wavDir = "OpenSLR/LJSpeech/wavs"
$example = "core\target\release\examples\e2e_transcribe_wav.exe"

# Build once
Write-Host "Building e2e_transcribe_wav..."
Push-Location core
cargo build --release --example e2e_transcribe_wav 2>&1 | Out-Null
Pop-Location

if (-not (Test-Path $example)) {
    Write-Host "ERROR: e2e_transcribe_wav.exe not found at $example"
    exit 1
}

$wavs = Get-ChildItem "$wavDir/*.wav"
if ($MaxFiles -gt 0) {
    $wavs = $wavs | Select-Object -First $MaxFiles
}
$totalCount = $wavs.Count

$results = @()
$totalWER = 0.0
$totalAsrMs = 0.0
$totalProcessTime = 0.0
$totalChunks = 0
$totalFinal = 0
$totalHardCut = 0
$totalProvisional = 0
$chunkDurationList = @()
$multiChunkFiles = 0
$tested = 0

Write-Host "Testing $totalCount files...`n"

foreach ($wav in $wavs) {
    $name = $wav.BaseName
    Write-Progress -Activity "E2E Testing" -Status "$name ($tested/$totalCount)" -PercentComplete (($tested / $totalCount) * 100)

    $output = & $example $wav.FullName 2>$null

    $wer = $null
    $asrMs = 0.0
    $procTime = 0.0
    $audioTime = 0.0
    $chunks = 0
    $final = 0
    $hardCut = 0
    $provisional = 0

    # Parse summary lines
    foreach ($line in $output) {
        if ($line -match "^WER: ([\d.]+)%") { $wer = [double]$Matches[1] }
        elseif ($line -match "^Audio: ([\d.]+)s .* ASR: (\d+)ms") {
            $audioTime = [double]$Matches[1]
            $asrMs = [double]$Matches[2]
        }
        elseif ($line -match "Processing: ([\d.]+)s") { $procTime = [double]$Matches[1] }
        elseif ($line -match "^Total chunks: (\d+).*Final: (\d+), HardCut: (\d+), Provisional: (\d+)") {
            $chunks = [int]$Matches[1]
            $final = [int]$Matches[2]
            $hardCut = [int]$Matches[3]
            $provisional = [int]$Matches[4]
        }
        elseif ($line -match "^Avg chunk: ([\d.]+)s.*Min: ([\d.]+)s.*Max: ([\d.]+)s") {
            # store for aggregate stats
        }
    }

    Write-Host "[$($tested+1)/$totalCount] $name" -NoNewline
    if ($wer -ne $null) {
        Write-Host "  WER: ${wer}%  ASR: ${asrMs}ms"
        $results += [PSCustomObject]@{
            File = $name
            WER = $wer
            ASR_Time_ms = [math]::Round($asrMs, 0)
            Process_Time_s = [math]::Round($procTime, 2)
            Audio_Time_s = [math]::Round($audioTime, 1)
            Chunks = $chunks
            Final = $final
            HardCut = $hardCut
            Provisional = $provisional
        }
        $totalWER += $wer
        $totalAsrMs += $asrMs
        $totalProcessTime += $procTime
        $totalChunks += $chunks
        $totalFinal += $final
        $totalHardCut += $hardCut
        $totalProvisional += $provisional
        if ($chunks -gt 1) { $multiChunkFiles++ }
        $tested++
    } else {
        Write-Host "  (no reference)"
    }
}

Write-Host "`n=== E2E Batch Summary ==="
Write-Host "Files tested: $tested / $totalCount"
if ($tested -gt 0) {
    Write-Host "Avg WER: $([math]::Round($totalWER / $tested, 1))%"
    Write-Host "Avg ASR: $([math]::Round($totalAsrMs / $tested, 0))ms"
    Write-Host "Avg Processing: $([math]::Round($totalProcessTime / $tested, 2))s"
    Write-Host "Total ASR time: $([math]::Round($totalAsrMs / 1000, 1))s"

    Write-Host "`n=== Segmentation Quality ($tested files) ==="
    Write-Host "Total chunks: $totalChunks  (Final: $totalFinal | HardCut: $totalHardCut | Provisional: $totalProvisional)"
    Write-Host "Files with >1 chunk: $multiChunkFiles ($([math]::Round($multiChunkFiles / $tested * 100, 0))%)"
}

$results | Export-Csv "e2e_batch_results.csv" -NoTypeInformation
Write-Host "`nResults saved to e2e_batch_results.csv"
```

- [ ] **Step 2: Run a small batch test**

Run: `.\scripts\run_e2e_batch.ps1 -MaxFiles 5`
Expected: Script runs 5 LJSpeech files, shows WER + segmentation summary, saves CSV

- [ ] **Step 3: Verify CSV output**

Run: `Import-Csv "e2e_batch_results.csv" | Format-Table`
Expected: Table with columns File, WER, ASR_Time_ms, Process_Time_s, Audio_Time_s, Chunks, Final, HardCut, Provisional

- [ ] **Step 4: Commit**

```bash
git add scripts/run_e2e_batch.ps1
git commit -m "feat: add run_e2e_batch.ps1 for E2E pipeline accuracy + segmentation metrics"
```
