using System;

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
    private GameOverlay.Windows.GraphicsWindow? _window;
    private readonly SubtitleQueue _subtitleQueue;
    private readonly TextRenderer _textRenderer;
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
        // TODO: Phase 4 implementation
        // 1. Create GraphicsWindow with transparent background
        // 2. Set up rendering loop (SetupGraphics, DrawGraphics, DestroyGraphics)
        // 3. Position at bottom-center of primary screen
        // 4. Apply WS_EX_TRANSPARENT for click-through
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

        if (_isClickThrough)
        {
            WindowManager.WindowStyleManager.EnableClickThrough(_window.Handle);
        }
        else
        {
            WindowManager.WindowStyleManager.DisableClickThrough(_window.Handle);
        }
    }

    public void Dispose()
    {
        _window?.Dispose();
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
