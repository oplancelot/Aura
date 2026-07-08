# E2E Transcribe: VAD+ASR Pipeline Accuracy Test

## Purpose

Measure the end-to-end accuracy of Aura's real-time VAD+ASR pipeline against LJSpeech, enabling data-driven decisions about whether accuracy regressions are acceptable when pursuing latency optimizations.

**Priority:** Accuracy first, latency second.

## Architecture

```
WAV file → PCM(16kHz mono) → SileroVad (512-sample frames, 32ms)
                                    ↓ probability + is_speech
                              ChunkingStateMachine (same config as Aura)
                                    ↓ Final / HardCut chunks
                              SenseVoiceEngine.transcribe()
                                    ↓ text
                              Collect → join → WER vs reference
```

Exactly mirrors the real-time pipeline in `core/src/ffi/pipeline.rs`:
- `SileroVad::AUDIO_SAMPLES` (512) per frame
- Same `ChunkingConfig`: silence_close=200ms, provisional_start=1000ms, hard_cut=5000ms
- State machine emits `Provisional` (skipped), `Final` (silence boundary), `HardCut` (5s forced split)

## Output

### Terminal

Emulates `transcribe_wav` output format for parsability:

```
--- Utterance breakdown ---
#1  Provisional    0.6s chunk      0ms ASR   [preview] "how are y..."
#2  Final          1.2s chunk    230ms ASR   "how are you"
#3  HardCut        3.4s chunk    345ms ASR   "this is a longer sentence"
#4  Final          0.8s chunk    145ms ASR   "I'm fine"

--- Summary ---
Audio: 6.0s | Processing: 0.9s | ASR: 720ms total | RTF: 0.15x

=== E2E Transcript ===
how are you this is a longer sentence I'm fine

=== Reference ===
how are you this is a longer sentence I am fine
WER: 8.3%

=== Segmentation Quality ===
Total chunks: 4  (Final: 2, HardCut: 1, Provisional: 1)
Avg chunk duration: 1.5s  |  Min: 0.6s  |  Max: 3.4s
```

- `Provisional` chunks logged with `[preview]` prefix, 0ms ASR (no inference)
- Only `Final` + `HardCut` + flush contribute to E2E text and WER
- `Provisional` counted in segmentation stats only

### Files

- `e2e_transcribe_wav` writes no files (the batch script aggregates to CSV)

## Implementation Plan

### 1. `core/examples/e2e_transcribe_wav.rs`

- Reuse `transcribe_wav.rs`'s WAV loading, resampling, `extract_reference()`, `simple_wer()`
- Replace manual 30s chunk loop with VAD pipeline:
  - `SileroVad::new(vad_model_path)` — same VAD model as pipeline
  - `ChunkingStateMachine::new(ChunkingConfig::default())`
  - Feed 512-sample frames from PCM buffer
  - On `Some(AudioChunk)`: log every chunk (including Provisional) to utterance breakdown
  - On `Final` or `HardCut`: call `sv.transcribe(chunk.samples)`, append text to `e2e_text`
  - On `Provisional`: log with `[preview]` tag, skip ASR (no inference)
- If no `Final`/`HardCut` emitted at end, flush remaining samples as a single ASR call
- Log per-utterance: `#{n}  {type:12}  {duration:.1s}  {asr_ms}ms ASR  "{text}"`
  - Provisional shown as `0ms ASR` with `[preview]` prefix in text
- Segmentation stats: total chunks, count by type (Final/HardCut/Provisional), avg/min/max chunk duration
- Final summary matches `transcribe_wav` format for script parsing: `WER: X.X%`, `ASR: XXXXms`, `Processing: X.Xs`

**Model path resolution:**
- VAD model: `CARGO_MANIFEST_DIR/../assets/silero_vad.onnx`
- ASR model: `CARGO_MANIFEST_DIR/../models/sense-voice-small-q4_k.gguf`

### 2. `scripts/run_e2e_batch.ps1`

- Mirror of `run_asr_batch.ps1` structure
- Build `e2e_transcribe_wav` example (`cargo build --release --example e2e_transcribe_wav`)
- Iterate WAV files from `OpenSLR/LJSpeech/wavs/`
- Parse `WER: X.X%`, `ASR: XXXXms`, `Processing: X.Xs`, and segmentation stats from stdout
- Output `e2e_batch_results.csv` with columns:
  ```
  File | WER | ASR_Time_ms | Process_Time_s | Audio_Time_s | Chunks | Final | HardCut | Provisional | Min_Chunk_s | Avg_Chunk_s | Max_Chunk_s
  ```
- Terminal summary includes both accuracy and segmentation quality:
  ```
  === E2E Batch Summary ===
  Files tested: 50 / 50
  Avg WER: 5.2%
  Avg ASR: 312ms
  Avg Processing: 0.85s
  Total ASR time: 15.6s

  === Segmentation Quality (50 files) ===
  Total chunks: 112       Final: 78 | HardCut: 12 | Provisional: 22
  Files with >1 chunk: 14 (28%)
  Avg chunk duration: 2.8s  |  Min: 0.3s  |  Max: 5.0s
  ```

## Scope

### In scope
- New example: `e2e_transcribe_wav.rs`
- New batch script: `run_e2e_batch.ps1`

### Out of scope
- Changes to existing `transcribe_wav.rs`, `run_asr_batch.ps1`
- Changes to pipeline Rust code, C++ FFI, or ChunkingConfig
- Aura.exe WAV file input support
- Real-time streaming from microphone for accuracy testing
- Automated regression alerts

## Success Criteria

1. `e2e_transcribe_wav` processes any WAV file and outputs WER against reference + segmentation quality
2. `run_e2e_batch.ps1` runs on full LJSpeech (13100 files) or subset (`-MaxFiles N`)
3. `e2e_batch_results.csv` includes accuracy columns (WER, ASR_Time, Process_Time) and segmentation columns (chunks by type, duration stats)
4. Terminal summary shows average WER, timing, and segmentation quality metrics
5. Files with >1 chunk are identified as VAD over-segmentation indicators
