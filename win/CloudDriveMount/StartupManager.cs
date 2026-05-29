using Microsoft.Win32;

namespace CloudDriveMount;

public static class StartupManager
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string AppName = "CloudDriveMount";

    public static bool IsSet()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, false);
        return key?.GetValue(AppName) is not null;
    }

    public static void Set(string exePath)
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, true)
            ?? Registry.CurrentUser.CreateSubKey(RunKey);
        key.SetValue(AppName, $"\"{exePath}\"");
    }

    public static void Unset()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKey, true);
        if (key?.GetValue(AppName) is not null)
            key.DeleteValue(AppName, false);
    }
}
