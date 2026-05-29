using System.IO;
using System.Text.Json;

namespace CloudDriveMount;

public class SettingsService
{
    private static readonly string AppDataFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "CloudDriveMount");

    private static readonly string SettingsPath = Path.Combine(AppDataFolder, "settings.json");

    public AppSettings Load()
    {
        if (!File.Exists(SettingsPath))
            return new AppSettings();

        try
        {
            var json = File.ReadAllText(SettingsPath);
            var settings = JsonSerializer.Deserialize<AppSettings>(json) ?? new AppSettings();
            settings.Buckets ??= new List<BucketMount>();
            settings.GoogleDrive ??= new GoogleDriveSettings();
            settings.SelectedProvider = CloudProvider.Normalize(settings.SelectedProvider);

            if (string.IsNullOrWhiteSpace(settings.GoogleDrive.RemoteName))
                settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
            settings.GoogleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;

            return settings;
        }
        catch
        {
            return new AppSettings();
        }
    }

    public void Save(AppSettings settings)
    {
        settings.Buckets ??= new List<BucketMount>();
        settings.GoogleDrive ??= new GoogleDriveSettings();
        settings.SelectedProvider = CloudProvider.Normalize(settings.SelectedProvider);

        if (string.IsNullOrWhiteSpace(settings.GoogleDrive.RemoteName))
            settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        settings.GoogleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;

        Directory.CreateDirectory(AppDataFolder);
        var json = JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(SettingsPath, json);
    }
}
