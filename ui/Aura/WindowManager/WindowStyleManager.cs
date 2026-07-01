using System;

namespace Aura.WindowManager;

/// <summary>
/// Manages the WS_EX_TRANSPARENT flag on the overlay window to switch
/// between Combat Mode (click-through) and Configuration Mode (draggable).
///
/// Combat Mode:      WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST → ghost window
/// Configuration Mode: WS_EX_LAYERED |                    WS_EX_TOPMOST → solid, draggable
/// </summary>
public static class WindowStyleManager
{
    /// <summary>
    /// Enable click-through (Combat Mode).
    /// Adds WS_EX_TRANSPARENT to the extended window style.
    /// </summary>
    public static void EnableClickThrough(IntPtr hwnd)
    {
        var currentStyle = (uint)(long)Interop.NativeMethods.GetWindowLongPtr(hwnd, Interop.NativeMethods.GWL_EXSTYLE);

        // Ensure LAYERED is present (required for TRANSPARENT to work)
        currentStyle |= Interop.NativeMethods.WS_EX_LAYERED;
        currentStyle |= Interop.NativeMethods.WS_EX_TRANSPARENT;

        Interop.NativeMethods.SetWindowLongPtr(hwnd, Interop.NativeMethods.GWL_EXSTYLE, (IntPtr)currentStyle);
    }

    /// <summary>
    /// Disable click-through (Configuration Mode).
    /// Removes WS_EX_TRANSPARENT from the extended window style.
    /// The window becomes solid and can receive mouse input for dragging.
    /// </summary>
    public static void DisableClickThrough(IntPtr hwnd)
    {
        var currentStyle = (uint)(long)Interop.NativeMethods.GetWindowLongPtr(hwnd, Interop.NativeMethods.GWL_EXSTYLE);

        // Clear the TRANSPARENT flag but keep LAYERED
        currentStyle &= ~Interop.NativeMethods.WS_EX_TRANSPARENT;

        Interop.NativeMethods.SetWindowLongPtr(hwnd, Interop.NativeMethods.GWL_EXSTYLE, (IntPtr)currentStyle);
    }

    /// <summary>
    /// Check if the window currently has click-through enabled.
    /// </summary>
    public static bool IsClickThrough(IntPtr hwnd)
    {
        var style = (uint)(long)Interop.NativeMethods.GetWindowLongPtr(hwnd, Interop.NativeMethods.GWL_EXSTYLE);
        return (style & Interop.NativeMethods.WS_EX_TRANSPARENT) != 0;
    }
}
