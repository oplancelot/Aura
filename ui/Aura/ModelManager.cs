using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Net.Http;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text.Json;
using System.Windows;

namespace Aura;

public class ModelInfo
{
    public string Version { get; set; } = "1.0";
    public string Model { get; set; } = "";
    public string Source { get; set; } = "";
    public string Sha256 { get; set; } = "";
    public DateTime UpdatedAt { get; set; }
    public string UpdateUrl { get; set; } = "";
}

public class ModelManager : INotifyPropertyChanged
{
    private static readonly string ModelsDir = AppDomain.CurrentDomain.BaseDirectory;
    private static readonly string ManifestPath = Path.Combine(ModelsDir, "model_manifest.json");

    private static readonly HttpClient HttpClient = new()
    {
        Timeout = TimeSpan.FromMinutes(30)
    };

    private string _statusMessage = "";
    public string StatusMessage
    {
        get => _statusMessage;
        set { _statusMessage = value; OnPropertyChanged(); }
    }

    private double _progress;
    public double Progress
    {
        get => _progress;
        set { _progress = value; OnPropertyChanged(); }
    }

    private bool _isBusy;
    public bool IsBusy
    {
        get => _isBusy;
        set { _isBusy = value; OnPropertyChanged(); }
    }

    private bool _isInstalled;
    public bool IsInstalled
    {
        get => _isInstalled;
        set { _isInstalled = value; OnPropertyChanged(); }
    }

    public bool CanDownload => !IsBusy && !IsInstalled;

    public string ModelFileName { get; }
    public string ModelFilePath { get; }

    public ModelManager(string modelFileName)
    {
        ModelFileName = modelFileName;
        ModelFilePath = Path.Combine(ModelsDir, modelFileName);
        RefreshStatus();
    }

    public void RefreshStatus()
    {
        if (File.Exists(ModelFilePath))
        {
            var manifest = LoadManifest();
            IsInstalled = true;
            StatusMessage = manifest != null
                ? $"Installed (v{manifest.Version}, {FormatBytes(new FileInfo(ModelFilePath).Length)})"
                : $"Installed ({FormatBytes(new FileInfo(ModelFilePath).Length)})";
        }
        else
        {
            IsInstalled = false;
            StatusMessage = "Not installed";
        }
        OnPropertyChanged(nameof(CanDownload));
    }

    public async Task DownloadAsync(string sourceUrl, string expectedSha256 = "",
        IProgress<double>? progress = null)
    {
        IsBusy = true;
        Progress = 0;
        StatusMessage = "Starting download...";

        try
        {
            using var response = await HttpClient.GetAsync(sourceUrl,
                HttpCompletionOption.ResponseHeadersRead);

            response.EnsureSuccessStatusCode();

            var totalBytes = response.Content.Headers.ContentLength ?? -1;
            using var stream = await response.Content.ReadAsStreamAsync();
            using var fileStream = File.Create(ModelFilePath + ".tmp");
            using var sha256 = SHA256.Create();

            var buffer = new byte[81920];
            long bytesReadTotal = 0;
            int bytesRead;

            while ((bytesRead = await stream.ReadAsync(buffer)) > 0)
            {
                await fileStream.WriteAsync(buffer, 0, bytesRead);
                sha256.TransformBlock(buffer, 0, bytesRead, null, 0);
                bytesReadTotal += bytesRead;

                if (totalBytes > 0)
                {
                    var pct = (double)bytesReadTotal / totalBytes * 100;
                    Progress = Math.Round(pct, 1);
                    progress?.Report(Progress);
                    StatusMessage = $"Downloading... {FormatBytes(bytesReadTotal)} / {FormatBytes(totalBytes)} ({Progress:F0}%)";
                }
                else
                {
                    StatusMessage = $"Downloading... {FormatBytes(bytesReadTotal)}";
                }
            }

            sha256.TransformFinalBlock([], 0, 0);
            var hash = sha256.Hash;
            var hashStr = BitConverter.ToString(hash!).Replace("-", "").ToLowerInvariant();

            if (!string.IsNullOrEmpty(expectedSha256) &&
                !string.Equals(hashStr, expectedSha256, StringComparison.OrdinalIgnoreCase))
            {
                File.Delete(ModelFilePath + ".tmp");
                StatusMessage = "SHA256 mismatch — download may be corrupted";
                IsBusy = false;
                return;
            }

            File.Move(ModelFilePath + ".tmp", ModelFilePath, overwrite: true);

            var manifest = new ModelInfo
            {
                Version = "1.0",
                Model = ModelFileName,
                Source = sourceUrl,
                Sha256 = hashStr,
                UpdatedAt = DateTime.UtcNow
            };
            SaveManifest(manifest);

            Progress = 100;
            IsInstalled = true;
            StatusMessage = $"Installed successfully ({FormatBytes(new FileInfo(ModelFilePath).Length)})";
            OnPropertyChanged(nameof(CanDownload));
        }
        catch (Exception ex)
        {
            StatusMessage = $"Download failed: {ex.Message}";
            if (File.Exists(ModelFilePath + ".tmp"))
                File.Delete(ModelFilePath + ".tmp");
        }
        finally
        {
            IsBusy = false;
        }
    }

    public static ModelInfo? LoadManifest()
    {
        try
        {
            if (File.Exists(ManifestPath))
                return JsonSerializer.Deserialize<ModelInfo>(File.ReadAllText(ManifestPath));
        }
        catch { }
        return null;
    }

    public static void SaveManifest(ModelInfo info)
    {
        File.WriteAllText(ManifestPath, JsonSerializer.Serialize(info, new JsonSerializerOptions
        {
            WriteIndented = true
        }));
    }

    private static string FormatBytes(long bytes) => bytes switch
    {
        < 1024 => $"{bytes} B",
        < 1024 * 1024 => $"{bytes / 1024.0:F1} KB",
        < 1024 * 1024 * 1024 => $"{bytes / (1024.0 * 1024):F1} MB",
        _ => $"{bytes / (1024.0 * 1024 * 1024):F2} GB"
    };

    public event PropertyChangedEventHandler? PropertyChanged;
    protected void OnPropertyChanged([CallerMemberName] string? name = null)
        => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
}
