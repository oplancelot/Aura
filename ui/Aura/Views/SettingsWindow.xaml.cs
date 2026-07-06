using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;

namespace Aura.Views;

public class ModelSource
{
    public string Name { get; init; } = "";
    public string FileName { get; init; } = "";
    public string Url { get; init; } = "";
    public string? ExpectedSha256 { get; init; }
}

public partial class SettingsWindow : Window
{
    private static readonly ModelSource VadSource = new()
    {
        Name = "Silero VAD",
        FileName = "silero_vad.onnx",
        Url = "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx",
    };

    private static readonly ModelSource AsrSource = new()
    {
        Name = "SenseVoice-Small ASR",
        FileName = "sense-voice-small-q4_k.gguf",
        Url = "https://huggingface.co/lovemefan/SenseVoiceGGUF/resolve/main/sense-voice-small-q4_k.gguf",
    };

    private readonly DispatcherTimer _refreshTimer;
    private readonly ModelManager _vadManager;
    private readonly ModelManager _asrManager;
    private bool _updateCheckDone;

    public SettingsWindow()
    {
        InitializeComponent();
        OnRefreshProcesses(this, new RoutedEventArgs());

        _refreshTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(3)
        };
        _refreshTimer.Tick += (_, _) => OnRefreshProcesses(this, new RoutedEventArgs());
        _refreshTimer.Start();

        // VAD manager
        _vadManager = new ModelManager(VadSource.FileName);
        _vadManager.PropertyChanged += OnVadPropertyChanged;
        RefreshVadStatus();

        // ASR manager
        _asrManager = new ModelManager(AsrSource.FileName);
        _asrManager.PropertyChanged += OnAsrPropertyChanged;
        RefreshAsrStatus();

        _ = CheckForUpdatesAsync();
    }

    private void RefreshVadStatus()
    {
        _vadManager.RefreshStatus();
        VadStatusText.Text = _vadManager.StatusMessage;
        DownloadVadButton.IsEnabled = _vadManager.CanDownload;
        VadProgressBar.Visibility = Visibility.Collapsed;
    }

    private void RefreshAsrStatus()
    {
        _asrManager.RefreshStatus();
        AsrStatusText.Text = _asrManager.StatusMessage;
        DownloadAsrButton.IsEnabled = _asrManager.CanDownload;
        AsrProgressBar.Visibility = Visibility.Collapsed;
    }

    private void BindModelToUI(ModelManager mgr, TextBlock statusText, Button btn, ProgressBar bar)
    {
        statusText.Text = mgr.StatusMessage;
        btn.IsEnabled = mgr.CanDownload;
        bar.Visibility = mgr.IsBusy ? Visibility.Visible : Visibility.Collapsed;
        if (mgr.IsBusy) bar.Value = mgr.Progress;
    }

    private void OnVadPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        Dispatcher.Invoke(() => BindModelToUI(_vadManager, VadStatusText, DownloadVadButton, VadProgressBar));
    }

    private void OnAsrPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        Dispatcher.Invoke(() => BindModelToUI(_asrManager, AsrStatusText, DownloadAsrButton, AsrProgressBar));
    }

    private async void OnDownloadVadClick(object sender, RoutedEventArgs e) =>
        await DownloadModel(_vadManager, VadSource, VadProgressBar);

    private async void OnDownloadAsrClick(object sender, RoutedEventArgs e) =>
        await DownloadModel(_asrManager, AsrSource, AsrProgressBar);

    private async Task DownloadModel(ModelManager mgr, ModelSource src, ProgressBar bar)
    {
        var progress = new Progress<double>(pct =>
            Dispatcher.Invoke(() => bar.Value = pct));
        await mgr.DownloadAsync(src.Url, src.ExpectedSha256 ?? "", progress);

        if (mgr.IsInstalled && src == AsrSource)
        {
            var path = System.IO.Path.Combine(
                AppDomain.CurrentDomain.BaseDirectory, src.FileName);
            Interop.AuraCoreBinding.SetAsrModelPath(path);
        }
    }

    private async Task CheckForUpdatesAsync()
    {
        if (_updateCheckDone) return;
        _updateCheckDone = true;

        var result = await UpdateChecker.CheckAsync();
        Dispatcher.Invoke(() =>
        {
            if (result.HasUpdate && result.Info != null)
            {
                var tag = result.Info.TargetFullRelease.Version;
                UpdateStatusText.Text = $"v{tag} available!";
                CheckUpdateButton.Content = "Download & Restart";
                CheckUpdateButton.Click -= OnCheckUpdateClick;
                CheckUpdateButton.Click += async (_, _) =>
                {
                    CheckUpdateButton.IsEnabled = false;
                    UpdateStatusText.Text = "Downloading...";
                    try
                    {
                        await UpdateChecker.DownloadUpdateAsync(result.Info);
                        UpdateChecker.ApplyAndRestart(result.Info);
                    }
                    catch (Exception ex)
                    {
                        UpdateStatusText.Text = $"Update failed: {ex.Message}";
                        CheckUpdateButton.IsEnabled = true;
                    }
                };
            }
            else if (result.ErrorMessage != null)
            {
                UpdateStatusText.Text = $"Check failed: {result.ErrorMessage}";
            }
            else
            {
                UpdateStatusText.Text = $"v{result.CurrentVersion} — up to date";
                CheckUpdateButton.IsEnabled = false;
            }
        });
    }

    private async void OnCheckUpdateClick(object sender, RoutedEventArgs e)
    {
        CheckUpdateButton.IsEnabled = false;
        UpdateStatusText.Text = "Checking...";
        _updateCheckDone = false;
        await CheckForUpdatesAsync();
    }

    private void OnRefreshProcesses(object sender, RoutedEventArgs e)
    {
        var prevPid = ProcessComboBox.SelectedItem is ComboBoxItem prev
            ? (int)prev.Tag : -1;

        ProcessComboBox.Items.Clear();
        ProcessComboBox.Items.Add(new ComboBoxItem
        {
            Content = "Self test (simulated subtitles)",
            Tag = 0
        });

        var targetApps = new[] { "msedge", "edge", "chrome", "vlc", "discord",
                                 "ms-teams", "MSTeams", "Teams" };

        var processes = Process.GetProcesses()
            .Where(p => p.Id > 0 && targetApps.Any(app =>
                p.ProcessName.Contains(app, StringComparison.OrdinalIgnoreCase)))
            .OrderBy(p => p.ProcessName)
            .ToList();

        var selectedIdx = 0;
        for (int i = 0; i < processes.Count; i++)
        {
            var proc = processes[i];
            ProcessComboBox.Items.Add(new ComboBoxItem
            {
                Content = $"{proc.ProcessName} (PID: {proc.Id})",
                Tag = proc.Id
            });
            if (proc.Id == prevPid) selectedIdx = i + 1; // +1 for Self Test
        }

        ProcessComboBox.SelectedIndex = selectedIdx;
    }

    private void OnEngineChanged(object sender, SelectionChangedEventArgs e)
    {
        // cloud-only engine disabled — no-op in local-only mode
    }

    private void OnStartClick(object sender, RoutedEventArgs e)
    {
        if (ProcessComboBox.SelectedItem is not ComboBoxItem selectedProcess)
        {
            MessageBox.Show("Please select a target voice application.",
                "Aura", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        var pid = (int)selectedProcess.Tag;

        var engineName = "sensevoice";
        Interop.AuraCoreBinding.SetEngine(engineName);

        // API key and target language are cloud-only — not used in local mode

        int result = Interop.AuraCoreBinding.Start((uint)pid);
        if (result == 0)
        {
            StartButton.IsEnabled = false;
            StopButton.IsEnabled = true;
            this.WindowState = WindowState.Minimized;
        }
        else
        {
            MessageBox.Show("Failed to start translation pipeline.",
                "Aura Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private void OnStopClick(object sender, RoutedEventArgs e)
    {
        Interop.AuraCoreBinding.Stop();
        StartButton.IsEnabled = true;
        StopButton.IsEnabled = false;
    }
}
