using System;
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
    public void OnTranslationReceived(string text, int isProvisional, int latencyMs)
    {
        var entry = new SubtitleEntry
        {
            Text = text,
            IsProvisional = isProvisional != 0,
            LatencyMs = latencyMs,
            Timestamp = DateTime.UtcNow
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
    }

    private void OnDragRequested(object sender, MouseButtonEventArgs e)
    {
        if (_window == null || _isClickThrough || e.ButtonState != MouseButtonState.Pressed)
        {
            return;
        }

        _window.DragMove();
    }

    private void RenderSubtitles()
    {
        if (_subtitlePanel == null) return;

        var entries = _subtitleQueue.Update();
        _subtitlePanel.Children.Clear();

        foreach (var entry in entries.TakeLast(_subtitleQueue.MaxVisibleLines))
        {
            var text = new TextBlock
            {
                Text = entry.Text,
                Foreground = entry.IsProvisional
                    ? new SolidColorBrush(Color.FromArgb(210, 220, 226, 240))
                    : Brushes.White,
                FontFamily = new FontFamily("Segoe UI Semibold"),
                FontSize = 28,
                TextAlignment = TextAlignment.Center,
                TextWrapping = TextWrapping.Wrap,
                MaxWidth = Math.Min(980, SystemParameters.PrimaryScreenWidth - 96),
                Effect = new System.Windows.Media.Effects.DropShadowEffect
                {
                    Color = Colors.Black,
                    BlurRadius = 8,
                    ShadowDepth = 2,
                    Opacity = 0.9
                }
            };

            var border = new Border
            {
                Background = new SolidColorBrush(Color.FromArgb(150, 16, 18, 24)),
                CornerRadius = new CornerRadius(6),
                Padding = new Thickness(14, 8, 14, 9),
                Margin = new Thickness(0, 4, 0, 4),
                Child = text
            };

            _subtitlePanel.Children.Add(border);
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
    public DateTime Timestamp { get; set; }
}
