using System.Diagnostics;
using System.IO;
using System.Text;
using Microsoft.Win32;

namespace CloudDriveMount;

public class RcloneManager : IDisposable
{
    private sealed record MountSpec(string Label, string RemotePath, string DriveLetter, string VolumeName, string VfsCacheMode);

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

    public bool IsGoogleDriveConfigured(GoogleDriveSettings googleDrive)
    {
        return HasConfigSection(GetGoogleDriveRemoteName(googleDrive));
    }

    public bool ConfigureGoogleDrive(GoogleDriveSettings googleDrive)
    {
        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            OnError?.Invoke("rclone.exe not found. Please install rclone and ensure it is in your PATH.");
            OnError?.Invoke("Searched: app directory, PATH, LocalAppData\\rclone, Program Files\\rclone.");
            return false;
        }

        var remoteName = GetGoogleDriveRemoteName(googleDrive);
        if (!IsValidRcloneRemoteName(remoteName))
        {
            OnError?.Invoke("Google Drive remote name cannot contain a colon, square bracket, or line break.");
            return false;
        }

        EnsureConfigDirectory();
        var originalConfigLines = File.Exists(_configPath)
            ? File.ReadAllLines(_configPath, Encoding.UTF8).ToList()
            : new List<string>();
        RemoveConfigSection(remoteName);

        var args = new List<string>
        {
            "config",
            "create",
            remoteName,
            "drive",
            "scope",
            "drive",
            "config_is_local",
            "true",
            "--no-output"
        };

        if (!string.IsNullOrWhiteSpace(googleDrive.RootFolderId))
        {
            args.Add("root_folder_id");
            args.Add(googleDrive.RootFolderId.Trim());
        }

        args.Add("--config");
        args.Add(_configPath);

        OnStatusChanged?.Invoke("Starting Google Drive authorization.");
        OnStatusChanged?.Invoke("A browser window should open. Sign in and allow access to complete the rclone setup.");

        var ok = RunRcloneToCompletion(rclonePath, args, "Google Drive authorization");
        if (!ok)
        {
            File.WriteAllLines(_configPath, originalConfigLines, Encoding.UTF8);
            return false;
        }

        if (!HasConfigSection(remoteName))
        {
            File.WriteAllLines(_configPath, originalConfigLines, Encoding.UTF8);
            OnError?.Invoke("Google Drive authorization completed, but the rclone remote was not found in the app config.");
            return false;
        }

        OnStatusChanged?.Invoke("Google Drive is configured in " + _configPath + ".");
        return true;
    }

    public bool DisconnectGoogleDrive(GoogleDriveSettings googleDrive)
    {
        var remoteName = GetGoogleDriveRemoteName(googleDrive);
        if (!IsValidRcloneRemoteName(remoteName))
        {
            OnError?.Invoke("Google Drive remote name cannot contain a colon, square bracket, or line break.");
            return false;
        }

        try
        {
            RemoveConfigSection(remoteName);
            OnStatusChanged?.Invoke("Google Drive remote has been removed from the app rclone config.");
            return true;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to disconnect Google Drive: " + ex);
            OnError?.Invoke("Failed to disconnect Google Drive: " + ex.Message);
            return false;
        }
    }

    public bool TestGoogleDriveConnection(GoogleDriveSettings googleDrive)
    {
        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            OnError?.Invoke("rclone.exe not found. Please install rclone and ensure it is in your PATH.");
            OnError?.Invoke("Searched: app directory, PATH, LocalAppData\\rclone, Program Files\\rclone.");
            return false;
        }

        var remoteName = GetGoogleDriveRemoteName(googleDrive);
        if (!HasConfigSection(remoteName))
        {
            OnError?.Invoke("Google Drive is not configured yet. Click Connect Google Drive first.");
            return false;
        }

        var remotePath = BuildGoogleDriveRemotePath(googleDrive);
        var args = new List<string>
        {
            "lsd",
            remotePath,
            "--config",
            _configPath
        };

        OnStatusChanged?.Invoke("Testing Google Drive connection using " + remotePath + ".");

        var ok = RunRcloneToCompletion(rclonePath, args, "Google Drive connection test");
        if (ok)
            OnStatusChanged?.Invoke("Google Drive connection test completed successfully.");

        return ok;
    }

    public bool Mount(AppSettings settings)
    {
        settings.Buckets ??= new List<BucketMount>();
        settings.GoogleDrive ??= new GoogleDriveSettings();
        settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;

        LogService.Info("Mount requested. BucketCount=" + settings.Buckets.Count + " GoogleDrive=" + settings.GoogleDrive.RemoteName);

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

        var mountSpecs = new List<MountSpec>();
        var validationOk = true;

        var hasB2Input = settings.Buckets.Count > 0;

        if (hasB2Input)
        {
            if (string.IsNullOrWhiteSpace(settings.ApplicationKeyId) || string.IsNullOrWhiteSpace(settings.ApplicationKey))
            {
                LogService.Error("B2 credentials not configured.");
                OnError?.Invoke("Backblaze B2 credentials are not configured.");
                OnError?.Invoke("Enter your Application Key ID and Application Key in Settings, then click Save and Mount All.");
                validationOk = false;
            }
            else if (settings.Buckets.Count == 0)
            {
                OnError?.Invoke("At least one B2 bucket and drive letter is required.");
                validationOk = false;
            }
            else
            {
                EnsureB2Config(settings);
                OnStatusChanged?.Invoke($"B2 rclone config written to: {_configPath}");

                foreach (var bucket in settings.Buckets)
                {
                    var bucketName = bucket.BucketName.Trim();
                    var driveLetter = NormalizeDriveLetter(bucket.DriveLetter);
                    mountSpecs.Add(new MountSpec(
                        Label: "B2 " + bucketName,
                        RemotePath: "b2remote:" + bucketName,
                        DriveLetter: driveLetter,
                        VolumeName: bucketName,
                        VfsCacheMode: "writes"));
                }
            }
        }

        var googleDrive = settings.GoogleDrive;
        if (!string.IsNullOrWhiteSpace(googleDrive.DriveLetter))
        {
            var remoteName = GetGoogleDriveRemoteName(googleDrive);
            if (!IsValidRcloneRemoteName(remoteName))
            {
                OnError?.Invoke("Google Drive remote name cannot contain a colon, square bracket, or line break.");
                validationOk = false;
            }
            else if (!HasConfigSection(remoteName))
            {
                OnError?.Invoke("Google Drive is not configured yet. Select Google Drive and click Connect Google Drive first.");
                validationOk = false;
            }
            else
            {
                var driveLetter = NormalizeDriveLetter(googleDrive.DriveLetter);
                var remotePath = BuildGoogleDriveRemotePath(googleDrive);
                mountSpecs.Add(new MountSpec(
                    Label: "Google Drive",
                    RemotePath: remotePath,
                    DriveLetter: driveLetter,
                    VolumeName: string.IsNullOrWhiteSpace(googleDrive.RemotePath) ? "Google Drive" : googleDrive.RemotePath.Trim(),
                    VfsCacheMode: "full"));
            }
        }

        if (!validationOk)
            return false;

        if (mountSpecs.Count == 0)
        {
            OnError?.Invoke("Configure at least one B2 bucket or Google Drive mount before mounting.");
            return false;
        }

        Unmount();

        var success = true;
        foreach (var mount in mountSpecs)
        {
            success &= MountRemote(rclonePath, mount);
        }

        return success;
    }

    private void EnsureB2Config(AppSettings settings)
    {
        var lines = new List<string>
        {
            "type = b2",
            "account = " + settings.ApplicationKeyId,
            "key = " + settings.ApplicationKey
        };

        UpsertConfigSection("b2remote", lines);
        LogService.Info("B2 rclone config written to: " + _configPath);
    }

    private bool MountRemote(string rclonePath, MountSpec mount)
    {
        var driveLetter = NormalizeDriveLetter(mount.DriveLetter);
        var args = new List<string>
        {
            "mount",
            mount.RemotePath,
            driveLetter,
            "--config",
            _configPath,
            "--vfs-cache-mode",
            mount.VfsCacheMode,
            "--volname",
            mount.VolumeName,
            "--links"
        };

        var psi = new ProcessStartInfo
        {
            FileName = rclonePath,
            Arguments = BuildArguments(args),
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        LogService.Info("Starting rclone mount. Label=" + mount.Label + " Remote=" + mount.RemotePath + " Drive=" + driveLetter + " Command=" + psi.FileName + " " + psi.Arguments);

        try
        {
            var process = new Process { StartInfo = psi, EnableRaisingEvents = true };
            process.Exited += (_, _) => HandleProcessExited(process, mount.Label, driveLetter);
            process.OutputDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                {
                    var safeLine = RedactSensitiveLine(e.Data);
                    LogService.Debug("[rclone stdout] [" + mount.Label + "] " + safeLine);
                    OnStatusChanged?.Invoke("[" + mount.Label + "] " + safeLine);
                }
            };
            process.ErrorDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                    HandleStderrLine(e.Data, mount.Label);
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            _processes[driveLetter] = process;
            LogService.Info("rclone process started. Label=" + mount.Label + " Drive=" + driveLetter + " PID=" + process.Id);
            OnStatusChanged?.Invoke($"Mounting {mount.Label} to {driveLetter}...");
            return true;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to start rclone for " + mount.Label + ": " + ex);
            OnError?.Invoke($"Failed to start rclone for {mount.Label}: {ex.Message}");
            return false;
        }
    }

    private bool RunRcloneToCompletion(string rclonePath, List<string> args, string label)
    {
        var psi = new ProcessStartInfo
        {
            FileName = rclonePath,
            Arguments = BuildArguments(args),
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        LogService.Info("Starting rclone command. Label=" + label + " Command=" + psi.FileName + " " + psi.Arguments);

        try
        {
            using var process = new Process { StartInfo = psi, EnableRaisingEvents = true };
            process.OutputDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                {
                    var safeLine = RedactSensitiveLine(e.Data);
                    LogService.Debug("[rclone stdout] [" + label + "] " + safeLine);
                    OnStatusChanged?.Invoke("[" + label + "] " + safeLine);
                }
            };
            process.ErrorDataReceived += (_, e) =>
            {
                if (!string.IsNullOrWhiteSpace(e.Data))
                {
                    var safeLine = RedactSensitiveLine(e.Data);
                    LogService.Debug("[rclone stderr] [" + label + "] " + safeLine);
                    if (LooksLikeRcloneError(safeLine))
                        OnError?.Invoke("[" + label + "] " + safeLine);
                    else
                        OnStatusChanged?.Invoke("[" + label + "] " + safeLine);
                }
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            process.WaitForExit();
            process.WaitForExit();

            if (process.ExitCode == 0)
            {
                LogService.Info("rclone command completed successfully. Label=" + label);
                return true;
            }

            LogService.Error("rclone command exited with code " + process.ExitCode + ". Label=" + label);
            OnError?.Invoke(label + " exited with code " + process.ExitCode + ".");
            return false;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to run rclone command for " + label + ": " + ex);
            OnError?.Invoke("Failed to run rclone command for " + label + ": " + ex.Message);
            return false;
        }
    }

    private void HandleProcessExited(Process process, string label, string driveLetter)
    {
        var code = process.ExitCode;
        if (_intentionalStops.Remove(process.Id))
        {
            LogService.Info("Mount process stopped (unmounted). Label=" + label + " Drive=" + driveLetter);
            return;
        }

        if (code == 0)
        {
            LogService.Info("Mount process exited normally. Label=" + label + " Drive=" + driveLetter);
            OnStatusChanged?.Invoke($"Mount process exited normally for {label} ({driveLetter}).");
        }
        else
        {
            LogService.Error("Mount process exited with code " + code + ". Label=" + label + " Drive=" + driveLetter);
            OnError?.Invoke($"Mount process exited with code {code} for {label} ({driveLetter}).");
        }
    }

    private void HandleStderrLine(string line, string label)
    {
        var safeLine = RedactSensitiveLine(line);
        LogService.Debug("[rclone stderr] [" + label + "] " + safeLine);

        if (safeLine.Length > 20)
        {
            var prefix = safeLine.Substring(0, 20);
            if (prefix[4] == '/' && prefix[7] == '/' && prefix[10] == ' ' &&
                prefix[13] == ':' && prefix[16] == ':' && prefix[19] == ' ')
            {
                var rest = safeLine.Substring(20);
                if (rest.StartsWith("NOTICE") || rest.StartsWith("INFO"))
                {
                    OnStatusChanged?.Invoke("[" + label + "] " + safeLine);
                    return;
                }

                if (rest.StartsWith("ERROR") || rest.StartsWith("CRITICAL") || rest.StartsWith("FATA"))
                {
                    OnError?.Invoke("[" + label + "] " + safeLine);
                    return;
                }
            }
        }

        if (safeLine.StartsWith("The service rclone has been started.") ||
            safeLine.StartsWith("rclone has been started."))
        {
            OnStatusChanged?.Invoke("[" + label + "] " + safeLine);
            return;
        }

        OnError?.Invoke("[" + label + "] " + safeLine);
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

    public void UnmountDrive(string driveLetter)
    {
        var drive = NormalizeDriveLetter(driveLetter);

        if (!_processes.TryGetValue(drive, out var process))
            return;

        _processes.Remove(drive);
        if (process.HasExited)
        {
            process.Dispose();
            return;
        }

        LogService.Info("Unmounting drive " + drive + ": killing rclone process PID=" + process.Id);
        _intentionalStops.Add(process.Id);
        try
        {
            process.Kill();
            process.WaitForExit(5000);
            LogService.Info("rclone process terminated for drive " + drive + ". PID=" + process.Id);
        }
        catch (Exception ex)
        {
            LogService.Error("Error killing rclone process for drive " + drive + ": " + ex.Message);
        }
        finally
        {
            process.Dispose();
        }
    }

    private void EnsureConfigDirectory()
    {
        var dir = Path.GetDirectoryName(_configPath);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);
    }

    private void UpsertConfigSection(string sectionName, IEnumerable<string> sectionLines)
    {
        EnsureConfigDirectory();

        var lines = File.Exists(_configPath)
            ? File.ReadAllLines(_configPath, Encoding.UTF8).ToList()
            : new List<string>();

        lines = RemoveSection(lines, sectionName);

        if (lines.Count > 0 && !string.IsNullOrWhiteSpace(lines[^1]))
            lines.Add(string.Empty);

        lines.Add("[" + sectionName + "]");
        lines.AddRange(sectionLines);

        File.WriteAllLines(_configPath, lines, Encoding.UTF8);
    }

    private void RemoveConfigSection(string sectionName)
    {
        EnsureConfigDirectory();

        if (!File.Exists(_configPath))
            return;

        var lines = File.ReadAllLines(_configPath, Encoding.UTF8).ToList();
        lines = RemoveSection(lines, sectionName);
        File.WriteAllLines(_configPath, lines, Encoding.UTF8);
    }

    private bool HasConfigSection(string sectionName)
    {
        if (!File.Exists(_configPath))
            return false;

        foreach (var line in File.ReadLines(_configPath, Encoding.UTF8))
        {
            if (TryReadSectionName(line, out var currentSection) &&
                string.Equals(currentSection, sectionName, StringComparison.OrdinalIgnoreCase))
            {
                return true;
            }
        }

        return false;
    }

    private static List<string> RemoveSection(List<string> lines, string sectionName)
    {
        var output = new List<string>();
        var skip = false;

        foreach (var line in lines)
        {
            if (TryReadSectionName(line, out var currentSection))
                skip = string.Equals(currentSection, sectionName, StringComparison.OrdinalIgnoreCase);

            if (!skip)
                output.Add(line);
        }

        while (output.Count > 0 && string.IsNullOrWhiteSpace(output[^1]))
            output.RemoveAt(output.Count - 1);

        return output;
    }

    private static bool TryReadSectionName(string line, out string sectionName)
    {
        var trimmed = line.Trim();
        if (trimmed.Length >= 3 && trimmed.StartsWith("[") && trimmed.EndsWith("]"))
        {
            sectionName = trimmed[1..^1];
            return true;
        }

        sectionName = string.Empty;
        return false;
    }

    private static string BuildGoogleDriveRemotePath(GoogleDriveSettings googleDrive)
    {
        var remoteName = GetGoogleDriveRemoteName(googleDrive);
        var remotePath = NormalizeRemotePath(googleDrive.RemotePath);
        return string.IsNullOrWhiteSpace(remotePath) ? remoteName + ":" : remoteName + ":" + remotePath;
    }

    private static string GetGoogleDriveRemoteName(GoogleDriveSettings googleDrive)
    {
        googleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        return CloudProvider.DefaultGoogleDriveRemoteName;
    }

    private static string NormalizeRemotePath(string path)
    {
        var normalized = path.Trim().Replace('\\', '/');
        while (normalized.StartsWith("/"))
            normalized = normalized[1..];
        while (normalized.StartsWith(":"))
            normalized = normalized[1..];

        return normalized;
    }

    private static string NormalizeDriveLetter(string driveLetter)
    {
        var drive = driveLetter.Trim().ToUpperInvariant();

        if (drive.EndsWith(":/") || drive.EndsWith(":\\"))
            drive = drive[..^2];
        else if (drive.EndsWith(":"))
            drive = drive[..^1];

        if (!drive.EndsWith(":"))
            drive += ":";

        return drive;
    }

    private static bool IsValidRcloneRemoteName(string remoteName)
    {
        return !string.IsNullOrWhiteSpace(remoteName) &&
               !remoteName.Contains(':') &&
               !remoteName.Contains('[') &&
               !remoteName.Contains(']') &&
               !remoteName.Contains('\r') &&
               !remoteName.Contains('\n');
    }

    private static bool LooksLikeRcloneError(string line)
    {
        return line.Contains(" ERROR ", StringComparison.OrdinalIgnoreCase) ||
               line.Contains(" CRITICAL ", StringComparison.OrdinalIgnoreCase) ||
               line.Contains(" FATAL ", StringComparison.OrdinalIgnoreCase) ||
               line.Contains("Failed", StringComparison.OrdinalIgnoreCase) ||
               line.Contains("error", StringComparison.OrdinalIgnoreCase);
    }

    private static string RedactSensitiveLine(string line)
    {
        var trimmedStart = line.TrimStart();
        var leadingWhitespaceLength = line.Length - trimmedStart.Length;
        var leadingWhitespace = leadingWhitespaceLength > 0 ? line[..leadingWhitespaceLength] : string.Empty;

        var sensitiveKeys = new[]
        {
            "token",
            "access_token",
            "refresh_token",
            "client_secret",
            "service_account_credentials",
            "key",
            "account"
        };

        foreach (var key in sensitiveKeys)
        {
            if (trimmedStart.StartsWith(key + " =", StringComparison.OrdinalIgnoreCase) ||
                trimmedStart.StartsWith(key + "=", StringComparison.OrdinalIgnoreCase))
            {
                return leadingWhitespace + key + " = <redacted>";
            }
        }

        if (line.Contains("\"access_token\"", StringComparison.OrdinalIgnoreCase) ||
            line.Contains("\"refresh_token\"", StringComparison.OrdinalIgnoreCase) ||
            line.Contains("\"client_secret\"", StringComparison.OrdinalIgnoreCase))
        {
            return "<redacted sensitive output>";
        }

        return line;
    }

    private static string BuildArguments(IEnumerable<string> args)
    {
        return string.Join(" ", args.Select(QuoteArgument));
    }

    private static string QuoteArgument(string arg)
    {
        if (arg.Length == 0)
            return "\"\"";

        if (!arg.Any(char.IsWhiteSpace) && !arg.Contains('"'))
            return arg;

        return "\"" + arg.Replace("\"", "\\\"") + "\"";
    }

    public void Dispose()
    {
        LogService.Info("RcloneManager disposing.");
        Unmount();
    }
}
