namespace CloudDriveMount;

public static class CloudProvider
{
    public const string BackblazeB2 = "B2";
    public const string GoogleDrive = "GoogleDrive";
    public const string DefaultGoogleDriveRemoteName = "gdrive";
    public const string DefaultGoogleDriveLetter = "G";

    public static string Normalize(string? provider)
    {
        return string.Equals(provider, GoogleDrive, StringComparison.OrdinalIgnoreCase)
            ? GoogleDrive
            : BackblazeB2;
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

public class AppSettings
{
    public string SelectedProvider { get; set; } = CloudProvider.BackblazeB2;

    public string ApplicationKeyId { get; set; } = string.Empty;
    public string ApplicationKey { get; set; } = string.Empty;
    public List<BucketMount> Buckets { get; set; } = new();

    public GoogleDriveSettings GoogleDrive { get; set; } = new();

    public bool StartOnLogin { get; set; } = true;
    public bool StartMinimized { get; set; } = true;
}
