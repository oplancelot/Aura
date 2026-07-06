using System.Diagnostics;
using System.Reflection;
using System.Windows;
using Velopack;

namespace Aura;

public class UpdateCheckResult
{
    public bool HasUpdate { get; set; }
    public UpdateInfo? Info { get; set; }
    public string CurrentVersion { get; set; } = "";
    public string? ErrorMessage { get; set; }
}

public static class UpdateChecker
{
    private static readonly Velopack.Sources.GithubSource UpdateSource = new(
        "https://github.com/oplancelot/Aura", "", false);

    public static string CurrentVersion =>
        Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "0.0.0";

    public static UpdateManager CreateManager()
        => new(UpdateSource);

    public static async Task<UpdateCheckResult> CheckAsync()
    {
        var result = new UpdateCheckResult
        {
            CurrentVersion = CurrentVersion
        };

        try
        {
            var mgr = CreateManager();
            result.Info = await mgr.CheckForUpdatesAsync();

            if (result.Info != null)
                result.HasUpdate = true;
        }
        catch (Exception ex)
        {
            result.ErrorMessage = ex.Message;
        }

        return result;
    }

    public static async Task DownloadUpdateAsync(UpdateInfo info, Action<int>? progress = null)
    {
        var mgr = CreateManager();
        await mgr.DownloadUpdatesAsync(info, progress);
    }

    public static void ApplyAndRestart(UpdateInfo info)
    {
        var mgr = CreateManager();
        mgr.ApplyUpdatesAndRestart(info);
    }
}
