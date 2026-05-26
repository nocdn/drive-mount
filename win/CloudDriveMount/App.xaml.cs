using System.Linq;
using System.Windows;

namespace CloudDriveMount;

public partial class App : System.Windows.Application
{
    private TrayIconManager? _trayIcon;
    private MainWindow? _mainWindow;
    private static System.Threading.Mutex? _mutex;

    protected override void OnStartup(StartupEventArgs e)
    {
        const string mutexName = "CloudDriveMount_SingleInstance";
        _mutex = new System.Threading.Mutex(true, mutexName, out var createdNew);
        if (!createdNew)
        {
            System.Windows.MessageBox.Show("Cloud Drive Mount is already running.");
            Shutdown();
            return;
        }

        base.OnStartup(e);
        ShutdownMode = ShutdownMode.OnExplicitShutdown;

        _mainWindow = new MainWindow();
        _trayIcon = new TrayIconManager(_mainWindow);
        _mainWindow.SetTrayIcon(_trayIcon);

        _trayIcon.OnQuitRequested += () =>
        {
            _trayIcon?.Dispose();
            _mainWindow?.AllowCloseAndQuit();
            Shutdown();
        };

        var settings = new SettingsService().Load();
        var showWindow = e.Args.Contains("--show-window");
        if (showWindow || !settings.StartMinimized)
        {
            _mainWindow.Show();
        }

        if (HasAnyConfiguredMount(settings))
        {
            _mainWindow.AttemptMount();
        }
    }

    private static bool HasAnyConfiguredMount(AppSettings settings)
    {
        var hasB2Mounts = !string.IsNullOrWhiteSpace(settings.ApplicationKeyId) &&
                          !string.IsNullOrWhiteSpace(settings.ApplicationKey) &&
                          settings.Buckets.Any(bucket =>
                              !string.IsNullOrWhiteSpace(bucket.BucketName) &&
                              !string.IsNullOrWhiteSpace(bucket.DriveLetter));

        var hasGoogleDriveMount = settings.GoogleDrive is not null &&
                                  !string.IsNullOrWhiteSpace(settings.GoogleDrive.RemoteName) &&
                                  !string.IsNullOrWhiteSpace(settings.GoogleDrive.DriveLetter);

        return hasB2Mounts || hasGoogleDriveMount;
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _trayIcon?.Dispose();
        _mainWindow?.AllowCloseAndQuit();
        _mutex?.ReleaseMutex();
        _mutex?.Dispose();
        base.OnExit(e);
    }
}
