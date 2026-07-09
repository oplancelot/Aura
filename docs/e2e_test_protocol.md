# E2E VAD+ASR 测试协议

> 版本 1.0 · 2026-07-09

## 1. 测试目的

量化 E2E 管线（Silero VAD → ChunkingStateMachine → SenseVoice ASR）在准确率、延迟、显示效果三个维度的表现，驱动优化决策。

## 2. 固定配置

### 2.1 模型

| 组件 | 文件 | 版本 |
|------|------|------|
| VAD | `assets/silero_vad.onnx` | v4 (512帧/帧) |
| ASR | `assets/sense-voice-small-q4_k.gguf` | SenseVoice-Small Q4_K |

### 2.2 ChunkingConfig 默认值

| 参数 | 值 | 说明 |
|------|----|------|
| `silence_close_ms` | 200 | 静音多久确认句尾 |
| `provisional_start_ms` | 1000 | 连续语音多久出预览 |
| `provisional_interval_ms` | 200 | 预览刷新间隔 |
| `hard_cut_ms` | 5000 | 最长语音段 |
| `hard_cut_overlap_ms` | 2000 | 硬切重叠量 |

### 2.3 数据集

默认使用 **LJSpeech**（13,100 条，单人朗读，16-22kHz）。子集按文件名排序取前 N 条。

### 2.4 测试模式

| 模式 | 标志 | 说明 |
|------|------|------|
| **Accuracy** | (默认) | 无帧间 sleep，VAD/ASR 全速跑。测量 WER、ASR 延迟上限 |
| **Latency** | `--realtime` | 每 VAD 帧 sleep ~16ms，模拟实时捕获时钟。Provisional/HardCut 正常触发 |
| **DisplayEval** | `--display-eval` | 在 Latency 基础上对 Provisional 也跑 ASR，评测预览质量。侵入式，WER 不具参考性 |

## 3. 测试脚本

| 脚本 | 功能 |
|------|------|
| `scripts/run_e2e_batch.ps1` | 主批量测试：编译→遍历 WAV→解析输出→CSV+JSON |
| `scripts/run_asr_batch.ps1` | Offline ASR 基线（30s 定长切句，无 VAD） |
| `scripts/compare_e2e_baseline.ps1` | ASR vs E2E 对比，计算 ΔWER 门禁 |
| `scripts/run_e2e_sweep.ps1` | ChunkingConfig 参数扫描 |
| `scripts/run_thread_sweep.ps1` | ASR 线程数扫描 |

### 3.1 参数说明

```powershell
.\scripts\run_e2e_batch.ps1 `
  -MaxFiles 100 `          # 测试文件数 (0=全部)
  -Suite Accuracy|Latency ` # 测试模式
  -DisplayEval `            # 预览质量评测
  -SilenceClose 200 `       # 覆盖 silence_close_ms
  -HardCut 5000 `           # 覆盖 hard_cut_ms
  -Threads 4                # 覆盖 ASR 线程数
```

### 3.2 输出文件

```
e2e_batch_results_{mode}_{timestamp}.csv   # 逐文件明细
e2e_batch_summary_{mode}_{timestamp}.json  # 汇总+元信息
```

## 4. 指标体系

### 4.1 准确率 (Accuracy)

| 指标 | 定义 | 来源 |
|------|------|------|
| WER | 词错误率 (Levenshtein 距离) | `simple_wer()` |
| WER p50/p90/p95 | 分位数 | `Get-Percentile()` |
| WER 分布 | 0% / <5% / ≥20% 文件占比 | 汇总统计 |
| ΔWER | WER_e2e − WER_offline | `compare_e2e_baseline.ps1` |

### 4.2 延迟 (Latency)

| 指标 | 定义 | 来源 |
|------|------|------|
| ASR time | 每文件 ASR 推理总耗时 (ms) | `total_asr_ms` |
| Processing time | 文件处理总耗时 (含 VAD+ASR) | 墙钟 |
| RTF | Processing / Audio 时长 | 汇总 |
| Endpoint latency | `silence帧数 × 帧时长 + ASR耗时` (Final chunk) | `Chunk.end_silence_frames` |
| TTFP | 语音 onset → 首次 Provisional (ms) | `Chunk.speech_start_offset` |

### 4.3 分段质量 (Segmentation)

| 指标 | 定义 |
|------|------|
| Chunks | 总 chunk 数 (Final/HardCut/Provisional) |
| Multi-chunk | 多段文件占比 |
| Chunk duration | 每段时长均值/最小/最大 |
| Flush 依赖率 | 依赖 flush 才出 final 的文件占比 |
| ASR error | transcribe 失败的 chunk 数 |

### 4.4 显示质量 (DisplayEval 模式)

| 指标 | 定义 |
|------|------|
| Prefix match | Provisional 文本词级前缀匹配 Final 的比例 |
| Text stability | 连续 Provisional 文本单调性 (无词回退) 占比 |
| TTFP | 同延迟指标的 TTFP |

## 5. 门禁 (Gates)

| 门禁 | 定义 | 阈值 |
|------|------|------|
| L0 | 无文件 ΔWER > +10pp | PASS if 0 files |
| L1 | < 20% 文件 ΔWER > +5pp | PASS if < 20% |

## 6. 操作流程

### 6.1 日常回归

```powershell
# 1. 打 Accuracy 基线 (10 条)
.\scripts\run_e2e_batch.ps1 -MaxFiles 10

# 2. 与 offline ASR 对比
.\scripts\run_asr_batch.ps1 -MaxFiles 10
.\scripts\compare_e2e_baseline.ps1
```

### 6.2 参数调优

```powershell
# ChunkingConfig 扫参
.\scripts\run_e2e_sweep.ps1 -MaxFiles 10

# 线程扫参
.\scripts\run_thread_sweep.ps1 -MaxFiles 10
```

### 6.3 预览质量评测

```powershell
.\scripts\run_e2e_batch.ps1 -Suite Latency -DisplayEval -MaxFiles 5
```

## 7. 已知限制

- Accuracy 模式下 Provisional/HardCut 不触发（墙钟远快于音频时钟），相关指标仅在 Latency 模式有效
- DisplayEval 模式添加侵入式 ASR 开销，WER 不具参考性
- hard_cut 在 LJSpeech 上几乎不触发（短句为主），扫参时 Latency 模式更有效
