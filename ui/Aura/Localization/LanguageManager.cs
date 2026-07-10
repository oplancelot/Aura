using System.ComponentModel;
using System.Windows;

namespace Aura.Localization;

public class LanguageManager : INotifyPropertyChanged
{
    public static LanguageManager Instance { get; } = new();

    public bool IsChinese { get; private set; } = true;

    public string LangLabel => IsChinese ? "English" : "中文";
    public string LangTooltip => IsChinese ? "切换到英文" : "Switch to Chinese";

    // ── Window ──────────────────────────────────────────────────────

    public string WindowTitle => Get("Aura 设置", "Aura Settings");

    // ── Title section ───────────────────────────────────────────────

    public string Subtitle => Get("实时音视频转字幕", "Real-time Audio/Video Subtitles");

    // ── Target App section ──────────────────────────────────────────

    public string TargetAppLabel => Get("目标应用", "Target App");
    public string TargetAppHint => Get("选择正在播放音频的应用，或使用自测模式", "Select an app playing audio, or use Self Test");
    public string SelfTestItem => Get("自测模式（模拟字幕）", "Self Test (simulated subtitles)");

    // ── Translate To section ────────────────────────────────────────

    public string TranslateToLabel => Get("翻译到", "Translate To");
    public string ChineseItem => Get("中文", "Chinese");
    public string EnglishItem => Get("英文", "English");
    public string TranslateToNote => Get("待开发", "Pending");

    // ── Engine section ──────────────────────────────────────────────

    public string EngineLabel => Get("引擎", "Engine");
    public string SenseVoiceItem => Get("SenseVoice-Small（本地离线）", "SenseVoice-Small (Local, offline)");
    public string GeminiItem => Get("Gemini 2.5 Flash（云端）", "Gemini 2.5 Flash (Cloud)");

    // ── Updates section ─────────────────────────────────────────────

    public string UpdatesLabel => Get("更新", "Updates");
    public string CheckButton => Get("检查更新", "Check for Updates");
    public string CheckInProgress => Get("检查中...", "Checking...");
    public string UpdateAvailable(string tag) => string.Format(Get("v{0} 可用", "v{0} available"), tag);
    public string DownloadAndRestart => Get("下载并重启", "Download && Restart");
    public string Downloading => Get("下载中...", "Downloading...");
    public string UpdateFailed(string msg) => string.Format(Get("更新失败：{0}", "Update failed: {0}"), msg);
    public string CheckFailedGithub => Get("检查失败，请前往 GitHub 查看最新版本", "Check failed — see GitHub for latest");
    public string OpenGithub => Get("打开 GitHub", "Open GitHub");

    // ── Controls ────────────────────────────────────────────────────

    public string ControlsLabel => Get("快捷键", "Shortcuts");
    public string ToggleMode => Get("Ctrl+Shift+L  切换字幕拖拽模式", "Ctrl+Shift+L  Toggle subtitle drag mode");
    public string DragHint => Get("拖拽模式下可自由移动字幕位置", "Drag mode allows repositioning subtitles");

    // ── Action buttons ──────────────────────────────────────────────

    public string StartButton => Get("▶ 开始", "▶ Start");
    public string StopButton => Get("⏹ 停止", "⏹ Stop");

    // ── Dialogs ─────────────────────────────────────────────────────

    public string SelectTargetApp => Get("请选择目标应用。", "Please select a target app.");
    public string DialogTitleAura => "Aura";
    public string DialogTitleError => Get("Aura 错误", "Aura Error");
    public string StartPipelineFailed => Get("启动翻译管道失败。", "Failed to start translation pipeline.");

    // ── App.xaml.cs ─────────────────────────────────────────────────

    public string CoreInitFailed => Get("Aura 核心引擎初始化失败。", "Failed to initialise Aura core engine.");
    public string DownloadFailed(string fileName, string msg) =>
        string.Format(Get("下载 {0} 失败：\n{1}", "Failed to download {0}:\n{1}"), fileName, msg);

    // ── Helpers ─────────────────────────────────────────────────────

    private string Get(string zh, string en) => IsChinese ? zh : en;

    public void SetLanguage(bool chinese)
    {
        if (IsChinese == chinese) return;
        IsChinese = chinese;
        OnAllPropertiesChanged();
    }

    public void ToggleLanguage()
    {
        IsChinese = !IsChinese;
        OnAllPropertiesChanged();
    }

    private void OnAllPropertiesChanged()
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(""));
    }

    public event PropertyChangedEventHandler? PropertyChanged;
}
