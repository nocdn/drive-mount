namespace CloudDriveMount;

public class BucketMount
{
    public string BucketName { get; set; } = string.Empty;
    public string DriveLetter { get; set; } = string.Empty;
}

public class AppSettings
{
    public string ApplicationKeyId { get; set; } = string.Empty;
    public string ApplicationKey { get; set; } = string.Empty;
    public List<BucketMount> Buckets { get; set; } = new();
    public bool StartOnLogin { get; set; } = true;
    public bool StartMinimized { get; set; } = true;
}