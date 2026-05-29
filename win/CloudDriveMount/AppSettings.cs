namespace CloudDriveMount;

public static class CloudProvider
{
    public const string BackblazeB2 = "B2";
    public const string GoogleDrive = "GoogleDrive";
    public const string Seedbox = "Seedbox";
    public const string DefaultGoogleDriveRemoteName = "gdrive";
    public const string DefaultGoogleDriveLetter = "G";
    public const string DefaultSeedboxRemoteName = "seedbox";
    public const string DefaultSeedboxLetter = "S";

    public static string Normalize(string? provider)
    {
        if (string.Equals(provider, GoogleDrive, StringComparison.OrdinalIgnoreCase))
            return GoogleDrive;

        if (string.Equals(provider, Seedbox, StringComparison.OrdinalIgnoreCase))
            return Seedbox;

        return BackblazeB2;
    }

    public static bool IsReservedGoogleDriveLetter(string? driveLetter)
    {
        return string.Equals(
            NormalizeDriveLetterInput(driveLetter),
            DefaultGoogleDriveLetter,
            StringComparison.OrdinalIgnoreCase);
    }

    public static string NormalizeDriveLetterInput(string? driveLetter)
    {
        var drive = (driveLetter ?? string.Empty).Trim().ToUpperInvariant();

        if (drive.EndsWith(":/") || drive.EndsWith(":\\"))
            drive = drive[..^2];
        else if (drive.EndsWith(":"))
            drive = drive[..^1];

        return drive;
    }

    public static string NormalizeSeedboxHost(string? host)
    {
        var normalized = (host ?? string.Empty).Trim();
        foreach (var prefix in new[] { "https://", "http://", "ftps://", "ftp://" })
        {
            if (!normalized.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
                continue;

            normalized = normalized[prefix.Length..].Trim();
            break;
        }

        while (normalized.EndsWith('/'))
            normalized = normalized[..^1].TrimEnd();

        return normalized.Trim();
    }
}

public class BucketMount
{
    public string BucketName { get; set; } = string.Empty;
    public string DriveLetter { get; set; } = string.Empty;
}

public class GoogleDriveSettings
{
    public string RemoteName { get; set; } = CloudProvider.DefaultGoogleDriveRemoteName;
    public string RemotePath { get; set; } = string.Empty;
    public string RootFolderId { get; set; } = string.Empty;
    public string DriveLetter { get; set; } = CloudProvider.DefaultGoogleDriveLetter;
}

public class SeedboxSettings
{
    public string RemoteName { get; set; } = CloudProvider.DefaultSeedboxRemoteName;
    public string Host { get; set; } = string.Empty;
    public string Username { get; set; } = string.Empty;
    public int Port { get; set; } = 21;
    public string RemotePath { get; set; } = "downloads";
    public string DriveLetter { get; set; } = CloudProvider.DefaultSeedboxLetter;
    public bool AllowUnverifiedCertificate { get; set; } = true;
    public bool ReadOnly { get; set; } = true;
}

public class AppSettings
{
    public string SelectedProvider { get; set; } = CloudProvider.BackblazeB2;

    public string ApplicationKeyId { get; set; } = string.Empty;
    public string ApplicationKey { get; set; } = string.Empty;
    public List<BucketMount> Buckets { get; set; } = new();

    public GoogleDriveSettings GoogleDrive { get; set; } = new();
    public SeedboxSettings Seedbox { get; set; } = new();

    public bool StartOnLogin { get; set; } = true;
    public bool StartMinimized { get; set; } = true;
}
