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

        // 1. Initialise the Rust core
        int result = Interop.AuraCoreBinding.Init();
        if (result != 0)
        {
            MessageBox.Show("Failed to initialise Aura core engine.", "Aura Error",
                MessageBoxButton.OK, MessageBoxImage.Error);
            Shutdown(1);
            return;
        }

        // 2. Set model paths (only if valid — LFS placeholder is ~1KB)
        var baseDir = AppDomain.CurrentDomain.BaseDirectory;

        var vadPath = Path.Combine(baseDir, "silero_vad.onnx");
        if (File.Exists(vadPath))
            Interop.AuraCoreBinding.SetModelPath(vadPath);

        var asrPath = Path.Combine(baseDir, "sense-voice-small-q4_k.gguf");
        if (File.Exists(asrPath) && new FileInfo(asrPath).Length > 1024 * 1024)
            Interop.AuraCoreBinding.SetAsrModelPath(asrPath);

        // 3. Fire background download for missing models
        _ = DownloadIfMissingAsync("silero_vad.onnx",
            "https://github.com/oplancelot/Aura/raw/main/assets/silero_vad.onnx", baseDir);
        _ = DownloadIfMissingAsync("sense-voice-small-q4_k.gguf",
            "https://github.com/oplancelot/Aura/raw/main/assets/sense-voice-small-q4_k.gguf", baseDir,
            onCompleted: p => Interop.AuraCoreBinding.SetAsrModelPath(p));

        // 4. Start the overlay renderer
        _overlay = new OverlayRenderer.TranslationOverlay();
        _overlay.Start();

        // 5. Register the translation callback
        Interop.AuraCoreBinding.RegisterCallback(_overlay.OnTranslationReceived);

        // 6. Set up system tray icon
        _trayManager = new WindowManager.TrayIconManager();
        _trayManager.Initialize();
    }

    private static async Task DownloadIfMissingAsync(string fileName, string url, string baseDir,
        Action<string>? onCompleted = null)
    {
        var destPath = Path.Combine(baseDir, fileName);
        if (File.Exists(destPath) && new FileInfo(destPath).Length > 1024 * 1024)
            return;

        try
        {
            using var client = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
            var response = await client.GetAsync(url);
            response.EnsureSuccessStatusCode();
            using var fs = new FileStream(destPath, FileMode.Create, FileAccess.Write);
            await response.Content.CopyToAsync(fs);
            onCompleted?.Invoke(destPath);
        }
        catch (Exception ex)
        {
            MessageBox.Show($"Failed to download {fileName}:\n{ex.Message}",
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
