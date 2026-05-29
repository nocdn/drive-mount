import AppKit
import ServiceManagement

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let rcloneManager = RcloneManager()
    private let singleInstanceCoordinator = SingleInstanceCoordinator()
    private let errorNotifications = ErrorNotificationManager()
    private var statusItem: NSStatusItem?
    private var settingsWindowController: SettingsWindowController?
    private var isRestarting = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        RuntimeLog.info("applicationDidFinishLaunching")
        let launchArguments = Set(CommandLine.arguments.dropFirst())

        AppPreferences.registerDefaults()
        guard singleInstanceCoordinator.claimOrSignalRunningInstance() else {
            RuntimeLog.info("Duplicate instance forwarded Settings request and will terminate.")
            NSApp.terminate(nil)
            return
        }

        singleInstanceCoordinator.listenForShowSettingsRequests { [weak self] in
            self?.showSettings()
        }

        rcloneManager.onError = { [weak self] message in
            self?.errorNotifications.showError(message)
        }

        RuntimeLog.info("Preferences loaded. startAtLogin=\(AppPreferences.startAtLogin) startMinimized=\(AppPreferences.startMinimized)")
        setupMainMenu()
        setupStatusItem()

        applyStartAtLoginPreference(showErrors: false)

        if launchArguments.contains("--show-settings") || !AppPreferences.startMinimized {
            showSettings()
        } else {
            RuntimeLog.info("Start minimized enabled; not showing settings window on launch")
        }

        rcloneManager.cleanupExistingAppProcesses()

        let macFuseInstalled = RcloneManager.isMacFuseInstalled()
        if !macFuseInstalled {
            RuntimeLog.info("macFUSE not detected on launch; showing instructions")
            showMacFuseInstructions()
        } else {
            RuntimeLog.info("macFUSE detected on launch")
            attemptAutoMount()
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        RuntimeLog.info("applicationShouldTerminate requested restart=\(isRestarting)")
        rcloneManager.unmountAll()
        rcloneManager.cleanupTemporaryFiles()
        singleInstanceCoordinator.stop()
        RuntimeLog.info("applicationShouldTerminate completed cleanup")
        return .terminateNow
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        RuntimeLog.info("applicationShouldHandleReopen hasVisibleWindows=\(flag)")
        showSettings()
        return false
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

    @discardableResult
    func restartApp() -> Bool {
        RuntimeLog.info("Restart requested")
        do {
            try startRestartHelper()
            isRestarting = true
            rcloneManager.unmountAll()
            rcloneManager.cleanupExistingAppProcesses()
            RuntimeLog.clear()
            NSApp.terminate(nil)
            return true
        } catch {
            RuntimeLog.error("Could not restart app: \(error.localizedDescription)")
            showRestartError(error)
            return false
        }
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
            controller.onErrorNotificationRequested = { [weak self] message in
                self?.errorNotifications.showError(message)
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

    private func attemptAutoMount() {
        var buckets = AppPreferences.b2Buckets.filter {
            !$0.bucketName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            !$0.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }

        let savedGoogleDrive = AppPreferences.googleDriveSettings
        let googleDrive = rcloneManager.isGoogleDriveConfigured(savedGoogleDrive)
            ? normalizedGoogleDriveForMount(savedGoogleDrive)
            : GoogleDriveSettings()

        guard !buckets.isEmpty || !googleDrive.mountPath.isEmpty else {
            RuntimeLog.info("No saved mounts configured for auto-mount")
            return
        }

        var credentials = B2Credentials(applicationKeyId: "", applicationKey: "")
        if buckets.isEmpty {
            credentials = B2Credentials(applicationKeyId: "", applicationKey: "")
        } else {
            do {
                guard let savedCredentials = try B2CredentialStore.load() else {
                    RuntimeLog.error("Skipping B2 auto-mount because saved credentials were not found")
                    buckets = []
                    if googleDrive.mountPath.isEmpty { return }
                    credentials = B2Credentials(applicationKeyId: "", applicationKey: "")
                    tryAutoMount(buckets: buckets, googleDrive: googleDrive, credentials: credentials)
                    return
                }
                credentials = savedCredentials
            } catch {
                RuntimeLog.error("Skipping B2 auto-mount because credentials could not be loaded: \(error.localizedDescription)")
                buckets = []
                if googleDrive.mountPath.isEmpty { return }
            }
        }

        tryAutoMount(buckets: buckets, googleDrive: googleDrive, credentials: credentials)
    }

    private func tryAutoMount(buckets: [BucketMount], googleDrive: GoogleDriveSettings, credentials: B2Credentials) {
        do {
            RuntimeLog.info("Auto-mounting saved mounts. bucketCount=\(buckets.count) googleDrive=\(!googleDrive.mountPath.isEmpty)")
            try rcloneManager.mount(applicationKeyId: credentials.applicationKeyId, applicationKey: credentials.applicationKey, buckets: buckets, googleDrive: googleDrive)
        } catch {
            RuntimeLog.error("Auto-mount failed: \(error.localizedDescription)")
            errorNotifications.showError("Auto-mount failed. \(error.localizedDescription)")
            settingsWindowController?.appendError("Auto-mount failed. \(error.localizedDescription)")
        }
    }

    private func normalizedGoogleDriveForMount(_ googleDrive: GoogleDriveSettings) -> GoogleDriveSettings {
        var normalized = googleDrive
        normalized.remoteName = CloudProvider.defaultGoogleDriveRemoteName
        if normalized.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            normalized.mountPath = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Drives")
                .appendingPathComponent("Google Drive")
                .path
        }
        return normalized
    }

    private func startRestartHelper() throws {
        let helper = Process()
        helper.executableURL = URL(fileURLWithPath: "/bin/sh")
        var environment = ProcessInfo.processInfo.environment
        environment["CDM_PARENT_PID"] = "\(ProcessInfo.processInfo.processIdentifier)"
        environment["CDM_BUNDLE_PATH"] = Bundle.main.bundleURL.path
        environment["CDM_EXECUTABLE_PATH"] = Bundle.main.executableURL?.path ?? ""
        helper.environment = environment
        helper.arguments = [
            "-c",
            """
            while /bin/kill -0 "$CDM_PARENT_PID" 2>/dev/null; do /bin/sleep 0.1; done
            if [ -d "$CDM_BUNDLE_PATH" ] && [ "${CDM_BUNDLE_PATH##*.}" = "app" ]; then
              /usr/bin/open "$CDM_BUNDLE_PATH" --args --show-settings --clean-restart
            elif [ -n "$CDM_EXECUTABLE_PATH" ]; then
              "$CDM_EXECUTABLE_PATH" --show-settings --clean-restart &
            fi
            """
        ]
        try helper.run()
    }

    private func showRestartError(_ error: Error) {
        let alert = NSAlert()
        alert.messageText = "Could not restart Cloud Drive Mount"
        alert.informativeText = error.localizedDescription
        alert.alertStyle = .warning
        alert.addButton(withTitle: "OK")
        alert.runModal()
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
