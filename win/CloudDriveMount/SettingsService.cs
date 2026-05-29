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
            settings.Seedbox ??= new SeedboxSettings();
            settings.SelectedProvider = CloudProvider.Normalize(settings.SelectedProvider);

            if (string.IsNullOrWhiteSpace(settings.GoogleDrive.RemoteName))
                settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
            settings.GoogleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;
            NormalizeSeedboxSettings(settings.Seedbox);
            MigrateInlineCredentials(settings);

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
        settings.Seedbox ??= new SeedboxSettings();
        settings.SelectedProvider = CloudProvider.Normalize(settings.SelectedProvider);

        if (string.IsNullOrWhiteSpace(settings.GoogleDrive.RemoteName))
            settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        settings.GoogleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;
        NormalizeSeedboxSettings(settings.Seedbox);
        settings.ApplicationKeyId = string.Empty;
        settings.ApplicationKey = string.Empty;

        Directory.CreateDirectory(AppDataFolder);
        var json = JsonSerializer.Serialize(settings, new JsonSerializerOptions { WriteIndented = true });
        File.WriteAllText(SettingsPath, json);
    }

    private static void MigrateInlineCredentials(AppSettings settings)
    {
        if (string.IsNullOrWhiteSpace(settings.ApplicationKeyId) &&
            string.IsNullOrWhiteSpace(settings.ApplicationKey))
        {
            return;
        }

        WindowsSecureStore.SaveB2Credentials(settings.ApplicationKeyId, settings.ApplicationKey);
        settings.ApplicationKeyId = string.Empty;
        settings.ApplicationKey = string.Empty;
    }

    private static void NormalizeSeedboxSettings(SeedboxSettings seedbox)
    {
        seedbox.RemoteName = CloudProvider.DefaultSeedboxRemoteName;
        seedbox.Host = CloudProvider.NormalizeSeedboxHost(seedbox.Host);
        if (seedbox.Port <= 0 || seedbox.Port > 65535)
            seedbox.Port = 21;
        seedbox.RemotePath = NormalizeRemotePath(seedbox.RemotePath);
        seedbox.DriveLetter = string.IsNullOrWhiteSpace(seedbox.DriveLetter)
            ? CloudProvider.DefaultSeedboxLetter
            : CloudProvider.NormalizeDriveLetterInput(seedbox.DriveLetter);
    }

    private static string NormalizeRemotePath(string path)
    {
        var normalized = (path ?? string.Empty).Trim().Replace('\\', '/');
        while (normalized.StartsWith("/"))
            normalized = normalized[1..];
        while (normalized.StartsWith(":"))
            normalized = normalized[1..];

        return normalized;
    }
}
