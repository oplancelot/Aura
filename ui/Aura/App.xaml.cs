using System;
using System.IO;
using System.Net.Http;
using System.Threading.Tasks;
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

        // 1. Auto-download ASR model if missing
        var baseDir = AppDomain.CurrentDomain.BaseDirectory;
        var ggufPath = Path.Combine(baseDir, "sense-voice-small-q4_k.gguf");
        if (!File.Exists(ggufPath))
        {
            _ = DownloadModelAsync(
                "https://github.com/oplancelot/Aura/raw/main/assets/sense-voice-small-q4_k.gguf",
                ggufPath);
        }

        // 2. Initialise the Rust core
        int result = Interop.AuraCoreBinding.Init();
        if (result != 0)
        {
            MessageBox.Show("Failed to initialise Aura core engine.", "Aura Error",
                MessageBoxButton.OK, MessageBoxImage.Error);
            Shutdown(1);
            return;
        }

        // 3. Set model paths
        var modelPath = Path.Combine(baseDir, "silero_vad.onnx");
        Interop.AuraCoreBinding.SetModelPath(modelPath);

        var asrModelPath = Path.Combine(baseDir, "sense-voice-small-q4_k.gguf");
        Interop.AuraCoreBinding.SetAsrModelPath(asrModelPath);

        // 4. Start the overlay renderer
        _overlay = new OverlayRenderer.TranslationOverlay();
        _overlay.Start();

        // 5. Register the translation callback
        Interop.AuraCoreBinding.RegisterCallback(_overlay.OnTranslationReceived);

        // 6. Set up system tray icon
        _trayManager = new WindowManager.TrayIconManager();
        _trayManager.Initialize();
    }

    private static async Task DownloadModelAsync(string url, string destPath)
    {
        try
        {
            using var client = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            var response = await client.GetAsync(url);
            response.EnsureSuccessStatusCode();
            Directory.CreateDirectory(Path.GetDirectoryName(destPath)!);
            using var fs = new FileStream(destPath, FileMode.Create, FileAccess.Write);
            await response.Content.CopyToAsync(fs);
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Failed to download model:\n{ex.Message}",
                "Aura", MessageBoxButton.OK, MessageBoxImage.Warning);
        }
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
