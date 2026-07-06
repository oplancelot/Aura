# ASR 测试指南

## 快速命令

```powershell
# 1. 编译 Rust（release）
cd core && cargo build --release

# 2. 复制 DLL 到 UI bin
Copy-Item core\target\release\aura_core.dll ui\Aura\bin\Release\net10.0-windows\ -Force

# 3. 编译 C# UI
cd .. && dotnet build ui\Aura\Aura.csproj -c Release

# 4. 启动
Start-Process -WorkingDirectory ui\Aura\bin\Release\net10.0-windows -FilePath Aura.exe
```

---

## 日志目录

ASR 输出自动保存在：

```
ui/Aura/bin/Release/net10.0-windows/logs/asr_YYYYMMDD_HHmmss.txt
```

格式：`[秒]\t[P/F]\t[文本]`

| 字段 | 说明 |
|------|------|
| elapsed | 从启动到输出的秒数 |
| type | P = provisional（渐进），F = final（最终） |
| text | 识别文本 |

---

## 测试结果汇总

| 日期 | 音频 | 识别文本 | 参考文本 | 评估 |
|------|------|----------|----------|------|
| 07/06 00:20 | LibriSpeech dev-clean 84-121123 (VLC) | `The net in which he was taken.`（仅 1/29 句） | `IN VAIN ENDEAVORING TO ESCAPE THE NET IN WHICH HE WAS TAKEN I RAVE` | ASR 质量 ✅ 但 recall ❌ |
| 07/06 01:42 | LJSpeech LJ001-0001 (VLC) | 无识别（仅 flush） | `Printing, in the only sense...` | 音频捕获失败 ❌ |
| 07/06 01:42 (2) | LJSpeech (VLC, 7min) | 无识别（仅 flush） | 同上 | 音频捕获失败 ❌ |

**核心问题：WASAPI process loopback 对 VLC 无效，音频未进入管线。**

---

## 流程说明

```
VLC 播放音频
  → WASAPI Loopback 捕获（当前是 process-specific，需改 device loopback）
    → 16kHz 重采样
      → Silero VAD 检测语音
        → Chunking State Machine 分割句子
          → SenseVoice-Small ASR 识别
            → C# Overlay 显示 + logs/*.txt 记录
```

自测模式（PID 0）绕过音频捕获，直接产生模拟字幕用于验证 UI 流程。
