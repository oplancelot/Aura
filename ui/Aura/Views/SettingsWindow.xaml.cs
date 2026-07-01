using System;
using System.Diagnostics;
using System.Linq;
using System.Windows;
using System.Windows.Controls;

namespace Aura.Views;

/// <summary>
/// Settings window code-behind.
/// Handles user configuration and pipeline lifecycle.
/// </summary>
public partial class SettingsWindow : Window
{
    public SettingsWindow()
    {
        InitializeComponent();
        OnRefreshProcesses(this, new RoutedEventArgs());
    }

    /// <summary>
    /// Refresh the list of running processes that could be voice applications.
    /// </summary>
    private void OnRefreshProcesses(object sender, RoutedEventArgs e)
    {
        ProcessComboBox.Items.Clear();

        // Find common voice application processes
        var voiceApps = new[] { "Discord", "TeamSpeak", "ts3client", "Zoom", "Skype" };

        var processes = Process.GetProcesses()
            .Where(p => voiceApps.Any(app =>
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

        if (ProcessComboBox.Items.Count > 0)
        {
            ProcessComboBox.SelectedIndex = 0;
        }
    }

    /// <summary>
    /// Start the translation pipeline.
    /// </summary>
    private void OnStartClick(object sender, RoutedEventArgs e)
    {
        if (ProcessComboBox.SelectedItem is not ComboBoxItem selectedProcess)
        {
            MessageBox.Show("Please select a target voice application.",
                "Aura", MessageBoxButton.OK, MessageBoxImage.Warning);
            return;
        }

        var pid = (int)selectedProcess.Tag;

        // Configure the engine
        var engineIndex = EngineComboBox.SelectedIndex;
        var engineName = engineIndex == 0 ? "gemini" : "sensevoice";
        Interop.AuraCoreBinding.SetEngine(engineName);

        // Set API key
        if (!string.IsNullOrWhiteSpace(ApiKeyTextBox.Text))
        {
            Interop.AuraCoreBinding.SetApiKey(ApiKeyTextBox.Text);
        }

        // Set target language
        if (TargetLangComboBox.SelectedItem is ComboBoxItem langItem && langItem.Tag is string lang)
        {
            Interop.AuraCoreBinding.SetTargetLang(lang);
        }

        // Start pipeline
        int result = Interop.AuraCoreBinding.Start((uint)pid);
        if (result == 0)
        {
            StartButton.IsEnabled = false;
            StopButton.IsEnabled = true;

            // Minimise to tray
            this.WindowState = WindowState.Minimized;
        }
        else
        {
            MessageBox.Show("Failed to start translation pipeline.",
                "Aura Error", MessageBoxButton.OK, MessageBoxImage.Error);
        }
    }

    /// <summary>
    /// Stop the translation pipeline.
    /// </summary>
    private void OnStopClick(object sender, RoutedEventArgs e)
    {
        Interop.AuraCoreBinding.Stop();
        StartButton.IsEnabled = true;
        StopButton.IsEnabled = false;
    }
}
