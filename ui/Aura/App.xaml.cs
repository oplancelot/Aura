using System;
using System.Windows;
using Velopack;

namespace Aura;

public partial class App : Application
{
    private OverlayRenderer.TranslationOverlay? _overlay;
    private WindowManager.TrayIconManager? _trayManager;

    protected override void OnStartup(StartupEventArgs e)
    {
        // Velopack hooks (install, update, uninstall) — must run first
        VelopackApp.Build()
            .SetAutoApplyOnStartup(true)
            .Run();

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

        // 2. Set model paths (co-located with the DLL in the bin directory)
        var modelPath = System.IO.Path.Combine(
            AppDomain.CurrentDomain.BaseDirectory,
            "silero_vad.onnx");
        Interop.AuraCoreBinding.SetModelPath(modelPath);

        var asrModelPath = System.IO.Path.Combine(
            AppDomain.CurrentDomain.BaseDirectory,
            "sense-voice-small-q4_k.gguf");
        Interop.AuraCoreBinding.SetAsrModelPath(asrModelPath);

        // 3. Start the overlay renderer (transparent OSD window)
        _overlay = new OverlayRenderer.TranslationOverlay();
        _overlay.Start();

        // 4. Register the translation callback
        Interop.AuraCoreBinding.RegisterCallback(_overlay.OnTranslationReceived);

        // 5. Set up system tray icon
        _trayManager = new WindowManager.TrayIconManager();
        _trayManager.Initialize();
    }

    protected override void OnExit(ExitEventArgs e)
    {
        // Gracefully shut down the Rust core
        Interop.AuraCoreBinding.Stop();
        Interop.AuraCoreBinding.Destroy();
        _trayManager?.Dispose();
        _overlay?.Dispose();
        base.OnExit(e);
    }
}
