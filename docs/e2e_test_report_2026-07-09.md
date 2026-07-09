# E2E 管线测试报告

> 日期: 2026-07-09
> 基线提交: `9a4c9f1` (所有里程碑 M1-M6)
> 机器: WIN-BM8FAG3M8GS

## 测试配置

| 项 | 值 |
|---|---|
| VAD 模型 | `assets/silero_vad.onnx` (512帧, 阈值 ON=0.10/OFF=0.05) |
| ASR 模型 | `assets/sense-voice-small-q4_k.gguf` (4线程) |
| ChunkingConfig | `silence_close=200ms, provisional_start=1000ms, provisional_interval=200ms, hard_cut=5000ms, hard_cut_overlap=2000ms` |
| 数据集 | LJSpeech 前 10 条 (文件名排序) |

## 1. 准确率 (Accuracy 模式)

| 指标 | 值 |
|------|-----|
| **平均 WER** | **5.5%** |
| p50 | 6.2% |
| p90 | 11.2% |
| p95 | 11.5% |
| WER=0 | 30% (3文件) |
| WER < 5% | 40% (4文件) |
| WER ≥ 20% | 0% (0文件) |

### ΔWER vs Offline ASR (M2, 50文件基线)

| 指标 | 值 |
|------|-----|
| **平均 ΔWER** | **+0.4pp** |
| p50 | 0pp |
| p90 | 0.4pp |
| Δ > +5pp | 8% (4文件) |
| Δ > +10pp | **0% (0文件)** |

> ✅ L0 门禁: 无文件 ΔWER > +10pp — PASS
> ✅ L1 门禁: < 20% 文件 Δ > +5pp — PASS (8%)

**结论**: VAD 切句对识别准确率几乎无影响。

## 2. 延迟 (Accuracy 模式)

| 指标 | 值 |
|------|-----|
| **平均 ASR 耗时** | **810ms** |
| ASR p50 | 905ms |
| ASR p90 | 1088ms |
| **平均 Processing** | **0.84s** |
| **RTF** | **0.12x** |
| **Endpoint (Final p50)** | **859ms avg** (p50=834ms, p90=1226ms) |

Endpoint latency 构成: `silence_close(200ms)` + `ASR(avg~650ms)` ≈ 850ms。

## 3. 分段质量

| 指标 | 值 |
|------|-----|
| 总 chunks | 15 (Final=15, HardCut=0, Provisional=0) |
| 多段文件占比 | 30% |
| Flush 依赖率 | 100% |
| ASR 错误 | 0 |
| Avg chunk 时长 | 5.3s (min=4.9s, max=5.5s) |
| Global min/max | 0.7s / 9.9s |

> 注: LJSpeech 多为 2-10s 朗读句，VAD 静音检测 + flush 完成切句。无 HardCut/Provisional (Accuracy 模式墙钟过快)。

## 4. 显示质量 (Latency + DisplayEval 模式)

| 指标 | 值 |
|------|-----|
| **Prefix match avg** | **98%** |
| Prefix match p50/p90 | 98% / 98% |
| **Text stability avg** | **93%** |
| **TTFP avg** | **1010ms** (匹配 `provisional_start_ms=1000`) |
| **Endpoint (Final p50)** | **45ms** |
| Provisional ASR chunks | 50 (5文件) |

> ⚠ DisplayEval 是侵入式评测: 对每个 Provisional (~2s) 额外跑 ASR (~250ms)，墙钟加速导致 HardCut 激增 (621/5文件)。只看显示指标，WER 不具参考性。

## 5. 参数扫参结论

### ChunkingConfig (M4, 5文件/组合)

| Config | Avg WER | Endpoint | Multi-chunk |
|--------|---------|----------|-------------|
| `sc=100` | 3.9% | **737ms** | 40% |
| **`sc=200`** | **3.9%** | **949ms** | **20%** |
| `sc=400` | **3.2%** | 1255ms | 0% |

> 推荐: **保持 `silence_close=200ms`** (最佳 trade-off)

### ASR 线程数 (M5, 10文件/组合)

| Threads | Avg ASR | vs 默认 | Endpoint | WER |
|---------|---------|---------|----------|-----|
| 1 | 2425ms | 3.0x 慢 | 2125ms | 5.5% |
| 2 | 1330ms | 1.66x 慢 | 1262ms | 5.5% |
| **4** | **800ms** | **1.00x** | **850ms** | **5.5%** |
| 8 | 675ms | 1.19x 快 | 749ms | 5.5% |

> 推荐: **保持 4 线程** (边际收益递减, t8 仅 +19%)

## 6. 对比总结

```
                        Accuracy              Latency
                   ──────────────────   ──────────────────
WER  (avg)         ████░░ 5.5%          ████████████░ 87.7%*
ASR  (avg)         ████░░ 810ms         ████████████░ 8996ms*
Endpoint (p50)     ████░░ 834ms         ██░░░░░░░░░░░  45ms
TTFP (avg)          N/A                 ████░░ 1010ms
Prefix match       N/A                 ████████████░ 98%
Multi-chunk        ██░░ 30%            █████████████ 100%

* DisplayEval 侵入式 ASR 导致; 仅显示指标有参考性
```

## 7. 后续建议

| 优先级 | 方向 | 说明 |
|--------|------|------|
| P0 | VAD 阈值调优 (M7) | 当前 100% flush 依赖率偏高，可能 silence_close 过长或 VAD 灵敏度不够 |
| P1 | 域内数据集 (M7) | LJSpeech 单人朗读 vs 游戏/Discord 多人、噪声场景差异大 |
| P2 | 翻译链路 E2E (M8) | ASR 文本 → 翻译 → 字幕，BLEU/COMET 评测 |
| P3 | 流式 ASR | 当前每 chunk 独立 ASR；流式推理可降低 TTFP 和 endpoint |
