using System.Windows;
using System.Windows.Forms;

namespace CloudDriveMount;

public class TrayIconManager : IDisposable
{
    private readonly NotifyIcon _notifyIcon;
    private readonly MainWindow _window;

    public event Action? OnQuitRequested;

    public TrayIconManager(MainWindow window)
    {
        _window = window;
        _notifyIcon = new NotifyIcon
        {
            Icon = SystemIcons.Application,
            Text = "Cloud Drive Mount",
            Visible = true,
        };

        var contextMenu = new ContextMenuStrip();
        var settingsItem = new ToolStripMenuItem("Settings");
        settingsItem.Click += (s, e) =>
        {
            _window.Dispatcher.Invoke(() =>
            {
                _window.Show();
                _window.WindowState = WindowState.Normal;
                _window.Activate();
            });
        };

        var quitItem = new ToolStripMenuItem("Quit");
        quitItem.Click += (s, e) => OnQuitRequested?.Invoke();

        contextMenu.Items.Add(settingsItem);
        contextMenu.Items.Add(new ToolStripSeparator());
        contextMenu.Items.Add(quitItem);

        _notifyIcon.ContextMenuStrip = contextMenu;
        _notifyIcon.DoubleClick += (s, e) =>
        {
            _window.Dispatcher.Invoke(() =>
            {
                _window.Show();
                _window.WindowState = WindowState.Normal;
                _window.Activate();
            });
        };
    }

    public void ShowBalloonTip(string title, string message, ToolTipIcon icon)
    {
        _notifyIcon.ShowBalloonTip(3000, title, message, icon);
    }

    public void Dispose()
    {
        _notifyIcon.Visible = false;
        _notifyIcon.Dispose();
    }
}
