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

        if (!string.IsNullOrWhiteSpace(settings.ApplicationKeyId) && !string.IsNullOrWhiteSpace(settings.ApplicationKey))
        {
            _mainWindow.AttemptMount();
        }
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
