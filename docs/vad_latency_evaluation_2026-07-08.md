# VAD 延迟实测评估

**测试日期:** 2026-07-08
**测试场景:** VLC 视频播放 → WASAPI loopback 采集 → Silero VAD → SenseVoice ASR → C# UI 渲染
**数据来源:** `ui/Aura/publish/logs/timing_20260708_001450.csv`, `vad_1783469933.csv`, `asr_20260708_001450.txt`

---

## 总体架构延迟

| 阶段 | 延迟 | 说明 |
|:---|:---:|:---|
| VAD 人声检测 | **< 1 帧 (~32ms)** | VAD CSV 显示 0.6ms 概率从 0.02→0.83 |
| 首次 Provisional 出现 | **~1.26s** | 语音起始到 UI 显示第一个 ~ 字幕 |
| Provisional 间隔 | **~200ms** | 符合 `provisional_interval_ms` 配置 |
| ASR 推理耗时 | **227~1,055ms** | 随音频长度线性增长 (~100-150ms/s) |
| Pipeline 开销 (不含 ASR) | **17~41ms** | Rust 处理 + FFI 回调 + UI 渲染 |
| **E2E Final 渲染** | **244~1,096ms** | 其中 ASR 占主要部分 |

## 逐句延迟分解

| 句号 | 音频长度 | ASR 耗时 | Pipeline 开销 | E2E 渲染 | 文字 |
|:---:|:---:|:---:|:---:|:---:|:---|
| 1 | 1.76s | 227ms | 17ms | 244ms | *at present concerned.* |
| 2 | 5.18s | 590ms | 39ms | 629ms | *most, if not from all the arts and crafts represented in the exhibition.* |
| 3 | 6.99s | 1,055ms | 41ms | 1,096ms | *represented in the exhibition in being comparatively modern for although the Chinese took impressions from wood.* |

## Provisional 更新序列（句 2 示例）

每 ~200ms 吐出一次中间结果，audio_ms 递增稳定：

| Seq | 时间 (s) | audio_ms | 类型 |
|:---:|:---:|:---:|:---:|
| 4 | 245.758 | 1,216 | P |
| 5 | 245.962 | 1,408 | P |
| 6 | 246.164 | 1,600 | P |
| ... | ... | ... | P |
| 22 | 249.702 | 5,152 | P |
| 23 | 250.345 | 5,184 | F |

## VAD 性能

- **人声检测:** 首帧概率 0.02（静音）→ 次帧 0.83（人声），响应 < 1 帧
- **语音脱落:** 概率从 0.75→0.09→0.06 约 3 帧完成 transition
- **3 帧平滑效果:** 概率曲线无明显毛刺，smoothing 未引入可感知额外延迟

## 评估结论

1. **VAD 响应极快**（< 1 帧），3 帧平滑未引入可感知延迟
2. **Pipeline 额外开销极小**（17-41ms），瓶颈在 SenseVoice ASR 推理
3. **长句 E2E 偏长**（第三句 ~1.1s），音频 7s → ASR 推理 1.05s
4. **切句点略有不均**（5.18s 处切断导致句子碎片化），可微调 silence_close_ms 或硬切 overlap 策略

## 建议方向

1. **减小 `hard_cut_ms`**（当前 5s → 对话场景 3s 足够），强制更早切分以减少单句 ASR 时间
2. **升级 ASR 推理优化**：量化 / 更小模型 / better GPU utilization
3. **引入流式 ASR**（取代当前非流式 SenseVoice），从根本上消除长音频推理延迟
4. **非关键路径优化**（当前收益有限，Pipeline 仅占 17-41ms）
