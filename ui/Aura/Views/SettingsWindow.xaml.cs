using System;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Threading;

namespace Aura.Views;

public partial class SettingsWindow : Window
{
    private readonly DispatcherTimer _refreshTimer;

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
        if (ApiKeyPanel == null) return;
        ApiKeyPanel.Visibility = EngineComboBox.SelectedIndex == 1
            ? Visibility.Visible
            : Visibility.Collapsed;
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
