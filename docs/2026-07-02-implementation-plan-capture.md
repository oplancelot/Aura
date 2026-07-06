# 实现计划：完成 capture.rs — WASAPI 进程级回环音频采集

> 日期：2026-07-02
> 状态：已批准

## 目标

将 [capture.rs](file:///d:/repo/aura/core/src/audio/capture.rs) 中的 TODO 桩代码替换为基于 `wasapi 0.23` crate 的完整 WASAPI 进程级回环采集实现。

## 依赖变更

| 依赖 | 旧版本 | 新版本 | 原因 |
|------|--------|--------|------|
| `wasapi` | `0.4` | `0.23` | 用户指定，使用最新版本 |
| `windows` | `0.58` | `0.62` | `wasapi 0.23.0` 要求 `windows ^0.62` |

**影响分析**：项目源码中无任何文件直接 `use windows::` 引用 `windows` crate 类型，升级无兼容性风险。

## 设计

### 结构体扩展

```rust
pub struct AudioCapturer {
    config: CaptureConfig,
    ring_buffer: Arc<AudioRingBuffer>,
    is_capturing: bool,
    capture_thread: Option<JoinHandle<()>>,   // 新增：采集线程句柄
    stop_signal: Arc<AtomicBool>,              // 新增：停止信号
}
```

### start() 方法流程

在 background thread 中执行：

1. `wasapi::initialize_mta()` — 初始化 COM MTA
2. `AudioClient::new_application_loopback_client(pid, include_tree)` — 创建进程回环客户端
3. 配置 `WaveFormat`：f32 / 48kHz / 2ch（带 autoconvert）
4. `initialize_client()` 使用 `EventsShared { autoconvert: true, buffer_duration_hns: 0 }`
5. `set_get_eventhandle()` → `get_audiocaptureclient()`
6. `start_stream()` → 进入事件循环：
   - `WaitForSingleObject(h_event, 100ms)` 带超时等待
   - `read_from_device_to_deque()` 读取原始字节
   - `u8` → `f32` 采样转换
   - 立体声混缩单声道：`(L + R) / 2.0`
   - `Resampler` 48kHz → 16kHz
   - `ring_buffer.push()` 写入

### stop() 方法流程

1. `stop_signal.store(true)` — 通知线程退出
2. `capture_thread.take().join()` — 等待线程结束
3. `is_capturing = false`

### 数据流

```
WASAPI EventsShared          capture thread              VAD thread
┌──────────────┐   event    ┌──────────────┐  push()   ┌──────────────┐
│ Audio Engine │──────────→ │ read_from_   │──────────→│ AudioRing    │
│ (48kHz/f32/  │            │ device_to_   │  (16kHz   │ Buffer       │
│  2ch)        │            │ deque → mono │   mono    │              │
│              │            │ → resample   │   f32)    │              │
└──────────────┘            └──────────────┘           └──────────────┘
```

## 变更文件清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `core/Cargo.toml` | MODIFY | wasapi 0.4→0.23, windows 0.58→0.62 |
| `core/src/audio/capture.rs` | MODIFY | 完整实现 AudioCapturer |

## 验证计划

```bash
cargo check
cargo build
```
