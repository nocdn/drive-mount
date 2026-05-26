import AppKit
import ServiceManagement

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let rcloneManager = RcloneManager()
    private var statusItem: NSStatusItem?
    private var settingsWindowController: SettingsWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        RuntimeLog.info("applicationDidFinishLaunching")
        AppPreferences.registerDefaults()
        RuntimeLog.info("Preferences loaded. startAtLogin=\(AppPreferences.startAtLogin) startMinimized=\(AppPreferences.startMinimized)")
        setupMainMenu()
        setupStatusItem()

        applyStartAtLoginPreference(showErrors: false)

        if !AppPreferences.startMinimized {
            showSettings()
        } else {
            RuntimeLog.info("Start minimized enabled; not showing settings window on launch")
        }

        if !RcloneManager.isMacFuseInstalled() {
            RuntimeLog.info("macFUSE not detected on launch; showing instructions")
            showMacFuseInstructions()
        } else {
            RuntimeLog.info("macFUSE detected on launch")
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        RuntimeLog.info("applicationShouldTerminate requested")
        rcloneManager.unmountAll()
        rcloneManager.cleanupTemporaryFiles()
        RuntimeLog.info("applicationShouldTerminate completed cleanup")
        return .terminateNow
    }

    private func setupMainMenu() {
        RuntimeLog.info("Setting up main menu")

        let mainMenu = NSMenu()

        let appMenuItem = NSMenuItem()
        mainMenu.addItem(appMenuItem)
        let appMenu = NSMenu()
        appMenuItem.submenu = appMenu
        appMenu.addItem(NSMenuItem(title: "Quit Cloud Drive Mount", action: #selector(quitMenuItem), keyEquivalent: "q"))

        let editMenuItem = NSMenuItem()
        mainMenu.addItem(editMenuItem)
        let editMenu = NSMenu(title: "Edit")
        editMenuItem.submenu = editMenu
        editMenu.addItem(NSMenuItem(title: "Undo", action: Selector(("undo:")), keyEquivalent: "z"))
        editMenu.addItem(NSMenuItem(title: "Redo", action: Selector(("redo:")), keyEquivalent: "Z"))
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(NSMenuItem(title: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x"))
        editMenu.addItem(NSMenuItem(title: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c"))
        editMenu.addItem(NSMenuItem(title: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v"))
        editMenu.addItem(NSMenuItem(title: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a"))

        NSApp.mainMenu = mainMenu
    }

    private func setupStatusItem() {
        RuntimeLog.info("Setting up status item")
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = item.button {
            button.image = NSImage(systemSymbolName: "externaldrive.badge.icloud", accessibilityDescription: "Cloud Drive Mount")
            button.image?.isTemplate = true
            if button.image == nil {
                button.title = "CDM"
            }
        }

        let menu = NSMenu()
        let settingsItem = NSMenuItem(title: "Settings", action: #selector(showSettingsMenuItem), keyEquivalent: ",")
        settingsItem.target = self
        menu.addItem(settingsItem)
        menu.addItem(NSMenuItem.separator())
        let quitItem = NSMenuItem(title: "Quit", action: #selector(quitMenuItem), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)
        item.menu = menu
        statusItem = item
        RuntimeLog.info("Status item configured")
    }

    @objc private func showSettingsMenuItem() {
        RuntimeLog.info("Settings menu item selected")
        showSettings()
    }

    @objc private func quitMenuItem() {
        RuntimeLog.info("Quit menu item selected")
        NSApp.terminate(nil)
    }

    private func showSettings() {
        RuntimeLog.info("showSettings called. existingController=\(settingsWindowController != nil)")
        if settingsWindowController == nil {
            RuntimeLog.info("Creating SettingsWindowController")
            let controller = SettingsWindowController(rcloneManager: rcloneManager)
            controller.onMacFuseHelpRequested = { [weak self] in
                RuntimeLog.info("macFUSE help requested from settings window")
                self?.showMacFuseInstructions()
            }
            settingsWindowController = controller
            RuntimeLog.info("SettingsWindowController created")
        }

        settingsWindowController?.showWindow(nil)
        settingsWindowController?.window?.center()
        settingsWindowController?.window?.makeKeyAndOrderFront(nil)
        settingsWindowController?.window?.orderFrontRegardless()
        NSApp.activate(ignoringOtherApps: true)
        RuntimeLog.info("showSettings completed. visible=\(settingsWindowController?.window?.isVisible == true) frame=\(String(describing: settingsWindowController?.window?.frame))")
    }

    private func showMacFuseInstructions() {
        RuntimeLog.info("Showing macFUSE instructions alert")
        let alert = NSAlert()
        alert.messageText = "macFUSE is required"
        alert.informativeText = "Cloud Drive Mount uses rclone mount, which needs macFUSE on macOS. Install macFUSE, then open System Settings > Privacy & Security and allow the system extension if macOS asks.\n\nOn Apple Silicon, if macOS blocks kernel extensions completely, restart into Recovery, open Startup Security Utility, choose Reduced Security, and enable user management of kernel extensions from identified developers."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Download macFUSE")
        alert.addButton(withTitle: "OK")

        let response = alert.runModal()
        RuntimeLog.info("macFUSE instructions alert dismissed response=\(response.rawValue)")
        if response == .alertFirstButtonReturn,
           let url = URL(string: "https://macfuse.github.io/") {
            NSWorkspace.shared.open(url)
        }
    }

    func setStartAtLogin(_ enabled: Bool) {
        AppPreferences.startAtLogin = enabled
        applyStartAtLoginPreference(showErrors: true)
    }

    private func applyStartAtLoginPreference(showErrors: Bool) {
        do {
            if AppPreferences.startAtLogin {
                if SMAppService.mainApp.status != .enabled {
                    RuntimeLog.info("Registering main app as login item. status=\(SMAppService.mainApp.status.rawValue)")
                    try SMAppService.mainApp.register()
                }
            } else if SMAppService.mainApp.status == .enabled {
                RuntimeLog.info("Unregistering main app login item")
                try SMAppService.mainApp.unregister()
            }

            RuntimeLog.info("Login item preference applied. enabled=\(AppPreferences.startAtLogin) status=\(SMAppService.mainApp.status.rawValue)")
        } catch {
            RuntimeLog.error("Failed to apply login item preference: \(error.localizedDescription)")
            if showErrors {
                let alert = NSAlert()
                alert.messageText = "Could not update Start at login"
                alert.informativeText = error.localizedDescription
                alert.alertStyle = .warning
                alert.addButton(withTitle: "OK")
                alert.addButton(withTitle: "Open Login Items Settings")
                if alert.runModal() == .alertSecondButtonReturn {
                    SMAppService.openSystemSettingsLoginItems()
                }
            }
        }
    }
}
