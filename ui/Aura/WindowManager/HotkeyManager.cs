using System;
using System.Runtime.InteropServices;
using System.Windows.Interop;

namespace Aura.WindowManager;

/// <summary>
/// Manages the global hotkey (Ctrl+Shift+L) for toggling between
/// Combat Mode (click-through) and Configuration Mode (draggable).
/// </summary>
public class HotkeyManager : IDisposable
{
    private const int HOTKEY_ID = 0xAURA;
    private IntPtr _hwnd;
    private HwndSource? _source;

    /// <summary>Fired when the toggle hotkey is pressed.</summary>
    public event Action? OnToggleHotkey;

    /// <summary>
    /// Register the global hotkey. Call after the WPF window is loaded.
    /// </summary>
    public void Register(IntPtr windowHandle)
    {
        _hwnd = windowHandle;

        // Ctrl + Shift + L
        bool success = Interop.NativeMethods.RegisterHotKey(
            _hwnd,
            HOTKEY_ID,
            Interop.NativeMethods.MOD_CONTROL | Interop.NativeMethods.MOD_SHIFT | Interop.NativeMethods.MOD_NOREPEAT,
            Interop.NativeMethods.VK_L
        );

        if (!success)
        {
            throw new InvalidOperationException(
                "Failed to register global hotkey Ctrl+Shift+L. " +
                "Another application may have claimed it.");
        }

        // Hook into the Win32 message loop
        _source = HwndSource.FromHwnd(_hwnd);
        _source?.AddHook(WndProc);
    }

    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (msg == Interop.NativeMethods.WM_HOTKEY && wParam.ToInt32() == HOTKEY_ID)
        {
            OnToggleHotkey?.Invoke();
            handled = true;
        }
        return IntPtr.Zero;
    }

    public void Dispose()
    {
        _source?.RemoveHook(WndProc);
        Interop.NativeMethods.UnregisterHotKey(_hwnd, HOTKEY_ID);
    }
}
