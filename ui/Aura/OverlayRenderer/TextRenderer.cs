using System;

namespace Aura.OverlayRenderer;

/// <summary>
/// Direct2D text rendering utilities for subtitle display.
/// Handles font management, anti-aliasing, shadow/outline effects,
/// and fade-in/fade-out animations.
/// </summary>
public class TextRenderer
{
    // TODO: Phase 4 implementation
    // - GameOverlay.Drawing.Graphics for Direct2D rendering
    // - Font: "Segoe UI Semibold", 18pt (configurable)
    // - Text colour: White with 2px dark outline for contrast
    // - Background: Semi-transparent dark panel (optional, configurable)
    // - Provisional text: slightly dimmed / italic
    // - Final text: full opacity, brief highlight flash

    /// <summary>
    /// Render a subtitle entry at the specified position.
    /// </summary>
    public void DrawSubtitle(/* Graphics gfx, */ SubtitleEntry entry, float x, float y, float maxWidth)
    {
        // TODO: Implement Direct2D text drawing with:
        // 1. Text outline/shadow for readability on any background
        // 2. Provisional entries rendered with lower opacity
        // 3. Smooth fade-in animation over ~150ms
    }
}
