using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;

namespace Aura.Views;

public partial class SettingsWindow : Window
{
    private readonly DispatcherTimer _refreshTimer;
    private readonly ModelManager _modelManager;
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

        // Model manager
        _modelManager = new ModelManager("sense-voice-small-q4_k.gguf");
        _modelManager.PropertyChanged += OnModelPropertyChanged;
        RefreshModelStatus();

        // Check for app updates on load (once)
        _ = CheckForUpdatesAsync();
    }

    private void RefreshModelStatus()
    {
        _modelManager.RefreshStatus();
        ModelStatusText.Text = _modelManager.StatusMessage;
        DownloadModelButton.IsEnabled = _modelManager.CanDownload;
        ModelProgressBar.Visibility = Visibility.Collapsed;
    }

    private void OnModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        Dispatcher.Invoke(() =>
        {
            ModelStatusText.Text = _modelManager.StatusMessage;
            DownloadModelButton.IsEnabled = _modelManager.CanDownload;
            ModelProgressBar.Visibility = _modelManager.IsBusy ? Visibility.Visible : Visibility.Collapsed;
            if (_modelManager.IsBusy)
                ModelProgressBar.Value = _modelManager.Progress;
        });
    }

    private async void OnDownloadModelClick(object sender, RoutedEventArgs e)
    {
        DownloadModelButton.IsEnabled = false;

        var sourceUrl = "https://huggingface.co/lovemefan/SenseVoiceGGUF/resolve/main/sense-voice-small-q4_k.gguf";

        var progress = new Progress<double>(pct =>
        {
            Dispatcher.Invoke(() => ModelProgressBar.Value = pct);
        });

        await _modelManager.DownloadAsync(sourceUrl, progress: progress);

        if (_modelManager.IsInstalled)
        {
            // Signal core to reload model path
            var modelPath = System.IO.Path.Combine(
                AppDomain.CurrentDomain.BaseDirectory, "sense-voice-small-q4_k.gguf");
            Interop.AuraCoreBinding.SetAsrModelPath(modelPath);
        }
    }

    private async Task CheckForUpdatesAsync()
    {
        if (_updateCheckDone) return;
        _updateCheckDone = true;

        var result = await UpdateChecker.CheckAsync();
        Dispatcher.Invoke(() =>
        {
            if (result.HasUpdate && result.Latest != null)
            {
                UpdateStatusText.Text = $"v{result.Latest.TagName.TrimStart('v')} available!";
                CheckUpdateButton.Content = "Download";
                CheckUpdateButton.Click -= OnCheckUpdateClick;
                CheckUpdateButton.Click += async (_, _) =>
                {
                    Process.Start(new ProcessStartInfo
                    {
                        FileName = result.Latest.HtmlUrl,
                        UseShellExecute = true
                    });
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
        await CheckForUpdatesAsync();
        CheckUpdateButton.IsEnabled = true;
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
