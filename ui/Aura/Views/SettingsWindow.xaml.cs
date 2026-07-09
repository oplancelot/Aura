using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;
using Aura.Localization;

namespace Aura.Views;

public partial class SettingsWindow : Window
{
    private readonly DispatcherTimer _refreshTimer;
    private bool _updateCheckDone;
    private UpdateCheckResult? _lastCheckResult;

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

        _ = CheckForUpdatesAsync();
    }

    private async Task CheckForUpdatesAsync()
    {
        if (_updateCheckDone) return;
        _updateCheckDone = true;

        var lm = LanguageManager.Instance;
        var result = await UpdateChecker.CheckAsync();
        _lastCheckResult = result;
        Dispatcher.Invoke(() =>
        {
            if (result.HasUpdate && result.Info != null)
            {
                var tag = result.Info.TargetFullRelease.Version.ToString();
                UpdateStatusText.Text = lm.UpdateAvailable(tag);
                CheckUpdateButton.Content = lm.DownloadAndRestart;
                ResetCheckButtonStyle();
                CheckUpdateButton.Click -= OnCheckUpdateClick;
                CheckUpdateButton.Click += async (_, _) =>
                {
                    CheckUpdateButton.IsEnabled = false;
                    UpdateStatusText.Text = lm.Downloading;
                    try
                    {
                        await UpdateChecker.DownloadUpdateAsync(result.Info);
                        UpdateChecker.ApplyAndRestart(result.Info);
                    }
                    catch (Exception ex)
                    {
                        UpdateStatusText.Text = lm.UpdateFailed(ex.Message);
                        CheckUpdateButton.IsEnabled = true;
                    }
                };
                CheckUpdateButton.IsEnabled = true;
                CheckUpdateButton.Visibility = Visibility.Visible;
                UpdateStatusText.Visibility = Visibility.Visible;
            }
            else if (result.ErrorMessage != null)
            {
                UpdateStatusText.Text = lm.CheckFailedGithub;
                CheckUpdateButton.Content = lm.OpenGithub;
                SetGitHubButtonStyle();
                CheckUpdateButton.Click -= OnCheckUpdateClick;
                CheckUpdateButton.Click += (_, _) =>
                    Process.Start(new ProcessStartInfo
                    {
                        FileName = "https://github.com/oplancelot/Aura/releases",
                        UseShellExecute = true
                    });
                CheckUpdateButton.IsEnabled = true;
                CheckUpdateButton.Visibility = Visibility.Visible;
                UpdateStatusText.Visibility = Visibility.Visible;
            }
            else
            {
                UpdateStatusText.Visibility = Visibility.Collapsed;
                CheckUpdateButton.Visibility = Visibility.Collapsed;
            }
        });
    }

    private async void OnCheckUpdateClick(object sender, RoutedEventArgs e)
    {
        ResetCheckButtonStyle();
        CheckUpdateButton.IsEnabled = false;
        UpdateStatusText.Text = LanguageManager.Instance.CheckInProgress;
        UpdateStatusText.Visibility = Visibility.Visible;
        CheckUpdateButton.Visibility = Visibility.Visible;
        _updateCheckDone = false;
        await CheckForUpdatesAsync();
    }

    private void OnRefreshProcesses(object sender, RoutedEventArgs e)
    {
        var prevPid = ProcessComboBox.SelectedItem is ComboBoxItem prev
            ? (int)prev.Tag : -1;

        ProcessComboBox.Items.Clear();

        var appPids = Interop.NativeMethods.GetVisibleAppPids();
        appPids.Remove((uint)Environment.ProcessId);

        var blocklist = new[] { "Taskmgr", "explorer", "SystemSettings", "SearchApp",
                                "TextInputHost", "ShellExperienceHost", "LockApp",
                                "PeopleExperienceHost", "StartMenuExperienceHost" };
        appPids.RemoveWhere(pid =>
        {
            try { using var p = Process.GetProcessById((int)pid); return blocklist.Contains(p.ProcessName); }
            catch { return false; }
        });

        var audioPids = Interop.AudioSessionEnumerator.GetPidsWithAudio();
        if (audioPids != null && audioPids.Count > 0)
            appPids.IntersectWith(audioPids);

        var processes = Process.GetProcesses()
            .Where(p => p.Id > 0 && appPids.Contains((uint)p.Id))
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
            if (proc.Id == prevPid) selectedIdx = i;
        }

        var selfTestIdx = processes.Count;
        ProcessComboBox.Items.Add(new ComboBoxItem
        {
            Content = LanguageManager.Instance.SelfTestItem,
            Tag = 0
        });

        ProcessComboBox.SelectedIndex = prevPid == 0 ? selfTestIdx : selectedIdx;
    }

    private void OnToggleLanguage(object sender, RoutedEventArgs e)
    {
        LanguageManager.Instance.ToggleLanguage();
        RefreshUpdateText();
    }

    private void RefreshUpdateText()
    {
        if (_lastCheckResult == null) return;
        var lm = LanguageManager.Instance;

        if (_lastCheckResult.HasUpdate && _lastCheckResult.Info != null)
        {
            ResetCheckButtonStyle();
            UpdateStatusText.Text = lm.UpdateAvailable(_lastCheckResult.Info.TargetFullRelease.Version.ToString());
            CheckUpdateButton.Content = lm.DownloadAndRestart;
        }
        else if (_lastCheckResult.ErrorMessage != null)
        {
            UpdateStatusText.Text = lm.CheckFailedGithub;
            CheckUpdateButton.Content = lm.OpenGithub;
            SetGitHubButtonStyle();
        }
    }

    private void ResetCheckButtonStyle()
    {
        CheckUpdateButton.ClearValue(Button.PaddingProperty);
        CheckUpdateButton.ClearValue(Button.FontSizeProperty);
        CheckUpdateButton.ClearValue(Button.BackgroundProperty);
        CheckUpdateButton.ClearValue(Button.BorderBrushProperty);
    }

    private void SetGitHubButtonStyle()
    {
        CheckUpdateButton.Padding = new Thickness(10, 2, 10, 2);
        CheckUpdateButton.FontSize = 11;
        CheckUpdateButton.Background = System.Windows.Media.Brushes.Transparent;
        CheckUpdateButton.BorderBrush = System.Windows.Media.Brushes.Gray;
    }

    private void OnEngineChanged(object sender, SelectionChangedEventArgs e)
    {
    }

    private void OnStartClick(object sender, RoutedEventArgs e)
    {
        if (ProcessComboBox.SelectedItem is not ComboBoxItem selectedProcess)
        {
            MessageBox.Show(LanguageManager.Instance.SelectTargetApp,
                LanguageManager.Instance.DialogTitleAura, MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        var pid = (int)selectedProcess.Tag;

        var engineName = "sensevoice";
        Interop.AuraCoreBinding.SetEngine(engineName);

        int result = Interop.AuraCoreBinding.Start((uint)pid);
        if (result == 0)
        {
            StartButton.IsEnabled = false;
            StopButton.IsEnabled = true;
            this.WindowState = WindowState.Minimized;
        }
        else
        {
            MessageBox.Show(LanguageManager.Instance.StartPipelineFailed,
                LanguageManager.Instance.DialogTitleError, MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    private void OnStopClick(object sender, RoutedEventArgs e)
    {
        Interop.AuraCoreBinding.Stop();
        StartButton.IsEnabled = true;
        StopButton.IsEnabled = false;
    }

    private void OnWindowClosing(object? sender, CancelEventArgs e)
    {
        Application.Current.Shutdown();
    }
}
