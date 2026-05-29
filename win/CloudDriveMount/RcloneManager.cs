using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32;

namespace CloudDriveMount;

public class RcloneManager : IDisposable
{
    private sealed record MountSpec(string Label, string RemotePath, string DriveLetter, string VolumeName, string VfsCacheMode, bool ReadOnly);
    private sealed record DriveMountInfo(bool Exists, string VolumeLabel, string FileSystemName);
    private sealed record MountProcess(Process Process, string DriveLetter, int RcPort, string Label);

    private const int RcBasePort = 5572;
    private const int SHCNE_DRIVEADD = 0x00000100;
    private const int SHCNE_DRIVEREMOVE = 0x00008000;
    private const uint SHCNF_PATH = 0x0001;
    private const uint SHCNF_FLUSH = 0x1000;

    private readonly Dictionary<string, MountProcess> _mounts = new(StringComparer.OrdinalIgnoreCase);
    private readonly HashSet<int> _intentionalStops = new();
    private readonly string _configPath;
    private readonly string _protectedConfigPath;

    public event Action<string>? OnStatusChanged;
    public event Action<string>? OnError;

    public bool IsMounted => _mounts.Values.Any(m => !m.Process.HasExited);

    public RcloneManager()
    {
        var appData = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "CloudDriveMount");
        _configPath = Path.Combine(appData, "rclone.conf");
        _protectedConfigPath = _configPath + ".dpapi";
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

    public void CleanupExistingAppProcesses()
    {
        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            LogService.Info("Skipping stale rclone cleanup because rclone.exe was not found.");
            return;
        }

        foreach (var process in Process.GetProcessesByName("rclone"))
        {
            try
            {
                if (!IsAppRcloneProcess(process, rclonePath))
                    continue;

                LogService.Info("Killing stale app rclone process PID=" + process.Id);
                OnStatusChanged?.Invoke("Stopping stale rclone process PID=" + process.Id + ".");
                process.Kill(entireProcessTree: true);
                if (!process.WaitForExit(5000))
                {
                    LogService.Error("Timed out waiting for stale rclone process to exit. PID=" + process.Id);
                    OnError?.Invoke("Timed out waiting for stale rclone process to exit. PID=" + process.Id + ".");
                }
                else
                {
                    LogService.Info("Stale app rclone process terminated. PID=" + process.Id);
                }
            }
            catch (Exception ex)
            {
                LogService.Error("Failed to stop stale rclone process PID=" + process.Id + ": " + ex.Message);
                OnError?.Invoke("Failed to stop stale rclone process PID=" + process.Id + ": " + ex.Message);
            }
            finally
            {
                process.Dispose();
            }
        }
    }

    public bool IsGoogleDriveConfigured(GoogleDriveSettings googleDrive)
    {
        return HasConfigSection(GetGoogleDriveRemoteName(googleDrive));
    }

    public bool IsSeedboxConfigured(SeedboxSettings seedbox)
    {
        return HasConfigSection(GetSeedboxRemoteName(seedbox));
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

    public bool ConfigureSeedbox(SeedboxSettings seedbox, string password)
    {
        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            OnError?.Invoke("rclone.exe not found. Please install rclone and ensure it is in your PATH.");
            OnError?.Invoke("Searched: app directory, PATH, LocalAppData\\rclone, Program Files\\rclone.");
            return false;
        }

        NormalizeSeedboxSettings(seedbox);
        var remoteName = GetSeedboxRemoteName(seedbox);
        if (!ValidateSeedboxSettings(seedbox))
            return false;

        if (string.IsNullOrEmpty(password))
        {
            if (HasConfigSection(remoteName))
            {
                OnStatusChanged?.Invoke("Using existing Seedbox rclone config.");
                return true;
            }

            OnError?.Invoke("Enter your Seedbox FTPS password before testing or mounting for the first time.");
            return false;
        }

        var obscuredPassword = ObscurePassword(rclonePath, password);
        if (obscuredPassword is null)
            return false;

        var lines = new List<string>
        {
            "type = ftp",
            "host = " + seedbox.Host.Trim(),
            "user = " + seedbox.Username.Trim(),
            "port = " + seedbox.Port,
            "pass = " + obscuredPassword,
            "explicit_tls = true",
            "tls = false",
            "no_check_certificate = " + seedbox.AllowUnverifiedCertificate.ToString().ToLowerInvariant()
        };

        UpsertConfigSection(remoteName, lines);
        OnStatusChanged?.Invoke("Seedbox FTPS rclone config written to: " + _configPath + ".");
        return true;
    }

    public bool TestSeedboxConnection(SeedboxSettings seedbox, string password)
    {
        var rclonePath = FindRclone();
        if (rclonePath is null)
        {
            OnError?.Invoke("rclone.exe not found. Please install rclone and ensure it is in your PATH.");
            OnError?.Invoke("Searched: app directory, PATH, LocalAppData\\rclone, Program Files\\rclone.");
            return false;
        }

        if (!ConfigureSeedbox(seedbox, password))
            return false;

        var remotePath = BuildSeedboxRemotePath(seedbox);
        var args = new List<string>
        {
            "lsd",
            remotePath,
            "--config",
            _configPath
        };

        OnStatusChanged?.Invoke("Testing Seedbox FTPS connection using " + remotePath + ".");
        var ok = RunRcloneToCompletion(rclonePath, args, "Seedbox connection test");
        if (ok)
            OnStatusChanged?.Invoke("Seedbox connection test completed successfully.");

        return ok;
    }

    public bool DisconnectSeedbox(SeedboxSettings seedbox)
    {
        var remoteName = GetSeedboxRemoteName(seedbox);
        if (!IsValidRcloneRemoteName(remoteName))
        {
            OnError?.Invoke("Seedbox remote name cannot contain a colon, square bracket, or line break.");
            return false;
        }

        try
        {
            RemoveConfigSection(remoteName);
            OnStatusChanged?.Invoke("Seedbox remote has been removed from the app rclone config.");
            return true;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to disconnect Seedbox: " + ex);
            OnError?.Invoke("Failed to disconnect Seedbox: " + ex.Message);
            return false;
        }
    }

    public bool Mount(AppSettings settings)
    {
        settings.Buckets ??= new List<BucketMount>();
        settings.GoogleDrive ??= new GoogleDriveSettings();
        settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        settings.Seedbox ??= new SeedboxSettings();
        NormalizeSeedboxSettings(settings.Seedbox);

        LogService.Info("Mount requested. BucketCount=" + settings.Buckets.Count + " GoogleDrive=" + settings.GoogleDrive.RemoteName + " Seedbox=" + settings.Seedbox.RemoteName);

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
            var b2Credentials = WindowsSecureStore.LoadB2Credentials();
            if (string.IsNullOrWhiteSpace(b2Credentials?.ApplicationKeyId) || string.IsNullOrWhiteSpace(b2Credentials?.ApplicationKey))
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
                EnsureB2Config(b2Credentials);
                OnStatusChanged?.Invoke($"B2 rclone config written to: {_configPath}");

                foreach (var bucket in settings.Buckets)
                {
                    var bucketName = bucket.BucketName.Trim();
                    var driveLetter = NormalizeDriveLetter(bucket.DriveLetter);
                    if (CloudProvider.IsReservedGoogleDriveLetter(driveLetter))
                    {
                        OnError?.Invoke("Drive letter G: is reserved for Google Drive. Choose another drive letter for bucket '" + bucketName + "'.");
                        validationOk = false;
                        continue;
                    }

                    mountSpecs.Add(new MountSpec(
                        Label: "B2 " + bucketName,
                        RemotePath: "b2remote:" + bucketName,
                        DriveLetter: driveLetter,
                        VolumeName: bucketName,
                        VfsCacheMode: "writes",
                        ReadOnly: false));
                }
            }
        }

        var googleDrive = settings.GoogleDrive;
        googleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;
        var googleDriveSelected = string.Equals(settings.SelectedProvider, CloudProvider.GoogleDrive, StringComparison.OrdinalIgnoreCase);
        var googleDriveConfigured = HasConfigSection(GetGoogleDriveRemoteName(googleDrive));

        if (googleDriveConfigured || googleDriveSelected)
        {
            var remoteName = GetGoogleDriveRemoteName(googleDrive);
            if (!IsValidRcloneRemoteName(remoteName))
            {
                OnError?.Invoke("Google Drive remote name cannot contain a colon, square bracket, or line break.");
                validationOk = false;
            }
            else if (!googleDriveConfigured)
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
                    VfsCacheMode: "full",
                    ReadOnly: false));
            }
        }

        var seedbox = settings.Seedbox;
        var seedboxSelected = string.Equals(settings.SelectedProvider, CloudProvider.Seedbox, StringComparison.OrdinalIgnoreCase);
        var seedboxConfigured = HasConfigSection(GetSeedboxRemoteName(seedbox));
        var hasSeedboxSettings = HasSeedboxSettings(seedbox);

        if (seedboxConfigured && hasSeedboxSettings || seedboxSelected)
        {
            if (!ValidateSeedboxSettings(seedbox))
            {
                validationOk = false;
            }
            else if (!seedboxConfigured && !ConfigureSeedbox(seedbox, WindowsSecureStore.LoadSeedboxPassword()))
            {
                OnError?.Invoke("Seedbox is not configured yet. Enter the FTPS password and click Test Connection first.");
                validationOk = false;
            }
            else
            {
                var driveLetter = NormalizeDriveLetter(seedbox.DriveLetter);
                var remotePath = BuildSeedboxRemotePath(seedbox);
                mountSpecs.Add(new MountSpec(
                    Label: "Seedbox",
                    RemotePath: remotePath,
                    DriveLetter: driveLetter,
                    VolumeName: "Seedbox",
                    VfsCacheMode: "full",
                    ReadOnly: seedbox.ReadOnly));
            }
        }

        if (!validationOk)
            return false;

        if (mountSpecs.Count == 0)
        {
            OnError?.Invoke("Configure at least one B2 bucket, Google Drive, or Seedbox mount before mounting.");
            return false;
        }

        Unmount();
        EnsureConfigDirectory();

        var success = true;
        foreach (var mount in mountSpecs)
        {
            success &= MountRemote(rclonePath, mount);
        }

        return success;
    }

    private void EnsureB2Config(B2StoredCredentials credentials)
    {
        var lines = new List<string>
        {
            "type = b2",
            "account = " + credentials.ApplicationKeyId,
            "key = " + credentials.ApplicationKey
        };

        UpsertConfigSection("b2remote", lines);
        LogService.Info("B2 rclone config written to: " + _configPath);
    }

    private bool MountRemote(string rclonePath, MountSpec mount)
    {
        EnsureConfigDirectory();
        var driveLetter = NormalizeDriveLetter(mount.DriveLetter);
        var existingDrive = GetDriveMountInfo(driveLetter);
        if (existingDrive.Exists &&
            string.Equals(existingDrive.FileSystemName, "FUSE-rclone", StringComparison.OrdinalIgnoreCase))
        {
            existingDrive = WaitForDriveToRelease(driveLetter, TimeSpan.FromSeconds(5));
        }

        if (existingDrive.Exists)
        {
            var details = string.IsNullOrWhiteSpace(existingDrive.FileSystemName)
                ? existingDrive.VolumeLabel
                : existingDrive.VolumeLabel + " (" + existingDrive.FileSystemName + ")";
            if (string.IsNullOrWhiteSpace(details))
                details = "another volume";
            OnError?.Invoke($"Drive {driveLetter} is already in use by {details}. Unmount it or choose another drive letter.");
            return false;
        }

        var rcPort = GetRcPortForDrive(driveLetter);
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
            "--links",
            "--rc",
            "--rc-addr",
            "127.0.0.1:" + rcPort,
            "--rc-no-auth"
        };

        if (mount.ReadOnly)
            args.Add("--read-only");

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

            _mounts[driveLetter] = new MountProcess(process, driveLetter, rcPort, mount.Label);
            LogService.Info("rclone process started. Label=" + mount.Label + " Drive=" + driveLetter + " PID=" + process.Id + " RC port=" + rcPort);
            OnStatusChanged?.Invoke($"Mounting {mount.Label} to {driveLetter}...");

            Task.Run(() =>
            {
                if (WaitForDriveToAppear(driveLetter, TimeSpan.FromSeconds(15)))
                    NotifyExplorerDriveAdded(driveLetter);
            });

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
        _mounts.Remove(driveLetter);
        NotifyExplorerDriveRemoved(driveLetter);

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
        foreach (var mount in _mounts.Values.ToList())
            StopMountProcess(mount);

        _mounts.Clear();
        ProtectConfigAtRest();
    }

    public void UnmountDrive(string driveLetter)
    {
        var drive = NormalizeDriveLetter(driveLetter);

        if (!_mounts.TryGetValue(drive, out var mount))
            return;

        _mounts.Remove(drive);
        StopMountProcess(mount);
    }

    public void NotifyExplorerForDriveLetters(IEnumerable<string> driveLetters)
    {
        foreach (var driveLetter in driveLetters)
            NotifyExplorerDriveRemoved(NormalizeDriveLetter(driveLetter));
    }

    private void StopMountProcess(MountProcess mount)
    {
        var process = mount.Process;
        var drive = mount.DriveLetter;

        if (process.HasExited)
        {
            WaitForDriveToRelease(drive, TimeSpan.FromSeconds(5));
            NotifyExplorerDriveRemoved(drive);
            process.Dispose();
            return;
        }

        LogService.Info("Unmounting " + drive + " via rclone remote control. PID=" + process.Id + " RC port=" + mount.RcPort);
        _intentionalStops.Add(process.Id);

        var rclonePath = FindRclone();
        if (rclonePath is not null)
            TryGracefulUnmount(rclonePath, mount);

        if (!process.WaitForExit(8000))
        {
            LogService.Info("Graceful unmount timed out for " + drive + "; terminating rclone PID=" + process.Id);
            try
            {
                process.Kill(entireProcessTree: true);
                process.WaitForExit(5000);
            }
            catch (Exception ex)
            {
                LogService.Error("Error killing rclone process for " + drive + ": " + ex.Message);
            }
        }
        else
        {
            LogService.Info("rclone process exited cleanly for " + drive + ". PID=" + process.Id);
        }

        WaitForDriveToRelease(drive, TimeSpan.FromSeconds(5));
        NotifyExplorerDriveRemoved(drive);
        process.Dispose();
    }

    private void TryGracefulUnmount(string rclonePath, MountProcess mount)
    {
        var url = "http://127.0.0.1:" + mount.RcPort;
        var drive = mount.DriveLetter;

        if (RunRcloneRc(rclonePath, "mount/unmount", "mountPoint=" + drive, url, "unmount " + drive))
            return;

        RunRcloneRc(rclonePath, "core/quit", string.Empty, url, "quit " + drive);
    }

    private bool RunRcloneRc(string rclonePath, string command, string parameter, string url, string label)
    {
        var args = new List<string> { "rc", command, "--url", url };
        if (!string.IsNullOrEmpty(parameter))
            args.Add(parameter);

        var psi = new ProcessStartInfo
        {
            FileName = rclonePath,
            Arguments = BuildArguments(args),
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        LogService.Info("Starting rclone rc. Label=" + label + " Command=" + psi.FileName + " " + psi.Arguments);

        try
        {
            using var process = Process.Start(psi);
            if (process is null)
                return false;

            if (!process.WaitForExit(5000))
            {
                try { process.Kill(); } catch { /* ignore */ }
            }

            var output = process.StandardOutput.ReadToEnd();
            var error = process.StandardError.ReadToEnd();

            if (process.ExitCode == 0)
            {
                LogService.Info("rclone rc succeeded. Label=" + label);
                return true;
            }

            _ = output;
            LogService.Debug("rclone rc failed. Label=" + label + " ExitCode=" + process.ExitCode + " Error=" + RedactSensitiveLine(error.Trim()));
            return false;
        }
        catch (Exception ex)
        {
            LogService.Debug("rclone rc failed. Label=" + label + " Error=" + ex.Message);
            return false;
        }
    }

    private void EnsureConfigDirectory()
    {
        var dir = Path.GetDirectoryName(_configPath);
        if (!string.IsNullOrEmpty(dir))
            Directory.CreateDirectory(dir);

        EnsureConfigMaterialized();
    }

    private void EnsureConfigMaterialized()
    {
        if (File.Exists(_configPath) || !File.Exists(_protectedConfigPath))
            return;

        try
        {
            var protectedBytes = File.ReadAllBytes(_protectedConfigPath);
            var bytes = WindowsSecureStore.UnprotectBytes(protectedBytes);
            File.WriteAllBytes(_configPath, bytes);
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to unlock protected rclone config: " + ex.Message);
        }
    }

    private void ProtectConfigAtRest()
    {
        try
        {
            if (!File.Exists(_configPath))
                return;

            var protectedBytes = WindowsSecureStore.ProtectBytes(File.ReadAllBytes(_configPath));
            File.WriteAllBytes(_protectedConfigPath, protectedBytes);
            File.Delete(_configPath);
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to protect rclone config at rest: " + ex.Message);
        }
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
        EnsureConfigDirectory();

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

    private string? ObscurePassword(string rclonePath, string password)
    {
        var psi = new ProcessStartInfo
        {
            FileName = rclonePath,
            Arguments = "obscure -",
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        try
        {
            using var process = new Process { StartInfo = psi };
            process.Start();
            process.StandardInput.WriteLine(password);
            process.StandardInput.Close();

            var output = process.StandardOutput.ReadToEnd().Trim();
            var error = process.StandardError.ReadToEnd();
            process.WaitForExit();

            if (process.ExitCode == 0 && !string.IsNullOrWhiteSpace(output))
                return output;

            LogService.Error("rclone obscure failed. ExitCode=" + process.ExitCode + " Error=" + RedactSensitiveLine(error));
            OnError?.Invoke("Could not prepare the Seedbox password for rclone.");
            return null;
        }
        catch (Exception ex)
        {
            LogService.Error("Failed to run rclone obscure: " + ex);
            OnError?.Invoke("Could not prepare the Seedbox password for rclone: " + ex.Message);
            return null;
        }
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

    private static string BuildSeedboxRemotePath(SeedboxSettings seedbox)
    {
        var remoteName = GetSeedboxRemoteName(seedbox);
        var remotePath = NormalizeRemotePath(seedbox.RemotePath);
        return string.IsNullOrWhiteSpace(remotePath) ? remoteName + ":" : remoteName + ":" + remotePath;
    }

    private static string GetSeedboxRemoteName(SeedboxSettings seedbox)
    {
        seedbox.RemoteName = CloudProvider.DefaultSeedboxRemoteName;
        return CloudProvider.DefaultSeedboxRemoteName;
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

    private static void NormalizeSeedboxSettings(SeedboxSettings seedbox)
    {
        seedbox.RemoteName = CloudProvider.DefaultSeedboxRemoteName;
        seedbox.Host = CloudProvider.NormalizeSeedboxHost(seedbox.Host);
        seedbox.Username = seedbox.Username.Trim();
        seedbox.RemotePath = NormalizeRemotePath(seedbox.RemotePath);
        seedbox.DriveLetter = string.IsNullOrWhiteSpace(seedbox.DriveLetter)
            ? CloudProvider.DefaultSeedboxLetter
            : CloudProvider.NormalizeDriveLetterInput(seedbox.DriveLetter);
        if (seedbox.Port <= 0 || seedbox.Port > 65535)
            seedbox.Port = 21;
    }

    private bool ValidateSeedboxSettings(SeedboxSettings seedbox)
    {
        if (seedbox.Port <= 0 || seedbox.Port > 65535)
        {
            OnError?.Invoke("Seedbox port must be between 1 and 65535.");
            return false;
        }

        NormalizeSeedboxSettings(seedbox);

        if (!IsValidRcloneRemoteName(GetSeedboxRemoteName(seedbox)))
        {
            OnError?.Invoke("Seedbox remote name cannot contain a colon, square bracket, or line break.");
            return false;
        }

        if (string.IsNullOrWhiteSpace(seedbox.Host) || string.IsNullOrWhiteSpace(seedbox.Username))
        {
            OnError?.Invoke("Enter your Seedbox host and username.");
            return false;
        }

        var drive = CloudProvider.NormalizeDriveLetterInput(seedbox.DriveLetter);
        if (drive.Length != 1 || !char.IsLetter(drive[0]))
        {
            OnError?.Invoke("Seedbox drive letter must be a single letter, like S.");
            return false;
        }

        return true;
    }

    private static bool HasSeedboxSettings(SeedboxSettings seedbox)
    {
        return !string.IsNullOrWhiteSpace(seedbox.Host) &&
               !string.IsNullOrWhiteSpace(seedbox.Username) &&
               !string.IsNullOrWhiteSpace(seedbox.DriveLetter);
    }

    private static string NormalizeDriveLetter(string driveLetter)
    {
        var drive = CloudProvider.NormalizeDriveLetterInput(driveLetter);

        if (!drive.EndsWith(":"))
            drive += ":";

        return drive;
    }

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool GetVolumeInformation(
        string lpRootPathName,
        StringBuilder lpVolumeNameBuffer,
        int nVolumeNameSize,
        out uint lpVolumeSerialNumber,
        out uint lpMaximumComponentLength,
        out uint lpFileSystemFlags,
        StringBuilder lpFileSystemNameBuffer,
        int nFileSystemNameSize);

    private static DriveMountInfo GetDriveMountInfo(string driveLetter)
    {
        var rootPath = NormalizeDriveLetter(driveLetter) + "\\";
        if (!Directory.Exists(rootPath))
            return new DriveMountInfo(false, string.Empty, string.Empty);

        var volumeName = new StringBuilder(261);
        var fileSystemName = new StringBuilder(261);
        var ok = GetVolumeInformation(
            rootPath,
            volumeName,
            volumeName.Capacity,
            out _,
            out _,
            out _,
            fileSystemName,
            fileSystemName.Capacity);

        return ok
            ? new DriveMountInfo(true, volumeName.ToString(), fileSystemName.ToString())
            : new DriveMountInfo(true, string.Empty, string.Empty);
    }

    private static DriveMountInfo WaitForDriveToRelease(string driveLetter, TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow.Add(timeout);
        var info = GetDriveMountInfo(driveLetter);

        while (info.Exists && DateTime.UtcNow < deadline)
        {
            Thread.Sleep(100);
            info = GetDriveMountInfo(driveLetter);
        }

        return info;
    }

    private static bool WaitForDriveToAppear(string driveLetter, TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow.Add(timeout);
        while (DateTime.UtcNow < deadline)
        {
            if (GetDriveMountInfo(driveLetter).Exists)
                return true;

            Thread.Sleep(200);
        }

        return GetDriveMountInfo(driveLetter).Exists;
    }

    private static int GetRcPortForDrive(string driveLetter)
    {
        var drive = CloudProvider.NormalizeDriveLetterInput(driveLetter);
        if (drive.Length != 1 || !char.IsLetter(drive[0]))
            return RcBasePort;

        return RcBasePort + (char.ToUpperInvariant(drive[0]) - 'A');
    }

    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern void SHChangeNotify(int wEventId, uint uFlags, string? psz1, IntPtr psz2);

    private static void NotifyExplorerDriveAdded(string driveLetter)
    {
        var path = NormalizeDriveLetter(driveLetter) + "\\";
        SHChangeNotify(SHCNE_DRIVEADD, SHCNF_PATH | SHCNF_FLUSH, path, IntPtr.Zero);
    }

    private static void NotifyExplorerDriveRemoved(string driveLetter)
    {
        var path = NormalizeDriveLetter(driveLetter) + "\\";
        SHChangeNotify(SHCNE_DRIVEREMOVE, SHCNF_PATH | SHCNF_FLUSH, path, IntPtr.Zero);
    }

    private static bool IsAppRcloneProcess(Process process, string rclonePath)
    {
        if (process.HasExited)
            return false;

        string? processPath = null;
        try
        {
            processPath = process.MainModule?.FileName;
        }
        catch
        {
            return false;
        }

        if (string.IsNullOrWhiteSpace(processPath))
            return false;

        var knownAppRclonePaths = new[]
        {
            rclonePath,
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles), "Cloud Drive Mount", "rclone.exe"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86), "Cloud Drive Mount", "rclone.exe")
        };

        return knownAppRclonePaths.Any(path =>
            !string.IsNullOrWhiteSpace(path) &&
            Path.GetFullPath(processPath).Equals(Path.GetFullPath(path), StringComparison.OrdinalIgnoreCase));
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
            "pass",
            "password",
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
