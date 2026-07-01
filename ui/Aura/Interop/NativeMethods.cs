using System;
using System.Runtime.InteropServices;

namespace Aura.Interop;

/// <summary>
/// Win32 API declarations via P/Invoke for window management.
/// </summary>
internal static class NativeMethods
{
    // ── Extended Window Styles ──────────────────────────────────────

    public const int GWL_EXSTYLE = -20;

    /// <summary>Layered window (prerequisite for transparency).</summary>
    public const uint WS_EX_LAYERED = 0x00080000;

    /// <summary>Mouse-click passthrough (hit-test transparent).</summary>
    public const uint WS_EX_TRANSPARENT = 0x00000020;

    /// <summary>Always-on-top Z-order.</summary>
    public const uint WS_EX_TOPMOST = 0x00000008;

    // ── Window Style APIs ───────────────────────────────────────────

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr GetWindowLongPtr(IntPtr hWnd, int nIndex);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SetWindowLongPtr(IntPtr hWnd, int nIndex, IntPtr dwNewLong);

    // ── Global Hotkey APIs ──────────────────────────────────────────

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll", SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool UnregisterHotKey(IntPtr hWnd, int id);

    // Modifier keys
    public const uint MOD_CONTROL = 0x0002;
    public const uint MOD_SHIFT = 0x0004;
    public const uint MOD_NOREPEAT = 0x4000;

    // Virtual key codes
    public const uint VK_L = 0x4C;

    // Windows message for hotkey
    public const int WM_HOTKEY = 0x0312;
}
