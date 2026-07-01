using System;
using System.Windows;

namespace Aura;

/// <summary>
/// WPF Application entry point.
/// Initialises the Rust core pipeline, overlay renderer, and system tray.
/// </summary>
public partial class App : Application
{
    protected override void OnStartup(StartupEventArgs e)
    {
        base.OnStartup(e);

        // 1. Initialise the Rust core
        int result = Interop.AuraCoreBinding.Init();
        if (result != 0)
        {
            MessageBox.Show("Failed to initialise Aura core engine.", "Aura Error",
                MessageBoxButton.OK, MessageBoxImage.Error);
            Shutdown(1);
            return;
        }

        // 2. Start the overlay renderer (transparent OSD window)
        var overlay = new OverlayRenderer.TranslationOverlay();
        overlay.Start();

        // 3. Register the translation callback
        Interop.AuraCoreBinding.RegisterCallback(overlay.OnTranslationReceived);

        // 4. Set up system tray icon
        var trayManager = new WindowManager.TrayIconManager();
        trayManager.Initialize();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        // Gracefully shut down the Rust core
        Interop.AuraCoreBinding.Stop();
        Interop.AuraCoreBinding.Destroy();
        base.OnExit(e);
    }
}
