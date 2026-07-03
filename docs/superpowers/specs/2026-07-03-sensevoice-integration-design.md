# SenseVoice 本地 ASR 引擎集成设计

## 目标

将 SenseVoice-Small 本地模型集成到 Aura 实时管线中，替换 Phase A 的 mock 文本，输出真实的语音识别结果。

## 架构

```
SenseVoice.cpp + ggml (C++ 静态库)
    ↑ CMake 编译
C Wrapper (extern "C")
    ↑ build.rs (cmake crate + cc crate)
Rust FFI 绑定
    ↑
SenseVoiceEngine::transcribe()  ←  pipeline.rs 调用
```

## 关键决策

- **编译方式**：使用 `cmake` crate 编译 SenseVoice.cpp + ggml，`cc` crate 编译 C 包装层
- **模型路径**：通过 `aura_core_set_model_path` 扩展，指向 `sense-voice-small-q4_k.gguf`
- **推理模式**：非流式 — Final/HardCut chunk 积累够后一次性推理；Provisional chunk 不做推理
- **线程安全**：`SenseVoiceContext` 用 `Mutex` 包裹，一次一个推理

## C 包装层 API

```c
void* sense_voice_load(const char* model_path, int use_gpu);
int sense_voice_transcribe(void* ctx, const float* pcm_16khz_mono,
                           int num_samples, char* out_text, int max_len);
void sense_voice_free(void* ctx);
```

## 管线集成点

`pipeline.rs` 的 `run_pipeline()` 中，当收到 `ChunkType::Final` 或 `HardCut` 时：
1. 将 chunk.samples 传递给 `SenseVoiceEngine::transcribe()`
2. 将返回文本通过 `emit_translation()` 发给 C# 回调
3. Provisional chunk 仍使用 "[~] Xms speech..." mock 文本（实时性要求 > 准确性）

## 验证

- `cargo test` — 7 个现有测试不破坏
- 手动测试：运行 `capture_to_vad` example，观察 SenseVoice 输出
