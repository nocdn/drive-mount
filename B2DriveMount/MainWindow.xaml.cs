using System.Diagnostics;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Forms;

namespace B2DriveMount;

public partial class MainWindow : Window
{
    private readonly SettingsService _settingsService;
    private readonly RcloneManager _rcloneManager;
    private AppSettings _settings;
    private TrayIconManager? _tray;

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
            _tray?.ShowBalloonTip("Cloud Drive Mount Error", msg, ToolTipIcon.Error);
        });

        _settings = _settingsService.Load();
        _settings.Buckets ??= new List<BucketMount>();

        TxtKeyId.Text = _settings.ApplicationKeyId;
        TxtKey.Text = _settings.ApplicationKey;
        ChkStartOnLogin.IsChecked = StartupManager.IsSet();
        ChkStartMinimized.IsChecked = _settings.StartMinimized;

        if (_settings.Buckets.Count == 0)
            AddBucketRow();
        else
            foreach (var bucket in _settings.Buckets)
                AddBucketRow(bucket.BucketName, bucket.DriveLetter);

        UpdateSaveMountButton();

        var logMsg = "Cloud Drive Mount started. Log file: " + LogService.GetLogFilePath();
        AddLog("[INFO] " + logMsg);
        LogService.Info(logMsg);
    }

    public void SetTrayIcon(TrayIconManager tray) => _tray = tray;

    public void AttemptMount()
    {
        if (!string.IsNullOrWhiteSpace(_settings.ApplicationKeyId) &&
            !string.IsNullOrWhiteSpace(_settings.ApplicationKey) &&
            _settings.Buckets.Count > 0)
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
        var driveText = new System.Windows.Controls.TextBox { Width = 35, Height = 23, MaxLength = 2, Text = NormalizeDriveInput(driveLetter), Margin = new Thickness(0, 0, 8, 0), Padding = new Thickness(3, 0, 3, 0), VerticalContentAlignment = VerticalAlignment.Center };
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

            SaveSettings();
            UpdateSaveMountButton();
        };
        saveButton.Click += (_, _) =>
        {
            if (SaveSettings())
            {
                rowSaved = true;
                savedDriveLetter = NormalizeDriveInput(driveText.Text);
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

    private void UpdateSaveMountButton()
    {
        BtnSaveMount.IsEnabled = BucketsPanel.Children.OfType<StackPanel>().Any(row =>
        {
            var textBoxes = row.Children.OfType<System.Windows.Controls.TextBox>().ToList();
            return textBoxes.Count >= 2 &&
                   !string.IsNullOrWhiteSpace(textBoxes[0].Text) &&
                   !string.IsNullOrWhiteSpace(textBoxes[1].Text);
        });
    }

    private bool SaveSettings()
    {
        try
        {
            _settings.ApplicationKeyId = TxtKeyId.Text.Trim();
            _settings.ApplicationKey = TxtKey.Text.Trim();
            _settings.Buckets = CollectBucketMounts(requireAtLeastOne: false);
            _settings.StartOnLogin = ChkStartOnLogin.IsChecked == true;
            _settings.StartMinimized = ChkStartMinimized.IsChecked == true;

            _settingsService.Save(_settings);

            var exePath = Process.GetCurrentProcess().MainModule?.FileName ?? string.Empty;
            if (_settings.StartOnLogin && !string.IsNullOrEmpty(exePath))
                StartupManager.Set(exePath);
            else
                StartupManager.Unset();

            AddLog("[INFO] Saved settings");
            LogService.Info("Settings saved. BucketCount=" + _settings.Buckets.Count + " StartOnLogin=" + _settings.StartOnLogin);
            return true;
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return false;
        }
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
        var usedSystemDrives = DriveInfo.GetDrives()
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

    private static string NormalizeDriveInput(string value)
    {
        var drive = value.Trim().ToUpperInvariant();
        if (drive.EndsWith(":"))
            drive = drive[..^1];

        return drive;
    }

    private void BtnAddBucket_Click(object sender, RoutedEventArgs e)
    {
        AddBucketRow();
        UpdateSaveMountButton();
    }

    private void BtnSaveMount_Click(object? sender, RoutedEventArgs? e)
    {
        try
        {
            _settings.Buckets = CollectBucketMounts(requireAtLeastOne: true);
        }
        catch (Exception ex)
        {
            AddLog("[ERROR] " + ex.Message);
            LogService.Error(ex.ToString());
            return;
        }

        if (SaveSettings())
        {
            var ok = _rcloneManager.Mount(_settings);
            if (!ok)
            {
                AddLog("[ERROR] Mount failed to start. See log above for details.");
                LogService.Error("Mount failed to start.");
            }
        }
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
        TxtLog.Clear();
    }

    private void Window_Closing(object? sender, System.ComponentModel.CancelEventArgs e)
    {
        if (!AllowClose)
        {
            e.Cancel = true;
            Hide();
        }
    }

    private void Window_StateChanged(object? sender, EventArgs e)
    {
        if (WindowState == WindowState.Minimized)
            Hide();
    }
}
