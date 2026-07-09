# VAD 优化执行计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 VAD 模块中已验证的 bug 和低风险性能问题，逐项编译测试、原子提交

**Architecture:** 所有修改局限在 `core/src/vad/`、`core/src/audio/ring_buffer.rs`、`core/examples/capture_to_vad.rs`，不涉及 FFI 或架构变更

**Tech Stack:** Rust 2021, ONNX Runtime (ort 2.0.0-rc.12), ringbuf 0.4, rubato 0.16

## 全局约束

- 每项修改必须通过 `cargo build` 和 `cargo test`
- 每项修改独立原子提交，commit message 格式 `fix:` 或 `perf:`
- 不引入新的依赖
- 不改动 FFI 接口、Cargo.toml 依赖版本

---

### Task 1: 修复 capture_to_vad.rs 帧长 bug

**Files:**
- Modify: `core/examples/capture_to_vad.rs:80-81`

**Bug:** 第 81 行 `chunk_buffer.drain(..SileroVad::FRAME_SAMPLES).collect()` 取得 576 个样本传入 `process_frame()`，但该函数 `assert_eq!(frame.len(), 512)`，运行时必 panic。

- [ ] **Step 1: 修改 drain 长度为 AUDIO_SAMPLES**

```rust
// Before (~line 80):
while chunk_buffer.len() >= SileroVad::FRAME_SAMPLES {
    let frame: Vec<f32> = chunk_buffer.drain(..SileroVad::FRAME_SAMPLES).collect();

// After:
while chunk_buffer.len() >= SileroVad::FRAME_SAMPLES {
    let frame: Vec<f32> = chunk_buffer.drain(..SileroVad::AUDIO_SAMPLES).collect();
```

- [ ] **Step 2: 编译验证**

```bash
cd core && cargo build --example capture_to_vad 2>&1
```

预期：编译通过，无 warning

- [ ] **Step 3: 确认 VAD 测试通过**

```bash
cd core && cargo test --lib vad::silero::tests 2>&1
```

预期：`test vad::silero::tests::test_silero_vad_init_and_zeros ... ok`

- [ ] **Step 4: 提交**

```bash
git add core/examples/capture_to_vad.rs && git commit -m "fix: correct frame length in capture_to_vad example (AUDIO_SAMPLES=512, not FRAME_SAMPLES=576)"
```

---

### Task 2: 修复静音帧计数的硬编码帧长

**Files:**
- Modify: `core/src/vad/state_machine.rs:133`

**问题:** `self.silence_frame_count as u64 * 32` 硬编码 32ms，若将来改采样率或 AUDIO_SAMPLES 会出错。应从 VAD 常量计算帧时长：`AUDIO_SAMPLES * 1000 / SAMPLE_RATE`

- [ ] **Step 1: 导入 SileroVad 常量**

```rust
// 文件顶部已有 use super::silero::VadResult;
// 需要增加:
use super::silero::SileroVad;
```

- [ ] **Step 2: 替换硬编码计算**

```rust
// Before (~line 133):
let silence_ms = self.silence_frame_count as u64 * 32; // 32ms per frame

// After:
let frame_ms = SileroVad::AUDIO_SAMPLES as u64 * 1000 / SileroVad::SAMPLE_RATE as u64;
let silence_ms = self.silence_frame_count as u64 * frame_ms;
```

- [ ] **Step 3: 编译验证**

```bash
cd core && cargo build --lib 2>&1
```

预期：编译通过

- [ ] **Step 4: 确认现有测试通过**

```bash
cd core && cargo test --lib vad:: 2>&1
```

预期：`test result: ok`

- [ ] **Step 5: 提交**

```bash
git add core/src/vad/state_machine.rs && git commit -m "fix: derive frame duration from SileroVad constants instead of hardcoded 32ms"
```

---

### Task 3: 预分配复用 process_frame 的 model_input Vec

**Files:**
- Modify: `core/src/vad/silero.rs`

**问题:** `process_frame()` 每帧 `Vec::with_capacity(576)` + `extend_from_slice` 两次，每秒 30 次堆分配。应预分配一个 `model_input` 字段在 `SileroVad` 结构体中复用。

- [ ] **Step 1: 在结构体中增加 `model_input` 字段**

```rust
// Before:
pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    is_speaking: bool,
}

// After:
pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    is_speaking: bool,
    /// Pre-allocated input buffer for ONNX inference (576 samples).
    model_input: Vec<f32>,
}
```

- [ ] **Step 2: 在构造函数中初始化**

```rust
// 在 Ok(Self { ... }) 中增加一行:
model_input: vec![0.0f32; Self::FRAME_SAMPLES],
```

- [ ] **Step 3: 修改 process_frame 复用 buffer**

```rust
// Before (~line 98-99):
let mut model_input = Vec::with_capacity(Self::FRAME_SAMPLES);
model_input.extend_from_slice(&self.context);
model_input.extend_from_slice(frame);

// After:
self.model_input[..CONTEXT_SAMPLES].copy_from_slice(&self.context);
self.model_input[CONTEXT_SAMPLES..].copy_from_slice(frame);
```

并将第 103 行的 `Tensor::from_array((vec![1, Self::FRAME_SAMPLES], model_input))` 改为：
```rust
Tensor::from_array((vec![1, Self::FRAME_SAMPLES], self.model_input.clone()))
```

注意：这里仍然有 clone 是因为 `Tensor::from_array` 需要所有权。与 #9（state.clone）同理，但此修改已减少每帧的 Vec 分配 + extend_from_slice 开销。可结合 Task 4 进一步优化。

- [ ] **Step 4: 编译验证**

```bash
cd core && cargo build --lib 2>&1
```

预期：编译通过

- [ ] **Step 5: 确认测试通过**

```bash
cd core && cargo test --lib vad::silero::tests 2>&1
```

预期：`test result: ok`

- [ ] **Step 6: 提交**

```bash
git add core/src/vad/silero.rs && git commit -m "perf: pre-allocate model_input buffer in SileroVad to avoid per-frame allocation"
```

---

### Task 4: 消除 process_frame 中 state.clone 的不必要拷贝

**Files:**
- Modify: `core/src/vad/silero.rs`

**问题:** `process_frame()` 每帧 clone `self.state`（2×1×128×4 = 1024 bytes）传入 ONNX，可以使用 `ort::inputs!` 的引用语义避免这份拷贝。

- [ ] **Step 1: 修改 Tensor 创建方式**

注意：`ort::inputs!` 宏要求每个参数是 `impl Into<Tensor>`。`Tensor::from_array` 接受 `Into<Array>`，其中 `Array` 可以是 `ArrayD`。但 `ort` 的 `Tensor::from_array` 接受 `(Vec<usize>, Vec<T>)` 的 tuple，会消费 data。

查看 ort 2.0.0-rc.12 API：`Tensor::from_array` 可以从 `Array` from ndarray 创建，但不引入 ndarray 依赖。当前方式 `Tensor::from_array((vec![2, 1, 128], self.state.clone()))` 是唯一不需要额外依赖的方式。

改为使用 `&` 借用是否可行？对于 ort 2.0.0-rc.12，`inputs!` 宏可能支持引用。如果测试不支持，则跳过此项—收益仅 1024 字节/帧，在 30fps 下约 30KB/s，不是瓶颈。

检查 ort 文档：`Tensor::from_array` 需要 `ArrayBase` 或 `TensorElement`。最简单的零拷贝方式可能是：

```rust
use ort::value::Tensor;
// Tensor::from_contiguous_array(arr_d) or similar
```

但为了避免引入 ndarray，最佳方式是利用 `Tensor::from_array` 的 tuple 形式，但传递 `&[f32]` 切片。查看 ort API：

`Tensor::from_array` 签名：`fn from_array<A, S>(array: A) -> Result<Self>` 其中 `A: Into<Arc<S>>` 且 `S: OwnedTensorElementType`。

最简单的方案：如果 ort 不支持引用，就用一个预分配的 `state_buffer: Vec<f32>` 替代 `self.state.clone()`。

```rust
// Before:
"state" => Tensor::from_array((vec![2, 1, 128], self.state.clone())).unwrap(),

// After — 复用预分配 state 的引用，Tensor::view 或直接传 clone (确认是否必须)
```

经过检查，ort 2.0.0-rc.12 的 `Tensor::from_array` 实际上会拷贝数据（它要求 `Into<Arc<S>>`）。`clone()` 是必须的。收益很小，跳过此项不实施。

→ **跳过 Task 4**（收益 < 30KB/s，不值得为了消除 clone 而深入 ort 内部 API。Task 3 已经解决了主要的分配瓶颈。）

---

### Task 5: 添加 VAD 模型 warm-up

**Files:**
- Modify: `core/src/ffi/pipeline.rs` (~line 257 附近)

**问题:** 首次 `vad.process_frame()` 最慢（ONNX graph optimization lazy run），导致首个音频帧的延迟异常高。应在 pipeline 启动时用一帧静音预热。

- [ ] **Step 1: 在 VAD 初始化后添加 warm-up**

```rust
// 在 let mut vad = SileroVad::new(model_path)... 之后、主循环之前:
// Warm-up: 一帧静音消除 ONNX 首次推理延迟
let warmup_frame = vec![0.0f32; SileroVad::AUDIO_SAMPLES];
if let Err(e) = vad.process_frame(&warmup_frame) {
    log::warn!("VAD warm-up inference failed (non-fatal): {:#}", e);
}
// Warm-up 后恢复状态
vad.reset_state();
```

- [ ] **Step 2: 编译验证**

```bash
cd core && cargo build --lib 2>&1
```

预期：编译通过，可能有一个 `unused` warning（warmup_frame 在成功路径上不会被使用），如果出现则需要 `#[allow(unused_variables)]` 或使用 `let _ = ...`

- [ ] **Step 3: 确认测试通过**

```bash
cd core && cargo test --lib 2>&1
```

预期：全部测试通过

- [ ] **Step 4: 提交**

```bash
git add core/src/ffi/pipeline.rs && git commit -m "perf: add VAD model warm-up to eliminate first-inference latency spike"
```

---

### Task 6: 添加 VAD 后处理平滑（滑动窗口）

**Files:**
- Modify: `core/src/vad/silero.rs`

**问题:** VAD 每帧的 `is_speech` 直接由单帧概率阈值决定，环境噪音或模型波动可能导致单帧毛刺。加一个简单的 3 帧滑动窗口投票。

- [ ] **Step 1: 在结构体中增加历史缓冲区**

```rust
pub struct SileroVad {
    session: Session,
    state: Vec<f32>,
    context: Vec<f32>,
    is_speaking: bool,
    model_input: Vec<f32>,
    /// Recent frame speech probabilities for smoothing (ring buffer, last N frames).
    prob_history: Vec<f32>,
}
const SMOOTHING_FRAMES: usize = 3;
```

构造函数中初始化：
```rust
prob_history: Vec::with_capacity(SMOOTHING_FRAMES),
```

- [ ] **Step 2: 实现平滑后的判断逻辑**

在 `process_frame()` 中，计算 `probability` 后、hysteresis 判断前插入：

```rust
// Accumulate probability history for smoothing
self.prob_history.push(probability);
if self.prob_history.len() > SMOOTHING_FRAMES {
    self.prob_history.remove(0);
}

// Use smoothed probability for hysteresis decision
let smoothed_prob = self.prob_history.iter().sum::<f32>() / self.prob_history.len() as f32;

let is_speech = if self.is_speaking {
    smoothed_prob > Self::THRESHOLD_OFF
} else {
    smoothed_prob > Self::THRESHOLD_ON
};
```

- [ ] **Step 3: reset_state 中重置平滑历史**

```rust
pub fn reset_state(&mut self) {
    self.state.fill(0.0);
    self.context.fill(0.0);
    self.is_speaking = false;
    self.prob_history.clear();
}
```

- [ ] **Step 4: 编译验证**

```bash
cd core && cargo build --lib 2>&1
```

预期：编译通过

- [ ] **Step 5: 确认测试通过**

```bash
cd core && cargo test --lib vad::silero::tests 2>&1
```

预期：`test result: ok`

- [ ] **Step 6: 提交**

```bash
git add core/src/vad/silero.rs && git commit -m "feat: add 3-frame smoothing window for VAD speech detection"
```

---

### Task 7: Pipeline VAD 推理失败降级

**Files:**
- Modify: `core/src/ffi/pipeline.rs:277`

**问题:** `vad.process_frame(&frame)?` 失败时 pipeline 线程直接退出，生产环境应降级为跳过当前帧并继续。

- [ ] **Step 1: 将 `?` 改为错误处理**

```rust
// Before (~line 277):
let vad_result = vad.process_frame(&frame)?;

// After:
let vad_result = match vad.process_frame(&frame) {
    Ok(r) => r,
    Err(e) => {
        log::error!("VAD inference failed, skipping frame: {:#}", e);
        continue;
    }
};
```

注意：`continue` 会跳过 `vad_logger.log` 和 `capture_dumper.feed`。需要考虑 logger/dumper 是否应该继续记录即使 VAD 失败。

更好的做法：

```rust
// 移到 continue 前：
vad_result = match vad.process_frame(&frame) {
    Ok(r) => r,
    Err(e) => {
        log::error!("VAD inference failed, skipping frame: {:#}", e);
        // 构造一个"不确定"的结果让状态机继续工作
        VadResult {
            probability: 0.0,
            is_speech: false,
        }
    }
};
```

为了使用 `VadResult`，需要在 `pipeline.rs` 顶部导入：
```rust
use crate::vad::silero::VadResult; // 确认路径
```

- [ ] **Step 2: 编译验证**

```bash
cd core && cargo build --lib 2>&1
```

预期：编译通过

- [ ] **Step 3: 确认测试通过**

```bash
cd core && cargo test --lib 2>&1
```

预期：全部测试通过

- [ ] **Step 4: 提交**

```bash
git add core/src/ffi/pipeline.rs && git commit -m "fix: degrade gracefully on VAD inference failure instead of crashing pipeline"
```

---

## 执行顺序总览

| 顺序 | Task | 影响范围 | 类型 |
|:---:|:---|:---|:---:|
| 1 | 修复 capture_to_vad.rs 帧长 bug | example 文件 | 修复 |
| 2 | 消除硬编码 32ms 帧长 | state_machine.rs | 修复 |
| 3 | 预分配 model_input Vec | silero.rs | 性能 |
| 4 | VAD 平滑滑动窗口 | silero.rs | 增强 |
| 5 | VAD 模型 warm-up | pipeline.rs | 性能 |
| 6 | pipeline 推理失败降级 | pipeline.rs | 健壮性 |
