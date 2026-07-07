# Latency Analysis — 2026-07-07

## Method

In-band telemetry: `TranslationMetrics` passed through FFI callback with Rust-side
`Instant` deltas; C# records render timestamp via `Channel<string>` and writes
unified `logs/timing_*.csv` asynchronously.

## Pipeline Stages

```
WASAPI capture → ring buffer → VAD(512帧) → state machine → ASR(SenseVoice) → FFI callback → C# dispatch → render(33ms timer)
                       [T1/T2]      [T3]        [T4]            [T5]               [T6]           [T7]          [T8]
```

Measured: T4 (chunk ready) → T5 (ASR inference) → T6 (callback) → T7 (C# received) → T8 (rendered)

## Results

### Final chunks (ASR applied) — 7 samples

| Metric | Min | Max | Avg |
|---|---|---|---|
| ASR inference (T5) | 688ms | 812ms | **769ms** |
| E2E render (T4→T8) | 730ms | 845ms | **801ms** |
| C# display delay (T7→T8) | 2.9ms | 45.7ms | 32.8ms |

### Provisional chunks (typing preview) — 101 samples

| Metric | Min | Max | Avg |
|---|---|---|---|
| C# display delay (T7→T8) | 0.2ms | 60.5ms | 23.6ms |

## Analysis

- **ASR (T5) is the dominant bottleneck**, accounting for ~96% of end-to-end latency
- Rust callback dispatch (T4→T5 + T6 overhead) is negligible
- C# rendering delay is bounded by the 33ms `DispatcherTimer` interval
- Provisional (preview) chunks skip ASR entirely, so their latency is just the render timer

## Optimization Priorities

1. Reduce ASR inference time (current ~770ms)
   - Try smaller quantized model
   - Increase thread count in `sense_voice_capi.cc` (currently 4)
   - Overlap ASR with next audio capture chunk
2. Reduce render timer interval (currently 33ms) if sub-16ms display matters
3. No benefit from optimizing Rust/C# dispatch overhead (~1-2ms)
