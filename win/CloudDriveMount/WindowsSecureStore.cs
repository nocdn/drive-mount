using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace CloudDriveMount;

public sealed class B2StoredCredentials
{
    public string ApplicationKeyId { get; set; } = string.Empty;
    public string ApplicationKey { get; set; } = string.Empty;
}

public static class WindowsSecureStore
{
    private static readonly byte[] Entropy = Encoding.UTF8.GetBytes("CloudDriveMount.WindowsSecureStore.v1");

    private static readonly string StoreFolder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "CloudDriveMount",
        "credentials");

    public static void SaveB2Credentials(string applicationKeyId, string applicationKey)
    {
        if (string.IsNullOrWhiteSpace(applicationKeyId) && string.IsNullOrWhiteSpace(applicationKey))
        {
            DeleteSecret("b2");
            return;
        }

        var credentials = new B2StoredCredentials
        {
            ApplicationKeyId = applicationKeyId,
            ApplicationKey = applicationKey
        };

        SaveSecret("b2", JsonSerializer.Serialize(credentials));
    }

    public static B2StoredCredentials? LoadB2Credentials()
    {
        var json = LoadSecret("b2");
        if (string.IsNullOrWhiteSpace(json))
            return null;

        try
        {
            return JsonSerializer.Deserialize<B2StoredCredentials>(json);
        }
        catch
        {
            return null;
        }
    }

    public static void SaveSeedboxPassword(string password)
    {
        if (string.IsNullOrEmpty(password))
            return;

        SaveSecret("seedbox-password", password);
    }

    public static string LoadSeedboxPassword()
    {
        return LoadSecret("seedbox-password") ?? string.Empty;
    }

    public static void DeleteSeedboxPassword()
    {
        DeleteSecret("seedbox-password");
    }

    public static byte[] ProtectBytes(byte[] data)
    {
        return ProtectedData.Protect(data, Entropy, DataProtectionScope.CurrentUser);
    }

    public static byte[] UnprotectBytes(byte[] data)
    {
        return ProtectedData.Unprotect(data, Entropy, DataProtectionScope.CurrentUser);
    }

    private static void SaveSecret(string name, string value)
    {
        Directory.CreateDirectory(StoreFolder);
        var bytes = Encoding.UTF8.GetBytes(value);
        File.WriteAllBytes(GetSecretPath(name), ProtectBytes(bytes));
    }

    private static string? LoadSecret(string name)
    {
        var path = GetSecretPath(name);
        if (!File.Exists(path))
            return null;

        try
        {
            return Encoding.UTF8.GetString(UnprotectBytes(File.ReadAllBytes(path)));
        }
        catch
        {
            return null;
        }
    }

    private static void DeleteSecret(string name)
    {
        var path = GetSecretPath(name);
        if (File.Exists(path))
            File.Delete(path);
    }

    private static string GetSecretPath(string name)
    {
        return Path.Combine(StoreFolder, name + ".bin");
    }
}
