# Aura ASR 测试指南

> 从编译到测试的完整流程。

---

## 1. 编译

### 1.1 Rust 核心库

```powershell
cd core
cargo build --release
```

产物：`core/target/release/aura_core.dll`（~22 MB）

### 1.2 C# UI

```powershell
cd ..
dotnet build ui\Aura\Aura.csproj -c Release
```

### 1.3 复制 DLL 到 UI 目录

```powershell
Copy-Item core\target\release\aura_core.dll ui\Aura\bin\Release\net10.0-windows\ -Force
```

---

## 2. 测试方式

有三种测试方式，按推荐优先级排列：

### 2.1 离线 CLI 测试（推荐）

跳过音频捕获和 VAD，直接读 WAV 文件跑 SenseVoice ASR。最快、最可靠。

**单文件测试：**

```powershell
cargo run --release --example transcribe_wav -- "OpenSLR/LJSpeech/wavs/LJ001-0001.wav"
```

**输出示例：**

```
=== Full transcription ===
Printing, in the only sense with which we are at present concerned, differs from most, If not from all the arts and crafts represented in the exhibition.

=== Reference ===
Printing, in the only sense with which we are at present concerned, differs from most if not from all the arts and crafts represented in the Exhibition

WER: 0.0%
```

**参考文本自动匹配：**

| 数据集 | 匹配规则 |
|--------|----------|
| LJSpeech | `metadata.csv` 第一列 = WAV 文件名前缀，第二列 = 参考文本 |
| LibriSpeech | 同级目录 `.trans.txt`，行前缀 = 文件名 |

**无参考文本时会跳过对比，只输出转录结果。**

### 2.2 批量测试（评估 ASR 整体质量）

```bash
# 用批量脚本（自动编译）
python scripts/run_asr_batch.py --max-files 10   # 测前 10 个
python scripts/run_asr_batch.py                    # 测全部 13,100 个
```

输出 CSV：`asr_batch_results.csv`

| 字段 | 说明 |
|------|------|
| File | WAV 文件名 |
| WER | 词错误率（百分比） |
| Time | ASR 处理耗时（秒） |
| Hyp | 识别文本 |
| Ref | 参考文本 |

### 2.3 在线 GUI 测试（验证完整链路）

启动 Aura：

```powershell
Start-Process -WorkingDirectory ui\Aura\bin\Release\net10.0-windows -FilePath Aura.exe
```

操作步骤：

| 步骤 | 操作 |
|------|------|
| 1 | 打开浏览器（Edge/Chrome）播放音频或视频 |
| 2 | Aura 下拉框自动刷新出进程（每 3 秒），选中它 |
| 3 | 点 **Start Translation** |
| 4 | 等待音频播放，观察字幕 |
| 5 | 点 **Stop** |
| 6 | 查看 `bin/Release/net10.0-windows/logs/asr_*.txt` |

**日志格式：**

```
# [elapsed]	[type]	[text]
# ---------	------	------
63.904	    F	    The net in which he was taken.
151.985	    F	    [?] 20ms (flush)
```

| 字段 | 说明 |
|------|------|
| elapsed | 从启动到输出的秒数 |
| type | P = provisional（渐进），F = final（最终） |
| text | 识别文本，`[✓]`=无ASR回退，`[~]`=provisional，`[?]`=编码问题 |

### 2.4 自测模式（验证 UI 流程）

选择 **Self Test (PID 0)** → **Start Translation**，验证：

- 字幕逐字出现（打字机效果）
- 句号后停顿 6 秒
- 下一句继续
- 复用 TextBlock 无闪烁

---

## 3. 诊断日志

所有日志文件保存在运行目录下的 `logs/` 文件夹（即 `ui/Aura/bin/Release/net10.0-windows/logs/`）。

| 文件 | 说明 |
|------|------|
| `asr_*.txt` | ASR 识别结果（C# 写入） |
| `vad_*.csv` | VAD 每帧概率（Rust 诊断） |
| `capture_dump_*.raw` | 捕获前 10s 原始 PCM |

### VAD 诊断（vad_*.csv）

记录每一帧（32ms）的 VAD 输出，用于分析 VAD 是否正常工作。

| 字段 | 说明 |
|------|------|
| elapsed_ms | 从启动到该帧的毫秒数 |
| probability | VAD 概率 [0,1] |
| is_speech | 1=语音，0=静音 |

**判断标准：**

- 语音段 probability 应 > 0.5，静音段 < 0.1
- 若全程 < 0.2 → 捕获到的音频可能是静音（捕获问题）
- 若概率在 0.3~0.6 波动 → VAD 阈值需要调整

### 捕获音频 Dump（capture_dump_*.raw）

前 10 秒原始 PCM 数据，16kHz mono f32 小端字节序，**无 WAV 头**。

回放验证方法：

```powershell
# 用 transcribe_wav 转写 dump 文件
# 需要先封装 WAV 头，或用 Python
python -c "
import numpy as np
import soundfile as sf
data = np.frombuffer(open('logs/capture_dump_*.raw','rb').read(), dtype=np.float32)
sf.write('dump.wav', data, 16000)
"
```

然后用 `transcribe_wav` 转写生成的 `dump.wav`：

```powershell
cargo run --release --example transcribe_wav -- "dump.wav"
```

如果转写结果为空或有明显异常 → 捕获链路有问题，需检查 WASAPI 回环。

---

## 4. 完整链路说明

```
VLC/浏览器播放音频
  → WASAPI 进程环回捕获（new_application_loopback_client）
    → 48kHz → 16kHz 重采样（rubato）
      → Silero VAD 检测语音（阈值 ON=0.10, OFF=0.05）
        → 状态机分割句子（silence_close=1200ms）
          → SenseVoice-Small ASR（Q4_K 量化，182MB）
            → 字幕叠加 + logs/*.txt
```

---

## 5. 测试结果参考

| 测试方式 | 音频 | WER | 说明 |
|----------|------|-----|------|
| CLI | LJSpeech LJ001-0001（9.7s） | 0.0% | ASR 质量好 |
| CLI | LJSpeech 全部 13,100 条 | TBD | 跑批量脚本 |
| GUI+VLC | LibriSpeech 84-121123（~5min） | 仅 1/29 句 | 捕获/VAD 问题 |

**诊断方法：** 运行后检查 `logs/vad_*.csv` 确认 VAD 概率，用 `logs/capture_dump_*.raw` 回放确认捕获音频质量。

**已知问题：**

- WASAPI process loopback 对 VLC 可能捕获不到音频
- VAD 对 22kHz 音频概率偏低（峰值 ~0.16），已设阈值 ON=0.10 补偿
- 降级策略：process loopback 失败 → device loopback（捕获所有系统输出）

---

## 8. 模型下载

Aura 使用两个模型，下载地址：

| 模型 | 文件 | 大小 | 下载地址 |
|------|------|------|----------|
| Silero VAD v4 | `silero_vad.onnx` | ~2.3 MB | `https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx` |
| SenseVoice-Small (Q4_K) | `sense-voice-small-q4_k.gguf` | ~173 MB | `https://huggingface.co/lovemefan/SenseVoiceGGUF/resolve/main/sense-voice-small-q4_k.gguf` |

- VAD 模型内置在 git 仓库 `assets/` 中，一般不需要手动下载。
- ASR 模型太大，不会进入 git，首次启动时可在设置界面点击下载。下载进度条会在模型下载完成后自动消失。

---

## 6. 调整参数

### VAD 阈值

`core/src/vad/silero.rs`：

```rust
pub const THRESHOLD_ON: f32 = 0.10;   // 静音→语音 触发阈值
pub const THRESHOLD_OFF: f32 = 0.05;  // 语音→静音 回落阈值
```

### 句子分割

`core/src/vad/state_machine.rs`：

```rust
silence_close_ms: 1200,      // 静音多久才切句子（ms）
provisional_start_ms: 2000,  // 连续语音多久开始输出 interim
hard_cut_ms: 28_000,         // 最长语音段（防 OOM）
```

---

## 7. 单元测试

```powershell
cd core
cargo test
```

7 个测试：

| 测试 | 说明 |
|------|------|
| `empty_queue_is_noop` | 空音频环回缓冲 |
| `partial_frame_preserved_in_queue` | 部分帧保留 |
| `stereo_to_mono_mixdown` | 立体声→单声道 |
| `resample_ratio_is_correct` | 重采样比例 |
| `passthrough_when_same_rate` | 同采样率直通 |
| `downsample_3x_preserves_length_ratio` | 3 倍降采样 |
| `test_silero_vad_init_and_zeros` | VAD 对静音输出低概率 |
