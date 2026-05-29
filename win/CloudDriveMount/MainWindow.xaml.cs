using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Forms;

namespace CloudDriveMount;

public partial class MainWindow : Window
{
    private readonly SettingsService _settingsService;
    private readonly RcloneManager _rcloneManager;
    private AppSettings _settings;
    private TrayIconManager? _tray;
    private bool _isLoadingProvider;
    private string _lastTrayErrorMessage = string.Empty;
    private DateTime _lastTrayErrorAt = DateTime.MinValue;

    public bool AllowClose { get; set; } = false;

    public MainWindow()
    {
        InitializeComponent();
        _settingsService = new SettingsService();
        _rcloneManager = new RcloneManager();

        _rcloneManager.OnStatusChanged += msg => Dispatcher.Invoke(() =>
        {
            AddLog("[INFO] " + msg);
            LogService.Info(msg);
        });
        _rcloneManager.OnError += msg => Dispatcher.Invoke(() =>
        {
            AddLog("[ERROR] " + msg);
            LogService.Error(msg);
            if (ShouldShowTrayError(msg))
                _tray?.ShowBalloonTip("Cloud Drive Mount Error", msg, ToolTipIcon.Error);
        });

        _settings = _settingsService.Load();
        _settings.Buckets ??= new List<BucketMount>();
        _settings.GoogleDrive ??= new GoogleDriveSettings();
        _settings.Seedbox ??= new SeedboxSettings();
        _settings.SelectedProvider = CloudProvider.Normalize(_settings.SelectedProvider);
        EnsureGoogleDriveDefaults();
        EnsureSeedboxDefaults();

        var b2Credentials = WindowsSecureStore.LoadB2Credentials();
        TxtKeyId.Text = b2Credentials?.ApplicationKeyId ?? string.Empty;
        TxtKey.Text = b2Credentials?.ApplicationKey ?? string.Empty;

        TxtGoogleRemotePath.Text = _settings.GoogleDrive.RemotePath;
        TxtGoogleRootFolderId.Text = _settings.GoogleDrive.RootFolderId;

        TxtSeedboxHost.Text = CloudProvider.NormalizeSeedboxHost(_settings.Seedbox.Host);
        TxtSeedboxUsername.Text = _settings.Seedbox.Username;
        TxtSeedboxPort.Text = _settings.Seedbox.Port.ToString();
        TxtSeedboxRemotePath.Text = _settings.Seedbox.RemotePath;
        TxtSeedboxDriveLetter.Text = NormalizeDriveInput(_settings.Seedbox.DriveLetter);
        ChkSeedboxReadOnly.IsChecked = _settings.Seedbox.ReadOnly;
        ChkSeedboxAllowUnverified.IsChecked = _settings.Seedbox.AllowUnverifiedCertificate;

        ChkStartOnLogin.IsChecked = StartupManager.IsSet();
        ChkStartMinimized.IsChecked = _settings.StartMinimized;

        if (_settings.Buckets.Count == 0)
            AddBucketRow();
        else
            foreach (var bucket in _settings.Buckets)
                AddBucketRow(bucket.BucketName, bucket.DriveLetter);

        _isLoadingProvider = true;
        CmbProvider.SelectedIndex = _settings.SelectedProvider switch
        {
            CloudProvider.GoogleDrive => 1,
            CloudProvider.Seedbox => 2,
            _ => 0
        };
        _isLoadingProvider = false;

        UpdateProviderPanels();
        RefreshGoogleDriveConnectionUi();
        RefreshSeedboxConnectionUi();
        UpdateSaveMountButton();

        var logMsg = "Cloud Drive Mount started. Log file: " + LogService.GetLogFilePath();
        AddLog("[INFO] " + logMsg);
        LogService.Info(logMsg);
    }

    public void SetTrayIcon(TrayIconManager tray) => _tray = tray;

    public void ShowSettingsWindow()
    {
        Show();
        WindowState = WindowState.Normal;
        Activate();
    }

    public void CleanupExistingAppProcesses()
    {
        _rcloneManager.CleanupExistingAppProcesses();
    }

    public void AttemptMount()
    {
        if (HasAnyCompleteMount(includeSelectedGoogleDrive: false))
        {
            Dispatcher.BeginInvoke(() => BtnSaveMount_Click(null, null));
        }
    }

    public void AllowCloseAndQuit()
    {
        AllowClose = true;
        _rcloneManager.Dispose();
        Close();
    }

    private void AddLog(string message)
    {
        var line = $"[{DateTime.Now:HH:mm:ss}] {message}";
        TxtLog.AppendText(line + Environment.NewLine);
        TxtLog.CaretIndex = TxtLog.Text.Length;
        TxtLog.ScrollToEnd();
    }

    private bool ShouldShowTrayError(string message)
    {
        var now = DateTime.Now;
        var trimmed = StripRcloneLabel(message).Trim();

        if (IsRcloneErrorDetailLine(trimmed))
            return false;

        if (string.Equals(message, _lastTrayErrorMessage, StringComparison.Ordinal) &&
            (now - _lastTrayErrorAt).TotalSeconds < 60)
        {
            return false;
        }

        if (message.Contains("Mount process exited with code", StringComparison.OrdinalIgnoreCase) &&
            (now - _lastTrayErrorAt).TotalSeconds < 10)
        {
            return false;
        }

        _lastTrayErrorMessage = message;
        _lastTrayErrorAt = now;
        return true;
    }

    private static string StripRcloneLabel(string message)
    {
        var trimmed = message.TrimStart();
        if (!trimmed.StartsWith("["))
            return trimmed;

        var end = trimmed.IndexOf(']');
        return end >= 0 && end + 1 < trimmed.Length ? trimmed[(end + 1)..].TrimStart() : trimmed;
    }

    private static bool IsRcloneErrorDetailLine(string line)
    {
        if (line.Equals("Details:", StringComparison.OrdinalIgnoreCase) ||
            line.Equals("[", StringComparison.Ordinal) ||
            line.Equals("]", StringComparison.Ordinal) ||
            line.Equals("{", StringComparison.Ordinal) ||
            line.Equals("}", StringComparison.Ordinal) ||
            line.Equals("},", StringComparison.Ordinal) ||
            line.Equals("],", StringComparison.Ordinal))
        {
            return true;
        }

        return line.StartsWith("\"", StringComparison.Ordinal) ||
               line.StartsWith("@type", StringComparison.OrdinalIgnoreCase) ||
               line.StartsWith("metadata", StringComparison.OrdinalIgnoreCase) ||
               line.StartsWith(", rateLimitExceeded", StringComparison.OrdinalIgnoreCase);
    }

    private void AddBucketRow(string bucketName = "", string driveLetter = "")
    {
        var rowSaved = !string.IsNullOrWhiteSpace(bucketName) && !string.IsNullOrWhiteSpace(driveLetter);
        var savedDriveLetter = NormalizeDriveInput(driveLetter);
        var row = new StackPanel
        {
            Orientation = System.Windows.Controls.Orientation.Horizontal,
            Margin = new Thickness(0, 2, 0, 2)
        };

        var bucketLabel = new System.Windows.Controls.Label { Content = "Bucket:", Width = 50, Padding = new Thickness(0), VerticalAlignment = VerticalAlignment.Center };
        var bucketText = new System.Windows.Controls.TextBox { Width = 180, Height = 23, Text = bucketName, Margin = new Thickness(0, 0, 8, 0), Padding = new Thickness(3, 0, 3, 0), VerticalContentAlignment = VerticalAlignment.Center };
        var driveLabel = new System.Windows.Controls.Label { Content = "Drive:", Width = 40, Padding = new Thickness(0), VerticalAlignment = VerticalAlignment.Center };
        var driveText = new System.Windows.Controls.TextBox { Width = 35, Height = 23, MaxLength = 3, Text = NormalizeDriveInput(driveLetter), Margin = new Thickness(0, 0, 8, 0), Padding = new Thickness(3, 0, 3, 0), VerticalContentAlignment = VerticalAlignment.Center };
        var saveButton = new System.Windows.Controls.Button { Content = "Save", Height = 23, Padding = new Thickness(6, 0, 6, 0), Margin = new Thickness(0, 0, 5, 0) };
        var removeButton = new System.Windows.Controls.Button { Content = "Remove", Height = 23, Padding = new Thickness(6, 0, 6, 0) };

        void UpdateRemoveButton() => removeButton.IsEnabled = rowSaved || (string.IsNullOrWhiteSpace(bucketText.Text) && string.IsNullOrWhiteSpace(driveText.Text));

        removeButton.Click += (_, _) =>
        {
            var driveToUnmount = rowSaved ? savedDriveLetter : NormalizeDriveInput(driveText.Text);
            BucketsPanel.Children.Remove(row);

            if (!string.IsNullOrWhiteSpace(driveToUnmount))
            {
                _rcloneManager.UnmountDrive(driveToUnmount);
                AddLog("[INFO] Removed drive " + driveToUnmount + ":");
            }

            SaveSettings(validateMounts: true);
            UpdateSaveMountButton();
        };
        saveButton.Click += (_, _) =>
        {
            if (SaveSettings(validateMounts: true))
            {
                rowSaved = true;
                savedDriveLetter = NormalizeDriveInput(driveText.Text);
                driveText.Text = savedDriveLetter;
                UpdateRemoveButton();
            }
        };
        bucketText.TextChanged += (_, _) =>
        {
            rowSaved = false;
            UpdateRemoveButton();
            UpdateSaveMountButton();
        };
        driveText.TextChanged += (_, _) =>
        {
            rowSaved = false;
            UpdateRemoveButton();
            UpdateSaveMountButton();
        };

        row.Children.Add(bucketLabel);
        row.Children.Add(bucketText);
        row.Children.Add(driveLabel);
        row.Children.Add(driveText);
        row.Children.Add(saveButton);
        row.Children.Add(removeButton);

        BucketsPanel.Children.Add(row);
        UpdateRemoveButton();
        UpdateSaveMountButton();
    }

    private void UpdateProviderPanels()
    {
        var provider = GetSelectedProvider();
        B2OptionsPanel.Visibility = provider == CloudProvider.BackblazeB2 ? Visibility.Visible : Visibility.Collapsed;
        GoogleDriveOptionsPanel.Visibility = provider == CloudProvider.GoogleDrive ? Visibility.Visible : Visibility.Collapsed;
        SeedboxOptionsPanel.Visibility = provider == CloudProvider.Seedbox ? Visibility.Visible : Visibility.Collapsed;
    }

    private string GetSelectedProvider()
    {
        return CmbProvider.SelectedIndex switch
        {
            1 => CloudProvider.GoogleDrive,
            2 => CloudProvider.Seedbox,
            _ => CloudProvider.BackblazeB2
        };
    }

    private void UpdateSaveMountButton()
    {
        BtnSaveMount.IsEnabled = HasAnyCompleteMount(includeSelectedGoogleDrive: true);
    }

    private bool HasAnyCompleteMount(bool includeSelectedGoogleDrive)
    {
        var hasB2 = BucketsPanel.Children.OfType<StackPanel>().Any(row =>
        {
            var textBoxes = row.Children.OfType<System.Windows.Controls.TextBox>().ToList();
            return textBoxes.Count >= 2 &&
                   !string.IsNullOrWhiteSpace(textBoxes[0].Text) &&
                   !string.IsNullOrWhiteSpace(textBoxes[1].Text);
        });

        var googleDrive = ReadGoogleDriveSettings();
        var hasGoogleDrive = _rcloneManager.IsGoogleDriveConfigured(googleDrive) ||
                             (includeSelectedGoogleDrive && GetSelectedProvider() == CloudProvider.GoogleDrive);

        var seedbox = ReadSeedboxSettings();
        var hasSeedbox = HasCompleteSeedboxSettings(seedbox) &&
                         (_rcloneManager.IsSeedboxConfigured(seedbox) ||
                          !string.IsNullOrEmpty(PwdSeedboxPassword.Password) ||
                          !string.IsNullOrEmpty(WindowsSecureStore.LoadSeedboxPassword()) ||
                          GetSelectedProvider() == CloudProvider.Seedbox);

        return hasB2 || hasGoogleDrive || hasSeedbox;
    }

    private bool SaveSettings(bool validateMounts, bool logSuccess = true)
    {
        try
        {
            NormalizeSeedboxHostInUi();
            _settings.SelectedProvider = GetSelectedProvider();
            WindowsSecureStore.SaveB2Credentials(TxtKeyId.Text.Trim(), TxtKey.Text.Trim());
            if (!string.IsNullOrEmpty(PwdSeedboxPassword.Password))
                WindowsSecureStore.SaveSeedboxPassword(PwdSeedboxPassword.Password);
            _settings.ApplicationKeyId = string.Empty;
            _settings.ApplicationKey = string.Empty;
            _settings.Buckets = validateMounts ? CollectBucketMounts(requireAtLeastOne: false) : ReadBucketRows();
            _settings.GoogleDrive = ReadGoogleDriveSettings();
            _settings.Seedbox = ReadSeedboxSettings();
            _settings.StartOnLogin = ChkStartOnLogin.IsChecked == true;
            _settings.StartMinimized = ChkStartMinimized.IsChecked == true;
            EnsureGoogleDriveDefaults();
            EnsureSeedboxDefaults();

            if (validateMounts)
            {
                ValidateGoogleDriveSettings(_settings.GoogleDrive, requireCompleteMount: false);
                ValidateSeedboxSettings(_settings.Seedbox, requireCompleteMount: false);
                ValidateDriveConflicts(_settings.Buckets, _settings.GoogleDrive, _settings.Seedbox);
            }

            _settingsService.Save(_settings);

            var exePath = Process.GetCurrentProcess().MainModule?.FileName ?? string.Empty;
            if (_settings.StartOnLogin && !string.IsNullOrEmpty(exePath))
                StartupManager.Set(exePath);
            else
                StartupManager.Unset();

            if (logSuccess)
            {
                AddLog("[INFO] Saved settings");
                LogService.Info("Settings saved. Provider=" + _settings.SelectedProvider + " BucketCount=" + _settings.Buckets.Count + " GoogleDrive=" + _settings.GoogleDrive.RemoteName + " Seedbox=" + _settings.Seedbox.RemoteName + " StartOnLogin=" + _settings.StartOnLogin);
            }

            return true;
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return false;
        }
    }

    private List<BucketMount> ReadBucketRows()
    {
        var buckets = new List<BucketMount>();

        foreach (StackPanel row in BucketsPanel.Children.OfType<StackPanel>())
        {
            var textBoxes = row.Children.OfType<System.Windows.Controls.TextBox>().ToList();
            if (textBoxes.Count < 2)
                continue;

            var bucketText = textBoxes[0].Text.Trim();
            var driveText = NormalizeDriveInput(textBoxes[1].Text);

            if (string.IsNullOrWhiteSpace(bucketText) && string.IsNullOrWhiteSpace(driveText))
                continue;

            buckets.Add(new BucketMount { BucketName = bucketText, DriveLetter = driveText });
        }

        return buckets;
    }

    private List<BucketMount> CollectBucketMounts(bool requireAtLeastOne)
    {
        var buckets = new List<BucketMount>();
        var seenDrives = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var seenBuckets = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var configuredDrives = _settings.Buckets
            .Select(bucket => NormalizeDriveInput(bucket.DriveLetter) + ":")
            .Where(drive => drive.Length == 2)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        var configuredGoogleDrive = NormalizeDriveInput(_settings.GoogleDrive?.DriveLetter ?? string.Empty);
        if (!string.IsNullOrWhiteSpace(configuredGoogleDrive))
            configuredDrives.Add(configuredGoogleDrive + ":");
        var configuredSeedboxDrive = NormalizeDriveInput(_settings.Seedbox?.DriveLetter ?? string.Empty);
        if (!string.IsNullOrWhiteSpace(configuredSeedboxDrive))
            configuredDrives.Add(configuredSeedboxDrive + ":");

        var usedSystemDrives = DriveInfo.GetDrives()
            .Where(drive => drive.Name.Length >= 2)
            .Select(drive => drive.Name.Substring(0, 2).ToUpperInvariant())
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        foreach (StackPanel row in BucketsPanel.Children.OfType<StackPanel>())
        {
            var bucketText = row.Children.OfType<System.Windows.Controls.TextBox>().ElementAt(0).Text.Trim();
            var driveInput = row.Children.OfType<System.Windows.Controls.TextBox>().ElementAt(1);
            var driveText = NormalizeDriveInput(driveInput.Text);

            if (string.IsNullOrWhiteSpace(bucketText) && string.IsNullOrWhiteSpace(driveText))
                continue;

            if (string.IsNullOrWhiteSpace(bucketText))
                throw new InvalidOperationException("Bucket name is required for every mount row.");

            if (string.IsNullOrWhiteSpace(driveText))
                throw new InvalidOperationException("Drive letter is required for bucket '" + bucketText + "'.");

            if (driveText.Length != 1 || !char.IsLetter(driveText[0]))
                throw new InvalidOperationException("Drive letter for bucket '" + bucketText + "' must be a single letter, like Z.");

            if (CloudProvider.IsReservedGoogleDriveLetter(driveText))
                throw new InvalidOperationException("Drive letter G: is reserved for Google Drive. Choose another drive letter for bucket '" + bucketText + "'.");

            var drive = driveText + ":";
            if (!seenDrives.Add(drive))
                throw new InvalidOperationException("Drive letter " + drive + " is used more than once.");

            if (usedSystemDrives.Contains(drive) && !configuredDrives.Contains(drive))
                throw new InvalidOperationException("Drive letter " + drive + " is already in use by Windows.");

            if (!seenBuckets.Add(bucketText))
                throw new InvalidOperationException("Bucket '" + bucketText + "' is listed more than once.");

            driveInput.Text = driveText;
            buckets.Add(new BucketMount { BucketName = bucketText, DriveLetter = drive });
        }

        if (requireAtLeastOne && buckets.Count == 0)
            throw new InvalidOperationException("Add at least one bucket and drive letter before mounting.");

        return buckets;
    }

    private GoogleDriveSettings ReadGoogleDriveSettings()
    {
        return new GoogleDriveSettings
        {
            RemoteName = CloudProvider.DefaultGoogleDriveRemoteName,
            RemotePath = NormalizeGoogleDrivePath(TxtGoogleRemotePath.Text),
            RootFolderId = TxtGoogleRootFolderId.Text.Trim(),
            DriveLetter = CloudProvider.DefaultGoogleDriveLetter
        };
    }

    private SeedboxSettings ReadSeedboxSettings()
    {
        var port = int.TryParse(TxtSeedboxPort.Text.Trim(), out var parsedPort) ? parsedPort : 21;
        return new SeedboxSettings
        {
            RemoteName = CloudProvider.DefaultSeedboxRemoteName,
            Host = CloudProvider.NormalizeSeedboxHost(TxtSeedboxHost.Text),
            Username = TxtSeedboxUsername.Text.Trim(),
            Port = port,
            RemotePath = NormalizeRemotePath(TxtSeedboxRemotePath.Text),
            DriveLetter = NormalizeDriveInput(TxtSeedboxDriveLetter.Text),
            ReadOnly = ChkSeedboxReadOnly.IsChecked != false,
            AllowUnverifiedCertificate = ChkSeedboxAllowUnverified.IsChecked != false
        };
    }

    private void ValidateGoogleDriveSettings(GoogleDriveSettings googleDrive, bool requireCompleteMount)
    {
        googleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        googleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;

        var hasOptionalValues = !string.IsNullOrWhiteSpace(googleDrive.RemotePath) ||
                                !string.IsNullOrWhiteSpace(googleDrive.RootFolderId);

        if (!hasOptionalValues && !requireCompleteMount)
            return;

        if (googleDrive.DriveLetter.Length != 1 || !char.IsLetter(googleDrive.DriveLetter[0]))
        {
            throw new InvalidOperationException("Google Drive letter must be a single letter, like G.");
        }
    }

    private void ValidateSeedboxSettings(SeedboxSettings seedbox, bool requireCompleteMount)
    {
        seedbox.RemoteName = CloudProvider.DefaultSeedboxRemoteName;
        seedbox.RemotePath = NormalizeRemotePath(seedbox.RemotePath);
        seedbox.DriveLetter = NormalizeDriveInput(seedbox.DriveLetter);

        var hasAnyInput = HasAnySeedboxInput(seedbox);
        if (!hasAnyInput && !requireCompleteMount)
            return;

        if (string.IsNullOrWhiteSpace(seedbox.Host))
            throw new InvalidOperationException("Seedbox host is required.");

        if (string.IsNullOrWhiteSpace(seedbox.Username))
            throw new InvalidOperationException("Seedbox username is required.");

        if (seedbox.Port <= 0 || seedbox.Port > 65535)
            throw new InvalidOperationException("Seedbox port must be between 1 and 65535.");

        if (string.IsNullOrWhiteSpace(seedbox.DriveLetter))
            throw new InvalidOperationException("Seedbox drive letter is required.");

        if (seedbox.DriveLetter.Length != 1 || !char.IsLetter(seedbox.DriveLetter[0]))
            throw new InvalidOperationException("Seedbox drive letter must be a single letter, like S.");
    }

    private static bool HasAnySeedboxInput(SeedboxSettings seedbox)
    {
        return !string.IsNullOrWhiteSpace(seedbox.Host) ||
               !string.IsNullOrWhiteSpace(seedbox.Username);
    }

    private static bool HasCompleteSeedboxSettings(SeedboxSettings seedbox)
    {
        return !string.IsNullOrWhiteSpace(seedbox.Host) &&
               !string.IsNullOrWhiteSpace(seedbox.Username) &&
               !string.IsNullOrWhiteSpace(seedbox.DriveLetter) &&
               seedbox.Port > 0 &&
               seedbox.Port <= 65535;
    }

    private void ValidateDriveConflicts(List<BucketMount> buckets, GoogleDriveSettings googleDrive, SeedboxSettings seedbox)
    {
        var seenDrives = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var configuredDrives = _settings.Buckets
            .Select(bucket => NormalizeDriveInput(bucket.DriveLetter) + ":")
            .Where(drive => drive.Length == 2)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        var configuredGoogleDrive = NormalizeDriveInput(_settings.GoogleDrive?.DriveLetter ?? string.Empty);
        if (!string.IsNullOrWhiteSpace(configuredGoogleDrive))
            configuredDrives.Add(configuredGoogleDrive + ":");
        var configuredSeedboxDrive = NormalizeDriveInput(_settings.Seedbox?.DriveLetter ?? string.Empty);
        if (!string.IsNullOrWhiteSpace(configuredSeedboxDrive))
            configuredDrives.Add(configuredSeedboxDrive + ":");

        var usedSystemDrives = DriveInfo.GetDrives()
            .Where(drive => drive.Name.Length >= 2)
            .Select(drive => drive.Name.Substring(0, 2).ToUpperInvariant())
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        foreach (var bucket in buckets)
        {
            var drive = NormalizeDriveInput(bucket.DriveLetter);
            if (string.IsNullOrWhiteSpace(drive))
                continue;

            var driveWithColon = drive + ":";
            if (!seenDrives.Add(driveWithColon))
                throw new InvalidOperationException("Drive letter " + driveWithColon + " is used more than once.");
        }

        var googleDriveLetter = NormalizeDriveInput(googleDrive.DriveLetter);
        if (!string.IsNullOrWhiteSpace(googleDriveLetter))
        {
            var driveWithColon = googleDriveLetter + ":";
            if (!seenDrives.Add(driveWithColon))
                throw new InvalidOperationException("Drive letter " + driveWithColon + " is used by both B2 and Google Drive.");

            if (usedSystemDrives.Contains(driveWithColon) && !configuredDrives.Contains(driveWithColon))
                throw new InvalidOperationException("Drive letter " + driveWithColon + " is already in use by Windows.");
        }

        var seedboxDrive = NormalizeDriveInput(seedbox.DriveLetter);
        if (!string.IsNullOrWhiteSpace(seedboxDrive) && HasAnySeedboxInput(seedbox))
        {
            var driveWithColon = seedboxDrive + ":";
            if (!seenDrives.Add(driveWithColon))
                throw new InvalidOperationException("Drive letter " + driveWithColon + " is used by more than one mount.");

            if (usedSystemDrives.Contains(driveWithColon) && !configuredDrives.Contains(driveWithColon))
                throw new InvalidOperationException("Drive letter " + driveWithColon + " is already in use by Windows.");
        }
    }

    private void ValidateMountRequest()
    {
        var b2HasAny = _settings.Buckets.Count > 0;

        var b2Credentials = WindowsSecureStore.LoadB2Credentials();
        var b2Complete = !string.IsNullOrWhiteSpace(b2Credentials?.ApplicationKeyId) &&
                         !string.IsNullOrWhiteSpace(b2Credentials?.ApplicationKey) &&
                         _settings.Buckets.Any(bucket => !string.IsNullOrWhiteSpace(bucket.BucketName) && !string.IsNullOrWhiteSpace(bucket.DriveLetter));

        if (b2HasAny && !b2Complete)
            throw new InvalidOperationException("B2 requires an Application Key ID, Application Key, and at least one complete bucket mount row.");

        var googleComplete = !string.IsNullOrWhiteSpace(_settings.GoogleDrive.RemoteName) &&
                             _rcloneManager.IsGoogleDriveConfigured(_settings.GoogleDrive);
        var googleSelected = _settings.SelectedProvider == CloudProvider.GoogleDrive;

        if (googleSelected && !googleComplete)
            throw new InvalidOperationException("Google Drive is not connected. Click Connect Google Drive first.");

        if (googleComplete)
            ValidateGoogleDriveSettings(_settings.GoogleDrive, requireCompleteMount: true);

        var seedboxSelected = _settings.SelectedProvider == CloudProvider.Seedbox;
        var seedboxHasAny = HasAnySeedboxInput(_settings.Seedbox);
        var seedboxComplete = HasCompleteSeedboxSettings(_settings.Seedbox) &&
                              (_rcloneManager.IsSeedboxConfigured(_settings.Seedbox) ||
                               !string.IsNullOrEmpty(PwdSeedboxPassword.Password) ||
                               !string.IsNullOrEmpty(WindowsSecureStore.LoadSeedboxPassword()));

        if ((seedboxSelected || seedboxHasAny) && !seedboxComplete)
            throw new InvalidOperationException("Seedbox requires host, username, port, drive letter, and a saved FTPS password. Click Test Connection first.");

        if (seedboxComplete)
            ValidateSeedboxSettings(_settings.Seedbox, requireCompleteMount: true);

        if (!b2Complete && !googleComplete && !seedboxComplete)
            throw new InvalidOperationException("Configure at least one B2 bucket, Google Drive, or Seedbox mount before mounting.");
    }

    private void RefreshGoogleDriveConnectionUi()
    {
        if (!IsInitialized)
            return;

        var googleDrive = ReadGoogleDriveSettings();
        var connected = _rcloneManager.IsGoogleDriveConfigured(googleDrive);

        TxtGoogleConnectHelp.Visibility = connected ? Visibility.Collapsed : Visibility.Visible;
        BtnConnectGoogleDrive.Content = connected ? "Disconnect Google Drive" : "Connect Google Drive";
        BtnTestGoogleDriveConnection.Visibility = connected ? Visibility.Visible : Visibility.Collapsed;
    }

    private void RefreshSeedboxConnectionUi()
    {
        if (!IsInitialized)
            return;

        var seedbox = ReadSeedboxSettings();
        var connected = _rcloneManager.IsSeedboxConfigured(seedbox);
        BtnForgetSeedbox.Visibility = connected ? Visibility.Visible : Visibility.Collapsed;
        TxtSeedboxConnectHelp.Text = connected
            ? "Seedbox FTPS is configured. Use Save and Mount All to mount it."
            : "Use your Ultra.cc FTP/SFTP connection details. Host is usually your server name, port is 21, and Remote Folder is usually downloads.";
    }

    private void EnsureGoogleDriveDefaults()
    {
        _settings.GoogleDrive ??= new GoogleDriveSettings();
        _settings.GoogleDrive.RemoteName = CloudProvider.DefaultGoogleDriveRemoteName;
        _settings.GoogleDrive.DriveLetter = CloudProvider.DefaultGoogleDriveLetter;
    }

    private void NormalizeSeedboxHostInUi()
    {
        var normalized = CloudProvider.NormalizeSeedboxHost(TxtSeedboxHost.Text);
        if (!string.Equals(TxtSeedboxHost.Text, normalized, StringComparison.Ordinal))
            TxtSeedboxHost.Text = normalized;
    }

    private void EnsureSeedboxDefaults()
    {
        _settings.Seedbox ??= new SeedboxSettings();
        _settings.Seedbox.RemoteName = CloudProvider.DefaultSeedboxRemoteName;
        _settings.Seedbox.Host = CloudProvider.NormalizeSeedboxHost(_settings.Seedbox.Host);
        _settings.Seedbox.RemotePath = NormalizeRemotePath(_settings.Seedbox.RemotePath);
        if (_settings.Seedbox.Port <= 0 || _settings.Seedbox.Port > 65535)
            _settings.Seedbox.Port = 21;
        if (string.IsNullOrWhiteSpace(_settings.Seedbox.DriveLetter))
            _settings.Seedbox.DriveLetter = CloudProvider.DefaultSeedboxLetter;
        _settings.Seedbox.DriveLetter = NormalizeDriveInput(_settings.Seedbox.DriveLetter);
    }

    private static string NormalizeGoogleDrivePath(string value)
    {
        return NormalizeRemotePath(value);
    }

    private static string NormalizeRemotePath(string value)
    {
        var path = value.Trim().Replace('\\', '/');
        while (path.StartsWith("/"))
            path = path[1..];
        while (path.StartsWith(":"))
            path = path[1..];

        return path;
    }

    private static string NormalizeDriveInput(string value)
    {
        return CloudProvider.NormalizeDriveLetterInput(value);
    }

    private void BtnAddBucket_Click(object sender, RoutedEventArgs e)
    {
        AddBucketRow();
        UpdateSaveMountButton();
    }

    private void SeedboxPassword_Changed(object sender, RoutedEventArgs e)
    {
        if (!IsInitialized)
            return;

        UpdateSaveMountButton();
    }

    private void AnySeedboxOption_Changed(object sender, RoutedEventArgs e)
    {
        if (!IsInitialized)
            return;

        UpdateSaveMountButton();
    }

    private async void BtnConnectGoogleDrive_Click(object sender, RoutedEventArgs e)
    {
        _settings.SelectedProvider = CloudProvider.GoogleDrive;
        _settings.GoogleDrive = ReadGoogleDriveSettings();
        EnsureGoogleDriveDefaults();

        try
        {
            ValidateGoogleDriveSettings(_settings.GoogleDrive, requireCompleteMount: false);
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return;
        }

        SaveSettings(validateMounts: false, logSuccess: false);

        var isConnected = _rcloneManager.IsGoogleDriveConfigured(_settings.GoogleDrive);

        BtnConnectGoogleDrive.IsEnabled = false;
        BtnTestGoogleDriveConnection.IsEnabled = false;
        BtnConnectGoogleDrive.Content = isConnected ? "Disconnecting..." : "Connecting...";

        try
        {
            if (isConnected)
            {
                var driveToUnmount = NormalizeDriveInput(_settings.GoogleDrive.DriveLetter);
                if (!string.IsNullOrWhiteSpace(driveToUnmount))
                    _rcloneManager.UnmountDrive(driveToUnmount);

                AddLog("[INFO] Disconnecting Google Drive.");
                var ok = await Task.Run(() => _rcloneManager.DisconnectGoogleDrive(_settings.GoogleDrive));
                if (ok)
                {
                    AddLog("[INFO] Google Drive is disconnected.");
                    LogService.Info("Google Drive disconnected. Remote=" + _settings.GoogleDrive.RemoteName);
                }
                else
                {
                    AddLog("[ERROR] Google Drive disconnect failed. See the log above for details.");
                    LogService.Error("Google Drive disconnect failed.");
                }
            }
            else
            {
                AddLog("[INFO] Starting Google Drive authorization. Complete the sign-in in your browser.");
                var ok = await Task.Run(() => _rcloneManager.ConfigureGoogleDrive(_settings.GoogleDrive));
                if (ok)
                {
                    AddLog("[INFO] Google Drive is connected. You can now click Save and Mount All.");
                    LogService.Info("Google Drive connected. Remote=" + _settings.GoogleDrive.RemoteName);
                }
                else
                {
                    AddLog("[ERROR] Google Drive authorization failed. See the log above for details.");
                    LogService.Error("Google Drive authorization failed.");
                }
            }
        }
        finally
        {
            BtnConnectGoogleDrive.IsEnabled = true;
            BtnTestGoogleDriveConnection.IsEnabled = true;
            RefreshGoogleDriveConnectionUi();
        }
    }

    private async void BtnTestGoogleDriveConnection_Click(object sender, RoutedEventArgs e)
    {
        _settings.SelectedProvider = CloudProvider.GoogleDrive;
        _settings.GoogleDrive = ReadGoogleDriveSettings();
        EnsureGoogleDriveDefaults();

        if (!SaveSettings(validateMounts: false, logSuccess: false))
            return;

        if (!_rcloneManager.IsGoogleDriveConfigured(_settings.GoogleDrive))
        {
            AddLog("[ERROR] Google Drive is not connected. Click Connect Google Drive first.");
            RefreshGoogleDriveConnectionUi();
            return;
        }

        BtnConnectGoogleDrive.IsEnabled = false;
        BtnTestGoogleDriveConnection.IsEnabled = false;
        BtnTestGoogleDriveConnection.Content = "Testing...";
        AddLog("[INFO] Testing Google Drive connection.");

        try
        {
            var ok = await Task.Run(() => _rcloneManager.TestGoogleDriveConnection(_settings.GoogleDrive));
            if (ok)
            {
                AddLog("[INFO] Google Drive connection test succeeded.");
                LogService.Info("Google Drive connection test succeeded.");
            }
            else
            {
                AddLog("[ERROR] Google Drive connection test failed. See the log above for details.");
                LogService.Error("Google Drive connection test failed.");
            }
        }
        finally
        {
            BtnConnectGoogleDrive.IsEnabled = true;
            BtnTestGoogleDriveConnection.IsEnabled = true;
            BtnTestGoogleDriveConnection.Content = "Test Connection";
            RefreshGoogleDriveConnectionUi();
        }
    }

    private async void BtnTestSeedboxConnection_Click(object sender, RoutedEventArgs e)
    {
        NormalizeSeedboxHostInUi();
        _settings.SelectedProvider = CloudProvider.Seedbox;
        _settings.Seedbox = ReadSeedboxSettings();
        EnsureSeedboxDefaults();

        try
        {
            ValidateSeedboxSettings(_settings.Seedbox, requireCompleteMount: true);
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return;
        }

        if (!SaveSettings(validateMounts: false, logSuccess: false))
            return;

        var password = !string.IsNullOrEmpty(PwdSeedboxPassword.Password)
            ? PwdSeedboxPassword.Password
            : WindowsSecureStore.LoadSeedboxPassword();

        BtnTestSeedboxConnection.IsEnabled = false;
        BtnForgetSeedbox.IsEnabled = false;
        BtnTestSeedboxConnection.Content = "Testing...";
        AddLog("[INFO] Testing Seedbox FTPS connection.");

        try
        {
            var seedbox = _settings.Seedbox;
            var ok = await Task.Run(() => _rcloneManager.TestSeedboxConnection(seedbox, password));
            if (ok)
            {
                if (!string.IsNullOrEmpty(PwdSeedboxPassword.Password))
                {
                    WindowsSecureStore.SaveSeedboxPassword(PwdSeedboxPassword.Password);
                    PwdSeedboxPassword.Clear();
                }

                AddLog("[INFO] Seedbox connection test succeeded.");
                LogService.Info("Seedbox connection test succeeded.");
            }
            else
            {
                AddLog("[ERROR] Seedbox connection test failed. See the log above for details.");
                LogService.Error("Seedbox connection test failed.");
            }
        }
        finally
        {
            BtnTestSeedboxConnection.IsEnabled = true;
            BtnForgetSeedbox.IsEnabled = true;
            BtnTestSeedboxConnection.Content = "Test Connection";
            RefreshSeedboxConnectionUi();
            UpdateSaveMountButton();
        }
    }

    private void BtnForgetSeedbox_Click(object sender, RoutedEventArgs e)
    {
        _settings.Seedbox = ReadSeedboxSettings();
        var driveToUnmount = NormalizeDriveInput(_settings.Seedbox.DriveLetter);
        if (!string.IsNullOrWhiteSpace(driveToUnmount))
            _rcloneManager.UnmountDrive(driveToUnmount);

        var ok = _rcloneManager.DisconnectSeedbox(_settings.Seedbox);
        WindowsSecureStore.DeleteSeedboxPassword();
        PwdSeedboxPassword.Clear();

        AddLog(ok ? "[INFO] Seedbox is disconnected." : "[ERROR] Seedbox disconnect failed. See the log above for details.");
        RefreshSeedboxConnectionUi();
        UpdateSaveMountButton();
    }

    private void BtnSaveMount_Click(object? sender, RoutedEventArgs? e)
    {
        if (!SaveSettings(validateMounts: true))
            return;

        try
        {
            ValidateMountRequest();
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return;
        }

        var ok = _rcloneManager.Mount(_settings);
        if (!ok)
        {
            AddLog("[ERROR] Mount failed to start. See log above for details.");
            LogService.Error("Mount failed to start.");
        }

        RefreshGoogleDriveConnectionUi();
        RefreshSeedboxConnectionUi();
    }

    private void BtnUnmount_Click(object sender, RoutedEventArgs e)
    {
        _rcloneManager.Unmount();
        AddLog("[INFO] Unmounted all.");
        LogService.Info("User clicked Unmount All.");
    }

    private void BtnOpenLogs_Click(object sender, RoutedEventArgs e)
    {
        var logDir = LogService.GetLogDirectory();
        try
        {
            Process.Start(new ProcessStartInfo("explorer.exe", logDir) { UseShellExecute = true });
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] Could not open log folder: " + ex.Message);
        }
    }

    private void BtnClearLogs_Click(object sender, RoutedEventArgs e)
    {
        LogService.Clear();
        TxtLog.Clear();
    }

    private void BtnRestart_Click(object sender, RoutedEventArgs e)
    {
        BtnRestart.IsEnabled = false;
        AddLog("[INFO] Restarting Cloud Drive Mount.");

        if (!SaveSettings(validateMounts: false, logSuccess: false))
        {
            BtnRestart.IsEnabled = true;
            return;
        }

        if (!StartRestartHelper())
        {
            BtnRestart.IsEnabled = true;
            return;
        }

        _rcloneManager.Unmount();
        _rcloneManager.CleanupExistingAppProcesses();
        TxtLog.Clear();

        AllowClose = true;
        System.Windows.Application.Current.Shutdown();
    }

    private bool StartRestartHelper()
    {
        var exePath = Process.GetCurrentProcess().MainModule?.FileName;
        if (string.IsNullOrWhiteSpace(exePath))
        {
            AddLog("[ERROR] Could not restart: current executable path was not found.");
            return false;
        }

        var command = "Wait-Process -Id " + Process.GetCurrentProcess().Id +
                      "; Start-Process -FilePath '" + exePath.Replace("'", "''") +
                      "' -ArgumentList '--show-window','--clean-restart'";

        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "powershell.exe",
                Arguments = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -Command " + QuoteProcessArgument(command),
                UseShellExecute = false,
                CreateNoWindow = true
            });

            return true;
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] Could not start restart helper: " + ex.Message);
            LogService.Error("Could not start restart helper: " + ex);
            return false;
        }
    }

    private static string QuoteProcessArgument(string value)
    {
        return "\"" + value.Replace("\"", "\\\"") + "\"";
    }

    private void CmbProvider_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_isLoadingProvider)
            return;

        UpdateProviderPanels();
        SaveSettings(validateMounts: false, logSuccess: false);
        RefreshGoogleDriveConnectionUi();
        RefreshSeedboxConnectionUi();
        UpdateSaveMountButton();
    }

    private void AnyMountField_Changed(object sender, TextChangedEventArgs e)
    {
        if (!IsInitialized)
            return;

        UpdateSaveMountButton();
    }

    private void Window_Closing(object? sender, System.ComponentModel.CancelEventArgs e)
    {
        if (!AllowClose)
        {
            SaveSettings(validateMounts: false, logSuccess: false);
            e.Cancel = true;
            Hide();
        }
    }

    private void Window_StateChanged(object? sender, EventArgs e)
    {
        if (WindowState == WindowState.Minimized)
        {
            SaveSettings(validateMounts: false, logSuccess: false);
            Hide();
        }
    }
}
