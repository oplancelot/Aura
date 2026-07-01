using System;

namespace Aura.WindowManager;

/// <summary>
/// Manages the system tray icon and its context menu.
/// Provides quick access to settings, start/stop translation, and exit.
/// </summary>
public class TrayIconManager : IDisposable
{
    // Note: WPF doesn't have a built-in tray icon. We use System.Windows.Forms.NotifyIcon
    // via a WindowsFormsHost or direct reference.

    /// <summary>
    /// Initialise the tray icon with context menu.
    /// </summary>
    public void Initialize()
    {
        // TODO: Phase 4 implementation
        // 1. Create NotifyIcon with Aura logo
        // 2. Build context menu:
        //    - "Start Translation" / "Stop Translation"
        //    - "Settings..." → open SettingsWindow
        //    - "Toggle Overlay (Ctrl+Shift+L)"
        //    - Separator
        //    - "Exit"
        // 3. Handle double-click to open settings
    }

    public void Dispose()
    {
        // TODO: Dispose NotifyIcon
    }
}
