using System.Diagnostics;
using System.IO;
using System.Text;
using Microsoft.Win32;

namespace B2DriveMount;

public class RcloneManager : IDisposable
{
    private readonly Dictionary<string, Process> _processes = new(StringComparer.OrdinalIgnoreCase);
    private readonly HashSet<int> _intentionalStops = new();
    private readonly string _configPath;

    public event Action<string>? OnStatusChanged;
    public event Action<string>? OnError;

    public bool IsMounted => _processes.Values.Any(p => !p.HasExited);

    public RcloneManager()
    {
        var appData = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "CloudDriveMount");
        _configPath = Path.Combine(appData, "rclone.conf");
    }

    public string? FindRclone()
    {
        var appDir = Path.GetDirectoryName(Process.GetCurrentProcess().MainModule?.FileName ?? AppContext.BaseDirectory);
        LogService.Debug("FindRclone: appDir=" + appDir);
        if (!string.IsNullOrEmpty(appDir))
        {
            var appDirCandidate = Path.Combine(appDir, "rclone.exe");
            if (File.Exists(appDirCandidate))
            {
                LogService.Info("Found rclone.exe in app directory: " + appDirCandidate);
                return appDirCandidate;
            }
        }

        var pathVar = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
        foreach (var dir in pathVar.Split(';'))
        {
            var candidate = Path.Combine(dir.Trim(), "rclone.exe");
            if (File.Exists(candidate))
            {
                LogService.Info("Found rclone.exe in PATH: " + candidate);
                return candidate;
            }
        }

        var commonPaths = new[]
        {
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "rclone", "rclone.exe"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "rclone", "rclone.exe"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), "rclone", "rclone.exe"),
        };

        foreach (var p in commonPaths)
        {
            if (File.Exists(p))
            {
                LogService.Info("Found rclone.exe in common path: " + p);
                return p;
            }
        }

        LogService.Error("rclone.exe not found anywhere.");
        return null;
    }

    public bool IsWinFspInstalled()
    {
        var commonWinFspPaths = new[]
        {
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), "WinFsp", "bin", "winfsp-x64.dll"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "WinFsp", "bin", "winfsp-x64.dll"),
        };

        foreach (var p in commonWinFspPaths)
        {
            if (File.Exists(p))
            {
                LogService.Info("WinFsp found at: " + p);
                return true;
            }
        }

        try
        {
            using var key = Registry.LocalMachine.OpenSubKey(@"Software\WinFsp");
            if (key is not null)
            {
                LogService.Info("WinFsp registry key found: HKLM\\Software\\WinFsp");
                return true;
            }

            using var key2 = Registry.LocalMachine.OpenSubKey(@"Software\WOW6432Node\WinFsp");
            if (key2 is not null)
            {
                LogService.Info("WinFsp registry key found: HKLM\\Software\\WOW6432Node\\WinFsp");
                return true;
            }
        }
        catch (Exception ex)
        {
            LogService.Debug("WinFsp registry check failed: " + ex.Message);
        }

        LogService.Error("WinFsp not found.");
        return false;
    }

    public void EnsureConfig(AppSettings settings)
    {
        var dir = Path.GetDirectoryName(_configPath);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        var sb = new StringBuilder();
        sb.AppendLine("[b2remote]");
        sb.AppendLine("type = b2");
        sb.AppendLine($"account = {settings.ApplicationKeyId}");
        sb.AppendLine($"key = {settings.ApplicationKey}");
        File.WriteAllText(_configPath, sb.ToString(), Encoding.UTF8);
        LogService.Info("rclone config written to: " + _configPath);
    }

    public bool Mount(AppSettings settings)
    {
        LogService.Info("Mount requested. BucketCount=" + settings.Buckets.Count);

        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            OnError?.Invoke("rclone.exe not found. Please install rclone and ensure it is in your PATH.");
            OnError?.Invoke("Searched: app directory, PATH, LocalAppData\\rclone, Program Files\\rclone.");
            return false;
        }

        OnStatusChanged?.Invoke($"Found rclone.exe at: {rclonePath}");

        if (!IsWinFspInstalled())
        {
            OnError?.Invoke("WinFsp not found. rclone mount requires WinFsp to create virtual drives on Windows.");
            OnError?.Invoke("Download and install from: https://winfsp.dev/rel/");
            return false;
        }

        OnStatusChanged?.Invoke("WinFsp is installed.");

        if (string.IsNullOrWhiteSpace(settings.ApplicationKeyId) || string.IsNullOrWhiteSpace(settings.ApplicationKey))
        {
            LogService.Error("B2 credentials not configured.");
            OnError?.Invoke("Backblaze B2 credentials are not configured.");
            OnError?.Invoke("Enter your Application Key ID and Application Key in Settings, then click Save and Mount All.");
            return false;
        }

        if (settings.Buckets.Count == 0)
        {
            OnError?.Invoke("At least one bucket and drive letter is required.");
            return false;
        }

        EnsureConfig(settings);
        OnStatusChanged?.Invoke($"rclone config written to: {_configPath}");

        Unmount();

        var success = true;
        foreach (var bucket in settings.Buckets)
        {
            success &= MountBucket(rclonePath, bucket);
        }

        return success;
    }

    private bool MountBucket(string rclonePath, BucketMount bucket)
    {
        var bucketName = bucket.BucketName.Trim();
        var driveLetter = bucket.DriveLetter.Trim().ToUpperInvariant();
        if (!driveLetter.EndsWith(":"))
            driveLetter += ":";

        var remotePath = $"b2remote:{bucketName}";
        var shareName = SanitizeShareName(bucketName);
        var volName = $"\\\\CloudMount\\{shareName}";

        var psi = new ProcessStartInfo
        {
            FileName = rclonePath,
            Arguments = $"mount \"{remotePath}\" \"{driveLetter}\" --config \"{_configPath}\" --network-mode --vfs-cache-mode writes --volname \"{volName}\" --links",
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        LogService.Info("Starting rclone mount. Bucket=" + bucketName + " Drive=" + driveLetter + " Command=" + psi.FileName + " " + psi.Arguments);

        try
        {
            var process = new Process { StartInfo = psi, EnableRaisingEvents = true };
            process.Exited += (_, _) => HandleProcessExited(process, bucketName, driveLetter);
            process.OutputDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                {
                    LogService.Debug("[rclone stdout] [" + bucketName + "] " + e.Data);
                    OnStatusChanged?.Invoke("[" + bucketName + "] " + e.Data);
                }
            };
            process.ErrorDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                    HandleStderrLine(e.Data, bucketName);
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            _processes[driveLetter] = process;
            LogService.Info("rclone process started. Bucket=" + bucketName + " Drive=" + driveLetter + " PID=" + process.Id);
            OnStatusChanged?.Invoke($"Mounting bucket {bucketName} to {driveLetter}...");
            return true;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to start rclone for bucket " + bucketName + ": " + ex);
            OnError?.Invoke($"Failed to start rclone for bucket {bucketName}: {ex.Message}");
            return false;
        }
    }

    private void HandleProcessExited(Process process, string bucketName, string driveLetter)
    {
        var code = process.ExitCode;
        if (_intentionalStops.Remove(process.Id))
        {
            LogService.Info("Mount process stopped (unmounted). Bucket=" + bucketName + " Drive=" + driveLetter);
            return;
        }

        if (code == 0)
        {
            LogService.Info("Mount process exited normally. Bucket=" + bucketName + " Drive=" + driveLetter);
            OnStatusChanged?.Invoke($"Mount process exited normally for {bucketName} ({driveLetter}).");
        }
        else
        {
            LogService.Error("Mount process exited with code " + code + ". Bucket=" + bucketName + " Drive=" + driveLetter);
            OnError?.Invoke($"Mount process exited with code {code} for {bucketName} ({driveLetter}).");
        }
    }

    private static string SanitizeShareName(string value)
    {
        var invalid = new[] { '\\', '/', ':', '*', '?', '"', '<', '>', '|' };
        var result = value;
        foreach (var c in invalid)
            result = result.Replace(c, '-');
        return string.IsNullOrWhiteSpace(result) ? "bucket" : result;
    }

    private void HandleStderrLine(string line, string bucketName)
    {
        LogService.Debug("[rclone stderr] [" + bucketName + "] " + line);

        if (line.Length > 20)
        {
            var prefix = line.Substring(0, 20);
            if (prefix[4] == '/' && prefix[7] == '/' && prefix[10] == ' ' &&
                prefix[13] == ':' && prefix[16] == ':' && prefix[19] == ' ')
            {
                var rest = line.Substring(20);
                if (rest.StartsWith("NOTICE") || rest.StartsWith("INFO"))
                {
                    OnStatusChanged?.Invoke("[" + bucketName + "] " + line);
                    return;
                }

                if (rest.StartsWith("ERROR") || rest.StartsWith("CRITICAL") || rest.StartsWith("FATA"))
                {
                    OnError?.Invoke("[" + bucketName + "] " + line);
                    return;
                }
            }
        }

        if (line.StartsWith("The service rclone has been started.") ||
            line.StartsWith("rclone has been started."))
        {
            OnStatusChanged?.Invoke("[" + bucketName + "] " + line);
            return;
        }

        OnError?.Invoke("[" + bucketName + "] " + line);
    }

    public void Unmount()
    {
        foreach (var process in _processes.Values.ToList())
        {
            if (process.HasExited)
                continue;

            LogService.Info("Unmounting: killing rclone process PID=" + process.Id);
            _intentionalStops.Add(process.Id);
            try
            {
                process.Kill();
                process.WaitForExit(5000);
                LogService.Info("rclone process terminated. PID=" + process.Id);
            }
            catch (Exception ex)
            {
                LogService.Error("Error killing rclone process: " + ex.Message);
            }
            finally
            {
                process.Dispose();
            }
        }

        _processes.Clear();
    }

    public void Dispose()
    {
        LogService.Info("RcloneManager disposing.");
        Unmount();
    }
}
