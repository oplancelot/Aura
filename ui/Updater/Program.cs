using System.Diagnostics;
using System.Security.Cryptography;
using System.Text.Json;

var cmdArgs = Environment.GetCommandLineArgs();

string? appDir = null;
string? updateDir = null;
int? parentPid = null;

for (int i = 1; i < cmdArgs.Length; i++)
{
    switch (cmdArgs[i])
    {
        case "--app-dir" when i + 1 < args.Length: appDir = args[++i]; break;
        case "--update-dir" when i + 1 < args.Length: updateDir = args[++i]; break;
        case "--pid" when i + 1 < args.Length: parentPid = int.Parse(args[++i]); break;
    }
}

if (appDir == null || updateDir == null || parentPid == null)
{
    Console.Error.WriteLine("Usage: Updater --app-dir <path> --update-dir <path> --pid <parent-pid>");
    Environment.Exit(1);
    return;
}

// Wait for parent process to exit
try
{
    var parent = Process.GetProcessById(parentPid.Value);
    parent.WaitForExit();
}
catch
{
    // Process already exited
}

Thread.Sleep(500);

// Load update manifest
var manifestPath = Path.Combine(updateDir, "update.json");
if (!File.Exists(manifestPath))
{
    Console.Error.WriteLine("update.json not found");
    Environment.Exit(2);
    return;
}

var manifest = JsonSerializer.Deserialize<UpdateManifest>(File.ReadAllText(manifestPath));
if (manifest == null)
{
    Console.Error.WriteLine("Invalid update.json");
    Environment.Exit(2);
    return;
}

// Copy files, verify hashes
foreach (var entry in manifest.Files)
{
    var src = Path.Combine(updateDir, entry.Path);
    var dst = Path.Combine(appDir, entry.Path);

    if (!File.Exists(src))
    {
        Console.Error.WriteLine($"Missing: {entry.Path}");
        continue;
    }

    var hash = SHA256.HashData(await File.ReadAllBytesAsync(src));
    var hashStr = Convert.ToHexString(hash).ToLowerInvariant();
    if (hashStr != entry.Sha256)
    {
        Console.Error.WriteLine($"Hash mismatch: {entry.Path}");
        continue;
    }

    var dir = Path.GetDirectoryName(dst);
    if (dir != null) Directory.CreateDirectory(dir);
    File.Copy(src, dst, overwrite: true);
    Console.WriteLine($"Updated: {entry.Path}");
}

// Launch main app
var mainExe = Path.Combine(appDir, "Aura.exe");
if (File.Exists(mainExe))
{
    Process.Start(new ProcessStartInfo(mainExe) { WorkingDirectory = appDir });
}

record UpdateManifest(string Version, List<FileEntry> Files);
record FileEntry(string Path, string Sha256);
