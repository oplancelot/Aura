using System.Net.Http;
using System.Reflection;
using System.Text.Json;
using System.Windows;

namespace Aura;

public class ReleaseInfo
{
    public string TagName { get; set; } = "";
    public string HtmlUrl { get; set; } = "";
    public string Body { get; set; } = "";
    public DateTime PublishedAt { get; set; }
    public List<ReleaseAsset> Assets { get; set; } = [];
}

public class ReleaseAsset
{
    public string Name { get; set; } = "";
    public string BrowserDownloadUrl { get; set; } = "";
    public long Size { get; set; }
}

public class UpdateCheckResult
{
    public bool HasUpdate { get; set; }
    public ReleaseInfo? Latest { get; set; }
    public string CurrentVersion { get; set; } = "";
    public string? ErrorMessage { get; set; }
}

public static class UpdateChecker
{
    private static readonly string RepoOwner = "anomalyco";
    private static readonly string RepoName = "aura";
    private static readonly HttpClient Client = new()
    {
        Timeout = TimeSpan.FromSeconds(15)
    };

    public static string CurrentVersion =>
        Assembly.GetExecutingAssembly().GetName().Version?.ToString(3) ?? "0.0.0";

    public static async Task<UpdateCheckResult> CheckAsync()
    {
        var result = new UpdateCheckResult
        {
            CurrentVersion = CurrentVersion
        };

        try
        {
            var url = $"https://api.github.com/repos/{RepoOwner}/{RepoName}/releases/latest";
            using var request = new HttpRequestMessage(HttpMethod.Get, url);
            request.Headers.UserAgent.ParseAdd("Aura/1.0");

            using var response = await Client.SendAsync(request);
            response.EnsureSuccessStatusCode();

            var json = await response.Content.ReadAsStringAsync();
            var release = JsonSerializer.Deserialize<ReleaseInfo>(json, new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            });

            if (release == null || string.IsNullOrEmpty(release.TagName))
                return result;

            result.Latest = release;

            var latestVer = release.TagName.TrimStart('v');
            var currentVer = CurrentVersion;

            if (Version.TryParse(latestVer, out var latest) &&
                Version.TryParse(currentVer, out var current))
            {
                result.HasUpdate = latest > current;
            }
        }
        catch (Exception ex)
        {
            result.ErrorMessage = ex.Message;
        }

        return result;
    }
}
