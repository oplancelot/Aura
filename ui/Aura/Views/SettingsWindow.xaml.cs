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

        _ = CheckForUpdatesAsync();
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

        var appPids = Interop.NativeMethods.GetVisibleAppPids();

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
            if (proc.Id == prevPid) selectedIdx = i + 1;
        }

        ProcessComboBox.SelectedIndex = selectedIdx;
    }

    private void OnEngineChanged(object sender, SelectionChangedEventArgs e)
    {
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
