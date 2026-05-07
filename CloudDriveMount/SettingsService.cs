using System.IO;
using System.Text.Json;

namespace CloudDriveMount;

public class SettingsService
{
    private static readonly string AppDataFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "CloudDriveMount");

    private static readonly string OldAppDataFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "B2DriveMount");

    private static readonly string SettingsPath = Path.Combine(AppDataFolder, "settings.json");
    private static readonly string OldSettingsPath = Path.Combine(OldAppDataFolder, "settings.json");

    public AppSettings Load()
    {
        var settingsPath = File.Exists(SettingsPath) ? SettingsPath : OldSettingsPath;
        if (!File.Exists(settingsPath))
            return new AppSettings();

        try
        {
            var json = File.ReadAllText(settingsPath);
            var settings = JsonSerializer.Deserialize<AppSettings>(json) ?? new AppSettings();
            settings.Buckets ??= new List<BucketMount>();

            // Migrate the earlier single-bucket settings format if present.
            if (settings.Buckets.Count == 0)
            {
                using var document = JsonDocument.Parse(json);
                var root = document.RootElement;
                var legacyBucket = root.TryGetProperty("BucketName", out var bucketElement) ? bucketElement.GetString() : string.Empty;
                var legacyDrive = root.TryGetProperty("DriveLetter", out var driveElement) ? driveElement.GetString() : string.Empty;
                if (!string.IsNullOrWhiteSpace(legacyBucket) && !string.IsNullOrWhiteSpace(legacyDrive))
                {
                    settings.Buckets.Add(new BucketMount
                    {
                        BucketName = legacyBucket,
                        DriveLetter = legacyDrive
                    });
                }
            }

            return settings;
        }
        catch
        {
            return new AppSettings();
        }
    }

    public void Save(AppSettings settings)
    {
        Directory.CreateDirectory(AppDataFolder);
        var json = JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(SettingsPath, json);
    }
}
