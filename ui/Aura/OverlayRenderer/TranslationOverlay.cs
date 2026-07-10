using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Interop;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Threading;

namespace Aura.OverlayRenderer;

/// <summary>
/// Hardware-accelerated transparent overlay window using GameOverlay.Net (Direct2D1).
///
/// Creates a WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST window that
/// renders translated subtitles on top of the game without intercepting any
/// mouse events.
/// </summary>
public class TranslationOverlay : IDisposable
{
    private readonly SubtitleQueue _subtitleQueue;
    private readonly TextRenderer _textRenderer;
    private WindowManager.HotkeyManager? _hotkeyManager;
    private Window? _window;
    private Grid? _root;
    private StackPanel? _subtitlePanel;
    private DispatcherTimer? _renderTimer;
    private bool _isClickThrough = true;
    private StreamWriter? _logWriter;
    private StreamWriter? _transcriptWriter;
    private string? _lastTranscriptText;
    private readonly object _logLock = new();
    private DateTime _sessionStart;

    public TranslationOverlay()
    {
        _subtitleQueue = new SubtitleQueue();
        _textRenderer = new TextRenderer();
    }

    /// <summary>
    /// Initialise and show the overlay window.
    /// </summary>
    public void Start()
    {
        if (_window != null) return;

        _sessionStart = DateTime.UtcNow;
        try
        {
            var logDir = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "logs");
            Directory.CreateDirectory(logDir);

            var logPath = Path.Combine(logDir, $"asr_{_sessionStart:yyyyMMdd_HHmmss}.txt");
            _logWriter = new StreamWriter(logPath, append: false) { AutoFlush = true };
            _logWriter.WriteLine($"# Aura ASR log — {_sessionStart:yyyy-MM-dd HH:mm:ss} UTC");
            _logWriter.WriteLine("# [elapsed]\t[type]\t[text]");
            _logWriter.WriteLine("# ---------\t------\t------");

            var transcriptPath = Path.Combine(logDir, $"transcript_{_sessionStart:yyyyMMdd_HHmmss}.txt");
            _transcriptWriter = new StreamWriter(transcriptPath, append: false) { AutoFlush = true };
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed to open log files: {ex.Message}");
        }

        var overlayWidth = Math.Min(1100, SystemParameters.PrimaryScreenWidth - 96);
        var overlayHeight = 260;

        _subtitlePanel = new StackPanel
        {
            Orientation = Orientation.Vertical,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Bottom,
            Margin = new Thickness(12)
        };

        _root = new Grid
        {
            Background = Brushes.Transparent,
            Children = { _subtitlePanel }
        };
        _root.MouseLeftButtonDown += OnDragRequested;

        _window = new Window
        {
            Width = overlayWidth,
            Height = overlayHeight,
            Left = (SystemParameters.PrimaryScreenWidth - overlayWidth) / 2,
            Top = 28,
            WindowStyle = WindowStyle.None,
            AllowsTransparency = true,
            Background = Brushes.Transparent,
            Topmost = true,
            ShowInTaskbar = false,
            ShowActivated = false,
            ResizeMode = ResizeMode.NoResize,
            Content = _root,
            IsHitTestVisible = false
        };

        _window.SourceInitialized += (_, _) =>
        {
            var handle = new WindowInteropHelper(_window).Handle;
            WindowManager.WindowStyleManager.EnableClickThrough(handle);

            _hotkeyManager = new WindowManager.HotkeyManager();
            _hotkeyManager.OnToggleHotkey += ToggleClickThrough;
            _hotkeyManager.Register(handle);
        };

        _window.Show();

        _renderTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromMilliseconds(33)
        };
        _renderTimer.Tick += (_, _) => RenderSubtitles();
        _renderTimer.Start();
    }

    /// <summary>
    /// Callback invoked from the Rust core (via FFI) when a translation is ready.
    /// Thread-safe — will be dispatched to the render thread.
    /// </summary>
    public void OnTranslationReceived(string text, int isProvisional, int latencyMs,
        Interop.AuraCoreBinding.TranslationMetrics metrics)
    {
        var now = DateTime.UtcNow;
        var elapsed = now - _sessionStart;
        var type = isProvisional != 0 ? "P" : "F";

        if (_logWriter != null)
        {
            lock (_logLock)
            {
                _logWriter.WriteLine($"{elapsed.TotalSeconds:F3}\t{type}\t{text}");
            }
        }

        if (isProvisional == 0 && _transcriptWriter != null && text != _lastTranscriptText)
        {
            lock (_logLock)
            {
                _transcriptWriter.WriteLine($"[{now:yyyy-MM-dd HH:mm:ss}] {text}");
                _lastTranscriptText = text;
            }
        }

        var entry = new SubtitleEntry
        {
            Text = text,
            IsProvisional = isProvisional != 0,
            LatencyMs = latencyMs,
            Metrics = metrics,
            Timestamp = now,
        };

        _subtitleQueue.Enqueue(entry);
    }

    /// <summary>
    /// Toggle between Combat Mode (click-through) and Configuration Mode (draggable).
    /// Called when the global hotkey (Ctrl+Shift+L) is pressed.
    /// </summary>
    public void ToggleClickThrough()
    {
        if (_window == null) return;

        _isClickThrough = !_isClickThrough;
        _window.IsHitTestVisible = !_isClickThrough;
        var handle = new WindowInteropHelper(_window).Handle;

        if (_isClickThrough)
        {
            WindowManager.WindowStyleManager.EnableClickThrough(handle);
        }
        else
        {
            WindowManager.WindowStyleManager.DisableClickThrough(handle);
        }
    }

    public void Dispose()
    {
        _renderTimer?.Stop();
        _hotkeyManager?.Dispose();
        _window?.Close();

        if (_logWriter != null)
        {
            lock (_logLock)
            {
                var elapsed = DateTime.UtcNow - _sessionStart;
                _logWriter.WriteLine($"# Session ended at {elapsed.TotalSeconds:F3}s");
                _logWriter.Dispose();
                _logWriter = null;
            }
        }

        if (_transcriptWriter != null)
        {
            lock (_logLock)
            {
                _transcriptWriter.Dispose();
                _transcriptWriter = null;
            }
        }
    }

    private void OnDragRequested(object sender, MouseButtonEventArgs e)
    {
        if (_window == null || _isClickThrough || e.ButtonState != MouseButtonState.Pressed)
        {
            return;
        }

        _window.DragMove();
    }

    private readonly List<Border> _subtitleBorders = new();
    private static readonly FontFamily SubtitleFont = new("Segoe UI Semibold");
    private static readonly SolidColorBrush ProvisionalBrush = new(Color.FromArgb(210, 220, 226, 240));
    private static readonly SolidColorBrush FinalBrush = Brushes.White;
    private static readonly SolidColorBrush BackgroundBrush = new(Color.FromArgb(150, 16, 18, 24));
    private static readonly System.Windows.Media.Effects.DropShadowEffect TextShadow = new()
    {
        Color = Colors.Black,
        BlurRadius = 8,
        ShadowDepth = 2,
        Opacity = 0.9
    };

    private void RenderSubtitles()
    {
        if (_subtitlePanel == null) return;

        var entries = _subtitleQueue.Update();
        var count = Math.Min(entries.Count, _subtitleQueue.MaxVisibleLines);
        var last = count > 0 ? entries.Skip(entries.Count - count).ToList() : entries;

        // Recycle existing borders — grow or shrink the panel as needed
        while (_subtitleBorders.Count > count)
        {
            _subtitlePanel.Children.Remove(_subtitleBorders[^1]);
            _subtitleBorders.RemoveAt(_subtitleBorders.Count - 1);
        }
        while (_subtitleBorders.Count < count)
        {
            var text = new TextBlock
            {
                FontFamily = SubtitleFont,
                FontSize = 28,
                TextAlignment = TextAlignment.Center,
                TextWrapping = TextWrapping.Wrap,
                MaxWidth = Math.Min(980, SystemParameters.PrimaryScreenWidth - 96),
                Effect = TextShadow
            };
            var border = new Border
            {
                Background = BackgroundBrush,
                CornerRadius = new CornerRadius(6),
                Padding = new Thickness(14, 8, 14, 9),
                Margin = new Thickness(0, 4, 0, 4),
                Child = text
            };
            _subtitleBorders.Add(border);
            _subtitlePanel.Children.Add(border);
        }

        // Update text and color on reused elements
        for (int i = 0; i < count; i++)
        {
            var entry = last[i];
            var border = _subtitleBorders[i];
            var textEl = (TextBlock)border.Child;
            textEl.Text = entry.Text;
            textEl.Foreground = entry.IsProvisional ? ProvisionalBrush : FinalBrush;
        }
    }
}

/// <summary>
/// A single subtitle entry in the display queue.
/// </summary>
public class SubtitleEntry
{
    public string Text { get; set; } = string.Empty;
    public bool IsProvisional { get; set; }
    public int LatencyMs { get; set; }
    public Interop.AuraCoreBinding.TranslationMetrics Metrics { get; set; }
    public DateTime Timestamp { get; set; }
}
