using System;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;

namespace Aura.Views;

public partial class SettingsWindow : Window
{
    public SettingsWindow()
    {
        InitializeComponent();
        OnRefreshProcesses(this, new RoutedEventArgs());
    }

    private void OnRefreshProcesses(object sender, RoutedEventArgs e)
    {
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

        foreach (var proc in processes)
        {
            ProcessComboBox.Items.Add(new ComboBoxItem
            {
                Content = $"{proc.ProcessName} (PID: {proc.Id})",
                Tag = proc.Id
            });
        }

        ProcessComboBox.SelectedIndex = 0;
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

        var engineName = EngineComboBox.SelectedIndex == 0 ? "sensevoice" : "gemini";
        Interop.AuraCoreBinding.SetEngine(engineName);

        if (!string.IsNullOrWhiteSpace(ApiKeyTextBox.Text))
        {
            Interop.AuraCoreBinding.SetApiKey(ApiKeyTextBox.Text);
        }

        if (TargetLangComboBox.SelectedItem is ComboBoxItem langItem && langItem.Tag is string lang)
        {
            Interop.AuraCoreBinding.SetTargetLang(lang);
        }

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
