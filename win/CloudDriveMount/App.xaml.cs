using System.Threading;
using System.Threading.Tasks;
using System.Windows;

namespace CloudDriveMount;

public partial class App : System.Windows.Application
{
    private const string MutexName = "CloudDriveMount_SingleInstance";
    private const string ShowSettingsEventName = "CloudDriveMount_ShowSettings";

    private TrayIconManager? _trayIcon;
    private MainWindow? _mainWindow;
    private static System.Threading.Mutex? _mutex;
    private EventWaitHandle? _showSettingsEvent;
    private CancellationTokenSource? _showSettingsCancellation;
    private bool _ownsMutex;

    protected override void OnStartup(StartupEventArgs e)
    {
        _mutex = new System.Threading.Mutex(true, MutexName, out var createdNew);
        _ownsMutex = createdNew;
        if (!createdNew)
        {
            SignalRunningInstanceToShowSettings();
            Shutdown();
            return;
        }

        base.OnStartup(e);
        ShutdownMode = ShutdownMode.OnExplicitShutdown;

        if (e.Args.Contains("--clean-restart"))
            LogService.Clear();

        _mainWindow = new MainWindow();
        _trayIcon = new TrayIconManager(_mainWindow);
        _mainWindow.SetTrayIcon(_trayIcon);
        StartShowSettingsListener();

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

        _mainWindow.CleanupExistingAppProcesses();
        _mainWindow.AttemptMount();
    }

    private static void SignalRunningInstanceToShowSettings()
    {
        try
        {
            using var showSettingsEvent = EventWaitHandle.OpenExisting(ShowSettingsEventName);
            showSettingsEvent.Set();
        }
        catch
        {
            // The mutex is authoritative; if the signal endpoint is gone, this short-lived instance still exits quietly.
        }
    }

    private void StartShowSettingsListener()
    {
        _showSettingsEvent = new EventWaitHandle(false, EventResetMode.AutoReset, ShowSettingsEventName);
        _showSettingsCancellation = new CancellationTokenSource();
        var token = _showSettingsCancellation.Token;

        Task.Run(() =>
        {
            while (!token.IsCancellationRequested)
            {
                try
                {
                    if (_showSettingsEvent.WaitOne(250))
                        Dispatcher.BeginInvoke(() => _mainWindow?.ShowSettingsWindow());
                }
                catch (ObjectDisposedException)
                {
                    break;
                }
            }
        }, token);
    }

    protected override void OnExit(ExitEventArgs e)
    {
        _showSettingsCancellation?.Cancel();
        _showSettingsEvent?.Set();
        _showSettingsEvent?.Dispose();
        _showSettingsCancellation?.Dispose();
        _trayIcon?.Dispose();
        _mainWindow?.AllowCloseAndQuit();
        if (_ownsMutex)
            _mutex?.ReleaseMutex();
        _mutex?.Dispose();
        base.OnExit(e);
    }
}
