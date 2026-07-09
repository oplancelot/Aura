# Aura VAD 系统优化分析

基于 `docs/vad_tuning.md` 及 `core/src/vad/`、`core/src/ffi/pipeline.rs`、`core/src/audio/` 全部代码的综合分析。

---

## 一、架构层优化

### 1. 状态机缺少"能量感知"的预判

当前状态机纯粹依赖 VAD 布尔结果 + 时间阈值，没有利用 `VadResult.probability` 的连续值。可以在接近阈值时做 **加权滑动窗口投票**，而非单帧翻转就触发状态变化。

### 2. 硬切分 overlap 逻辑过于简单

`check_hard_cut()` 只做尾部 2 秒拼接，没有考虑语义边界。如果硬切正好在词中间，overlap 拼接的上下文对 ASR 无意义。可引入 **基于能量的断句点检测** 在 overlap 区域找最佳拼接位置。

### 3. Provisional 发送无条件全量 buffer（注意 SenseVoice 的全量限制）

`emit_provisional()` 每次 clone 整个 buffer（`self.buffer.clone()`），随着说话时间增长，buffer 越来越大，每 200ms 都 clone 一份全量。**注意：** 因为我们目前使用的是 SenseVoice（非流式 ASR 模型），不能只发增量（否则会丢失上下文导致乱码）。应改为用 `Arc<[f32]>` 或零拷贝的切片视图（View）共享全量历史数据，在不破坏 ASR 上下文的前提下消除内存拷贝。

### 4. 线程模型：VAD 与 ASR 串行（pipeline 线程内）

采集已独立线程（`aura-capture`），但 VAD 推理和 ASR 推理在 `aura-pipeline` 线程内串行执行。VAD 推理期间 ASR 等待，ASR 推理期间 VAD 无法处理新帧，可能导致音频堆积。可 **流水线化**：VAD 持续实时推理，ASR 异步消费 chunk（通过 channel 传递 `AudioChunk`）。

### 5. pipeline 线程用 `sleep(10ms)` 轮询

`pipeline.rs:340` 每轮循环 `thread::sleep(10ms)`，这是 busy-wait 模式。应改为 **condvar/channel 通知**：当 ring buffer 有足够数据时唤醒 pipeline 线程，减少空闲时的 CPU 占用和延迟。

### 6. 捕获线程应设高优先级

WASAPI 捕获线程（`aura-capture`）未设置线程优先级。在系统负载高时可能被调度延迟，导致音频 glitch。应使用 `thread::Builder::new().spawn_unchecked` 或 Windows API 设置 `THREAD_PRIORITY_HIGHEST`。

### 7. 若实现 VAD→ASR 流水化需处理线程安全

如果实现 #4 的流水线方案（VAD 持续跑，ASR 异步消费），`SileroVad` 当前要求 `&mut self` 独占引用。需要将 VAD 交给独立线程，通过 channel 传递 `VadResult` 和音频切片给状态机和 ASR，或给 VAD 加锁。

---

## 二、函数/代码层优化

### 8. `process_frame()` 每帧分配 `model_input` Vec

`silero.rs:98` 每帧 `Vec::with_capacity(576)` + `extend_from_slice` 两次，30fps 下每秒 30 次堆分配。应 **预分配并复用** `model_input` buffer（类似 state/context 的做法）。

### 9. `process_frame()` 中 `self.state.clone()` 不必要

`silero.rs:105` 传入 `self.state.clone()` 给 ONNX，但 `state` 已经是 `Vec<f32>`，可以直接传引用或用 `Cow`，避免每帧 clone 2×1×128×4 = 1024 字节。

### 10. `emit_provisional()` 全量 clone buffer（实现方案）

承接 #3（架构层讨论），具体实现上：`state_machine.rs:197-198` `self.buffer.clone()` 当说话持续 5 秒时构造 320KB 新 Vec，每 200ms 一次。改为 `Arc<[f32]>` 共享 buffer，provisional 时 `Arc::clone`（引用计数 +1，无拷贝）。需要同时改造 `AudioChunk` 结构体以承载 `Arc<[f32]>`。

### 11. `frame_buffer.drain(..).collect()` 每帧创建新 Vec

`pipeline.rs:274-275` 和 `capture_to_vad.rs:81` 每帧 drain 到新 Vec。可改用 **固定大小数组 + 切片** 避免分配。

### 12. Ring buffer `pull()` 每次分配新 Vec

`ring_buffer.rs:82` 每次 `vec![0.0f32; count]`。应给 Ring Buffer 增加一个 `pull_into(&mut slice)` 的接口，让 consumer 传入预分配好的 buffer 被直接填充，彻底消除这段 Hot Path 上的堆分配。

### 13. 静音帧计数用 `u64 * 32` 硬编码帧长

`state_machine.rs:133` 硬编码 `32ms` 帧长。若将来改采样率或帧大小会出 bug。应从 config 或 `SileroVad::AUDIO_SAMPLES` 计算。

### 14. ONNX `Tensor::from_array(Vec)` 可能有隐式拷贝

`silero.rs:103-105` 传入 `Vec` 给 `Tensor::from_array`，ONNX Runtime 可能内部再拷贝一次。考虑用 `ndarray::Array1::from_vec` 预分配后直接传引用，或使用 `ort::value::Tensor::from_array` 的零拷贝路径。

### 15. `pipeline.rs:269` `pull(available)` 一次拉取全部的策略正确

`consumer.pull(available)` 拉取所有可用样本，然后循环 drain 512 逐帧处理。这个策略是正确的："追赶模式"——当系统忙时一次取尽可能多的数据然后批量处理，反而能防止堆积落后。无需修改。但建议添加一个最大拉取量上限（如 `min(available, 16000)`），防止极偶然的 spike 导致单次分配过大。

### 16. `capture_to_vad.rs` 帧长用错导致运行时 panic（bug）

`capture_to_vad.rs:80-81` 用 `SileroVad::FRAME_SAMPLES`（576 = 512 + 64 context）作为 drain 长度，然后将 576 个样本传入 `process_frame()`。但该函数 `assert_eq!(frame.len(), 512)`（`silero.rs:89`），运行时必然 panic。应改为 `SileroVad::AUDIO_SAMPLES`（512），context 由 SileroVad 内部维护。

---

## 三、算法/参数优化

### 17. 缺少 VAD 后处理平滑（smoothing）

当前 VAD 输出直接喂给状态机，无任何滤波。可以加 **中值滤波器** 或 **形态学开运算**（连续 N 帧才确认状态变化），消除单帧毛刺。

### 18. THRESHOLD_ON/OFF 是编译时常量

`silero.rs:50-52` 硬编码为 `const`，无法运行时调整。应改为 `ChunkingConfig` 的字段，支持按环境动态调参（如噪音环境自动调高阈值）。

### 19. 缺少自适应阈值

当前阈值对所有场景一视同仁。可引入 **自适应阈值**：根据近期 probability 的均值/方差动态调整 `THRESHOLD_ON`/`THRESHOLD_OFF`，适应不同说话人音量和环境。

### 20. [已修复] silence_close_ms 文档与代码默认值曾不一致

文档最初写 `400ms`，代码实际为 `200ms`；`provisional_start_ms` 文档 `2000ms`，代码 `1000ms`。经激进调优后已统一。**目前 `vad_tuning.md` 已与代码对齐。** 此项仅留作历史记录。

### 21. 缺少"提前预判"机制

可以在概率接近阈值时提前预警，而不是等到概率越过阈值才切换状态。例如：probability > 0.08 时开始计时，> 0.10 确认，减少延迟。

### 22. 缺少 energy floor 检测

极低能量帧（RMS < 阈值）可直接判定为静音，跳过神经网络推理。Silero VAD 对纯静音帧的推理是浪费算力，且概率输出不一定为 0。可在 `process_frame` 前加一个快速 energy gate。

### 23. 缺少"尾音截断"优化

静音后 VAD 模型仍会输出几帧高于阈值的 speech 概率（RNN 记忆衰减延迟），导致每句话尾部多出 60-100ms 冗余静音。可在确认 silence 后丢弃最后 N 帧，或在 ASR 前做尾部静音裁剪。

### 24. 缺少"语音起始预判"

当前在概率越过 `THRESHOLD_ON` 后才开始累积 buffer，浪费了起始前几帧的音频。可检测概率快速上升斜率（如连续 3 帧概率递增），提前开始累积，降低首字延迟。

### 25. `reset_state()` 清空 RNN 记忆导致下句首帧检测延迟

`pipeline.rs:303` 在 Final/HardCut 时调用 `vad.reset_state()`，该函数清空了 RNN state 和 context。这导致下一句开头的几帧 inference 没有历史上下文，speech 概率可能偏低，检测延迟增加。可以考虑只重置 `is_speaking` 和 `context`，保留 RNN state 的部分记忆；或者在前几句 warm-up 后再启动 speech 检测。

---

## 四、性能优化

### 26. ONNX 推理优化级只用 Level1（与 #30 联动）

`silero.rs:63` `GraphOptimizationLevel::Level1`。注意 Silero VAD 模型极小（~2MB），Level3 的全量图优化可能增加加载时间而推理速度无明显提升（见 #30）。建议先实测对比 Level1/Level2/Level3，选性价比最高的级别。

### 27. 重采样器可预分配输出 buffer

`capture.rs:297` `resampler.process(&mono_samples)` 每次分配新 Vec。rubato 的 API 可以用 `process_into` 预分配输出。

### 28. 缺少 SIMD 优化的 mono mix-down

`capture.rs:293` `(left + right) * 0.5` 逐样本处理。对大批量音频可用 SIMD 加速 stereo→mono 转换。

### 29. 状态机 buffer 预分配过大

`state_machine.rs:102` `Vec::with_capacity(16000 * 30)` = 30 秒预分配。实际大多数句子 < 5 秒，浪费内存。可根据 `hard_cut_ms` 动态计算。

### 30. Silero VAD 模型极小，Level3 优化可能适得其反（与 #26 联动）

`silero.rs:63` 当前使用 `GraphOptimizationLevel::Level1`。但 Silero VAD 模型非常小（~2MB），Level3 的全量图优化可能增加加载时间而推理速度无明显提升。与 #26 联动：建议实测 Level1/Level2/Level3 三档的（加载耗时，平均推理耗时），选最优解。

### 31. 缺少 VAD 模型 warm-up

首次推理总是最慢（ONNX graph optimization + JIT 编译）。应在启动时用几帧静音数据预热模型，避免首句识别延迟。

---

## 五、健壮性/可观测性优化

### 32. 缺少 VAD 置信度统计指标

没有暴露 VAD 的平均 probability、speech/silence 比例等统计量。可加入 `TranslationMetrics` 供调试。

### 33. 缺少丢帧/溢出恢复机制

Ring buffer overflow 时只是 `log::warn` 一次就静默。应增加 **overflow 计数器** 并在 UI 显示，且考虑在 overflow 后重置 VAD 状态。

### 34. 缺少 energy-based voice activity 辅助判断

纯神经网络 VAD 在极低信噪比下可能失效。可并行计算 **短时能量（RMS）** 作为辅助特征，与 Silero 概率做融合决策。

### 35. 缺少采样率/帧长自动校验

`process_frame` 用 `assert_eq!` 校验帧长，生产环境会 panic。应改为返回 `Result`。

### 36. VadLogger 每帧 I/O

`pipeline.rs:281` 每帧写 CSV。高频写入影响性能。应改为 **批量写入** 或 **环形内存 buffer + 定期刷盘**。

### 37. VadLogger/CaptureDumper 生产环境也分配

`pipeline.rs:264-265` 每次启动 pipeline 都创建 `VadLogger` 和 `CaptureDumper`，即使在生产环境。应 behind `#[cfg(debug_assertions)]` 或 feature flag，避免生产环境的无谓开销。

### 38. pipeline VAD 推理失败直接退出，缺少降级逻辑

`pipeline.rs:277` `vad.process_frame(&frame)?` 失败时直接 `?` 传播错误，pipeline 线程退出。应增加重试机制或降级为"跳过当前帧继续运行"，避免单帧异常导致整个翻译中断。

### 39. ring buffer overflow 发生在语音段中时 VAD 状态不一致

当 ring buffer 溢出丢弃样本时，如果正好在语音段中间，VAD 已经进入 `SpeechActive` 状态但后续样本缺失，导致状态机判断失准。应在 overflow 后调用 `vad.reset_state()` 并将状态机重置为 `Silence`。

### 40. 长时间采集的音频时钟漂移

WASAPI 捕获时钟（48kHz 设备时钟）与 pipeline 的 16kHz 处理时钟可能因晶振偏差导致长期趋势性不同步。运行 30 分钟以上可能出现 buffer 持续增长（漂入）或持续缩短（漂出）。建议监控 pipeline cycle 的帧处理耗时，或在 ring buffer consumer 侧检测单向趋势性水位变化并选择性丢帧/插值。

### 41. FFI 回调的线程亲和性问题

`emit_translation()` 在 pipeline 线程中调用 C# callback，但 C# UI 通常期望在主线程更新。如果直接在回调中操作 UI 控件，可能抛出跨线程异常或导致隐性问题。建议在 callback 实现侧做 `Control.Invoke` 或 `SynchronizationContext.Post`，或在 Rust 侧提供 `dispatch_to_main_thread` 的接口约定。

---

## 六、功能增强

### 42. 检测可能的 speaker change 区间

纯 speaker diarization 需要独立的声纹聚类模型，超出当前 VAD 范围。但可以在状态机中增加一个弱提示：检测到长时间静音后恢复说话时，标记一个可能的 speaker change 点，供上层参考（不是精确分割，而是概率提示）。

### 43. 支持动态配置参数

`ChunkingConfig` 目前只用 `Default`，没有暴露给 FFI。应增加 `aura_core_set_vad_params()` 让 C# 侧运行时调参。

### 44. 缺少 VAD 结果的回调/事件机制

VAD 的 `is_speech` 状态变化没有通知上层。可增加 event callback 让 UI 显示实时 VAD 状态（说话中/静音）。

---

## 最高优先级建议（立即可做）

| 优先级 | 优化项 | 收益 |
|:---:|:---|:---|
| 1 | 修复 `emit_provisional()` 全量 clone → `Arc<[f32]>`（#3 / #10） | 降低长句内存压力，消除每 200ms 的 320KB 拷贝 |
| 2 | 预分配复用 `model_input` Vec（#8） | 消除每帧堆分配，30fps 下每秒 30 次 malloc |
| 3 | 加 VAD 后处理平滑（#17） | 减少单帧毛刺导致的误切分 |
| 4 | pipeline 轮询改 condvar/channel（#5） | 降低空闲 CPU 占用，减少延迟 |
| 5 | 添加 VAD 模型 warm-up（#31） | 消除首句识别延迟 |
| 6 | pipeline 推理失败加降级逻辑（#38） | 避免单帧异常中断整个翻译 |
| 7 | 修复 capture_to_vad.rs frame panic bug（#16） | 示例代码可用 |
