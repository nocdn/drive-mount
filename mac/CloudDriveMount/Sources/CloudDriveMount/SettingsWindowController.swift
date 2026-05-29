import AppKit

@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    var onMacFuseHelpRequested: (() -> Void)?

    private let rcloneManager: RcloneManager
    private let rootStack = NSStackView()
    private let providerPopup = NSPopUpButton()
    private let b2OptionsStack = NSStackView()
    private let googleDriveOptionsStack = NSStackView()
    private let keyIdField = NSTextField()
    private let keyField = NSSecureTextField()
    private let bucketStack = NSStackView()
    private let googleDriveRemotePathField = NSTextField()
    private let googleDriveRootFolderIdField = NSTextField()
    private let googleDriveConnectHelp = NSTextField(wrappingLabelWithString: "Click Connect Google Drive and sign in through the browser. After it connects, use Save and Mount All. Google Drive will mount as a disk named Google Drive under ~/Drives/Google Drive.")
    private let connectGoogleDriveButton = NSButton(title: "Connect Google Drive", target: nil, action: nil)
    private let testGoogleDriveConnectionButton = NSButton(title: "Test Connection", target: nil, action: nil)
    private let mountButton = NSButton(title: "Save and Mount All", target: nil, action: nil)
    private let unmountButton = NSButton(title: "Unmount All", target: nil, action: nil)
    private let openLogsButton = NSButton(title: "Open Log Folder", target: nil, action: nil)
    private let clearLogsButton = NSButton(title: "Clear Logs", target: nil, action: nil)
    private let restartButton = NSButton(title: "Restart", target: nil, action: nil)
    private let startAtLoginCheckbox = NSButton(checkboxWithTitle: "Start at login", target: nil, action: nil)
    private let startMinimizedCheckbox = NSButton(checkboxWithTitle: "Start minimized to menu bar", target: nil, action: nil)
    private let logView = NSTextView()

    init(rcloneManager: RcloneManager) {
        self.rcloneManager = rcloneManager
        RuntimeLog.info("SettingsWindowController init starting")

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 540, height: 660),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Cloud Drive Mount Settings"
        window.center()
        window.isReleasedWhenClosed = false

        super.init(window: window)
        window.delegate = self
        buildInterface()
        loadSavedSettings()
        wireRcloneManager()
        updateProviderPanels()
        refreshGoogleDriveConnectionUi()
        updateMountedState(rcloneManager.isMounted)
        RuntimeLog.info("SettingsWindowController init completed frame=\(window.frame)")
    }

    required init?(coder: NSCoder) {
        nil
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        RuntimeLog.info("Settings window close requested; hiding window")
        saveVisibleSettings()
        sender.orderOut(nil)
        return false
    }

    private func buildInterface() {
        RuntimeLog.info("Building settings interface")
        guard let contentView = window?.contentView else { return }

        let root = rootStack
        root.orientation = .vertical
        root.spacing = 10
        root.alignment = .leading
        root.detachesHiddenViews = true
        root.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(root)

        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: 16),
            root.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -16),
            root.topAnchor.constraint(equalTo: contentView.topAnchor, constant: 16),
            root.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -16)
        ])

        providerPopup.addItems(withTitles: ["B2", "Google Drive"])
        providerPopup.target = self
        providerPopup.action = #selector(providerChanged)
        let providerRow = makeFieldRow(label: "Provider", field: providerPopup)
        root.addArrangedSubview(providerRow)
        providerRow.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true

        b2OptionsStack.orientation = .vertical
        b2OptionsStack.spacing = 10
        b2OptionsStack.alignment = .leading
        root.addArrangedSubview(b2OptionsStack)
        b2OptionsStack.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true
        buildB2Options()

        googleDriveOptionsStack.orientation = .vertical
        googleDriveOptionsStack.spacing = 10
        googleDriveOptionsStack.alignment = .leading
        root.addArrangedSubview(googleDriveOptionsStack)
        googleDriveOptionsStack.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true
        buildGoogleDriveOptions()

        let macFuseRow = NSStackView()
        macFuseRow.orientation = .horizontal
        macFuseRow.spacing = 8
        macFuseRow.alignment = .centerY
        let macFuseStatus = NSTextField(labelWithString: RcloneManager.isMacFuseInstalled() ? "macFUSE detected." : "macFUSE not detected. Mounting requires macFUSE.")
        macFuseStatus.textColor = RcloneManager.isMacFuseInstalled() ? .secondaryLabelColor : .systemOrange
        let macFuseHelp = NSButton(title: "macFUSE Help", target: self, action: #selector(showMacFuseHelp))
        macFuseRow.addArrangedSubview(macFuseHelp)
        macFuseRow.addArrangedSubview(macFuseStatus)
        root.addArrangedSubview(macFuseRow)

        startAtLoginCheckbox.state = AppPreferences.startAtLogin ? .on : .off
        startAtLoginCheckbox.target = self
        startAtLoginCheckbox.action = #selector(startAtLoginChanged)
        configurePreferenceCheckbox(startAtLoginCheckbox)
        root.addArrangedSubview(startAtLoginCheckbox)

        startMinimizedCheckbox.state = AppPreferences.startMinimized ? .on : .off
        startMinimizedCheckbox.target = self
        startMinimizedCheckbox.action = #selector(startMinimizedChanged)
        configurePreferenceCheckbox(startMinimizedCheckbox)
        root.addArrangedSubview(startMinimizedCheckbox)
        root.setCustomSpacing(14, after: startMinimizedCheckbox)

        let buttonRow = NSStackView()
        buttonRow.orientation = .horizontal
        buttonRow.spacing = 8
        buttonRow.alignment = .centerY
        mountButton.target = self
        mountButton.action = #selector(mountAll)
        unmountButton.target = self
        unmountButton.action = #selector(unmountAll)
        openLogsButton.target = self
        openLogsButton.action = #selector(openLogFolder)
        clearLogsButton.target = self
        clearLogsButton.action = #selector(clearLogs)
        restartButton.target = self
        restartButton.action = #selector(restartApp)
        buttonRow.addArrangedSubview(mountButton)
        buttonRow.addArrangedSubview(unmountButton)
        buttonRow.addArrangedSubview(openLogsButton)
        buttonRow.addArrangedSubview(clearLogsButton)
        buttonRow.addArrangedSubview(restartButton)
        root.addArrangedSubview(buttonRow)

        root.addArrangedSubview(makeLabel("Logs"))

        logView.isEditable = false
        logView.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        logView.textContainerInset = NSSize(width: 6, height: 6)

        let logScroll = NSScrollView()
        logScroll.hasVerticalScroller = true
        logScroll.borderType = .bezelBorder
        logScroll.documentView = logView
        logScroll.translatesAutoresizingMaskIntoConstraints = false
        logScroll.heightAnchor.constraint(equalToConstant: 190).isActive = true
        root.addArrangedSubview(logScroll)
        logScroll.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true

        appendInfo("Cloud Drive Mount started. Use the menu bar icon to reopen Settings after closing this window.")
        RuntimeLog.info("Settings interface built")
    }

    private func buildB2Options() {
        let keyIdRow = makeFieldRow(label: "B2 Application Key ID", field: keyIdField)
        b2OptionsStack.addArrangedSubview(keyIdRow)
        keyIdRow.widthAnchor.constraint(equalTo: b2OptionsStack.widthAnchor).isActive = true

        let keyRow = makeFieldRow(label: "B2 Application Key", field: keyField)
        b2OptionsStack.addArrangedSubview(keyRow)
        keyRow.widthAnchor.constraint(equalTo: b2OptionsStack.widthAnchor).isActive = true

        let bucketsLabel = makeLabel("Buckets")
        b2OptionsStack.addArrangedSubview(bucketsLabel)
        b2OptionsStack.setCustomSpacing(3, after: bucketsLabel)

        let bucketsDescription = makeDescription("Each bucket mounts to a local folder.")
        b2OptionsStack.addArrangedSubview(bucketsDescription)

        bucketStack.orientation = .vertical
        bucketStack.spacing = 4
        bucketStack.alignment = .leading
        b2OptionsStack.addArrangedSubview(bucketStack)
        bucketStack.widthAnchor.constraint(equalTo: b2OptionsStack.widthAnchor).isActive = true

        let addBucketRow = NSStackView()
        addBucketRow.orientation = .horizontal
        addBucketRow.alignment = .centerY
        let addBucketButton = NSButton(title: "+ Add Bucket", target: self, action: #selector(addBucketButtonClicked))
        addBucketRow.addArrangedSubview(addBucketButton)
        b2OptionsStack.addArrangedSubview(addBucketRow)
        b2OptionsStack.setCustomSpacing(14, after: addBucketRow)
    }

    private func buildGoogleDriveOptions() {
        let remotePathRow = makeFieldRow(label: "Google Drive Folder Path (optional)", field: googleDriveRemotePathField)
        googleDriveOptionsStack.addArrangedSubview(remotePathRow)
        remotePathRow.widthAnchor.constraint(equalTo: googleDriveOptionsStack.widthAnchor).isActive = true

        let rootFolderIdRow = makeFieldRow(label: "Google Drive Root Folder ID (optional)", field: googleDriveRootFolderIdField)
        googleDriveOptionsStack.addArrangedSubview(rootFolderIdRow)
        rootFolderIdRow.widthAnchor.constraint(equalTo: googleDriveOptionsStack.widthAnchor).isActive = true

        googleDriveOptionsStack.addArrangedSubview(googleDriveConnectHelp)

        let googleButtonRow = NSStackView()
        googleButtonRow.orientation = .horizontal
        googleButtonRow.spacing = 8
        googleButtonRow.alignment = .centerY
        connectGoogleDriveButton.target = self
        connectGoogleDriveButton.action = #selector(connectGoogleDriveClicked)
        testGoogleDriveConnectionButton.target = self
        testGoogleDriveConnectionButton.action = #selector(testGoogleDriveConnectionClicked)
        googleButtonRow.addArrangedSubview(connectGoogleDriveButton)
        googleButtonRow.addArrangedSubview(testGoogleDriveConnectionButton)
        googleDriveOptionsStack.addArrangedSubview(googleButtonRow)
        googleDriveOptionsStack.setCustomSpacing(14, after: googleButtonRow)
    }

    private func wireRcloneManager() {
        rcloneManager.onLog = { [weak self] line in
            self?.appendLog(line)
        }
        rcloneManager.onMountedStateChanged = { [weak self] mounted in
            self?.updateMountedState(mounted)
        }
    }

    private func loadSavedSettings() {
        do {
            if let credentials = try B2CredentialStore.load() {
                keyIdField.stringValue = credentials.applicationKeyId
                keyField.stringValue = credentials.applicationKey
                RuntimeLog.info("Loaded saved B2 credentials from Keychain")
            }
        } catch {
            RuntimeLog.error("Failed to load saved B2 credentials: \(error.localizedDescription)")
            appendError(error.localizedDescription)
        }

        providerPopup.selectItem(at: AppPreferences.selectedProvider == .googleDrive ? 1 : 0)

        let savedBuckets = AppPreferences.b2Buckets
        if savedBuckets.isEmpty {
            addBucketRow()
        } else {
            for bucket in savedBuckets {
                addBucketRow(bucketName: bucket.bucketName, mountPath: bucket.mountPath)
            }
        }

        let googleDrive = AppPreferences.googleDriveSettings
        googleDriveRemotePathField.stringValue = googleDrive.remotePath
        googleDriveRootFolderIdField.stringValue = googleDrive.rootFolderId
    }

    private func saveVisibleSettings() {
        AppPreferences.selectedProvider = selectedProvider()
        AppPreferences.b2Buckets = collectBuckets(allowEmpty: false)
        AppPreferences.googleDriveSettings = readGoogleDriveSettings()
    }

    private func makeLabel(_ text: String) -> NSTextField {
        NSTextField(labelWithString: text)
    }

    private func makeDescription(_ text: String) -> NSTextField {
        NSTextField(wrappingLabelWithString: text)
    }

    private func configurePreferenceCheckbox(_ checkbox: NSButton) {
        checkbox.controlSize = .small
        checkbox.cell?.controlSize = .small
        checkbox.title = " \(checkbox.title)"
        checkbox.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
    }

    private func makeFieldRow(label: String, field: NSControl) -> NSView {
        let row = NSStackView()
        row.orientation = .vertical
        row.spacing = 3
        row.alignment = .leading

        let labelView = NSTextField(labelWithString: label)
        field.heightAnchor.constraint(equalToConstant: 24).isActive = true

        row.addArrangedSubview(labelView)
        row.addArrangedSubview(field)
        field.widthAnchor.constraint(equalTo: row.widthAnchor).isActive = true
        return row
    }

    private func selectedProvider() -> CloudProvider {
        providerPopup.indexOfSelectedItem == 1 ? .googleDrive : .backblazeB2
    }

    private func updateProviderPanels() {
        let provider = selectedProvider()
        b2OptionsStack.isHidden = provider != .backblazeB2
        googleDriveOptionsStack.isHidden = provider != .googleDrive
        rootStack.layoutSubtreeIfNeeded()
        resizeWindowToFitCurrentProvider(animated: true)
    }

    private func refreshGoogleDriveConnectionUi() {
        let connected = rcloneManager.isGoogleDriveConfigured(readGoogleDriveSettings())
        googleDriveConnectHelp.isHidden = connected
        connectGoogleDriveButton.title = connected ? "Disconnect Google Drive" : "Connect Google Drive"
        testGoogleDriveConnectionButton.isHidden = !connected
    }

    @objc private func providerChanged() {
        saveVisibleSettings()
        updateProviderPanels()
        refreshGoogleDriveConnectionUi()
    }

    @objc private func showMacFuseHelp() {
        onMacFuseHelpRequested?()
    }

    @objc private func addBucketButtonClicked() {
        addBucketRow()
        saveVisibleSettings()
    }

    @objc private func startAtLoginChanged() {
        let enabled = startAtLoginCheckbox.state == .on
        RuntimeLog.info("Start at login checkbox changed: \(enabled)")
        (NSApp.delegate as? AppDelegate)?.setStartAtLogin(enabled)
    }

    @objc private func startMinimizedChanged() {
        let enabled = startMinimizedCheckbox.state == .on
        RuntimeLog.info("Start minimized checkbox changed: \(enabled)")
        AppPreferences.startMinimized = enabled
    }

    private func addBucketRow(bucketName: String = "", mountPath: String = "") {
        let row = BucketRowView(bucketName: bucketName, mountPath: mountPath.isEmpty ? defaultMountPath(for: bucketName) : mountPath)
        row.onRemove = { [weak self, weak row] in
            guard let self, let row else { return }
            if self.bucketStack.arrangedSubviews.count > 1 {
                self.bucketStack.removeArrangedSubview(row)
                row.removeFromSuperview()
            } else {
                row.clear()
            }
            self.saveVisibleSettings()
        }
        row.onChanged = { [weak self] in
            self?.saveVisibleSettings()
        }
        bucketStack.addArrangedSubview(row)
        growWindowForBucketCount()
    }

    private func defaultMountPath(for bucketName: String) -> String {
        let folder = bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
        return folder.isEmpty ? "" : "~/Drives/\(folder)"
    }

    private func readGoogleDriveSettings() -> GoogleDriveSettings {
        GoogleDriveSettings(
            remoteName: CloudProvider.defaultGoogleDriveRemoteName,
            remotePath: normalizeGoogleDrivePath(googleDriveRemotePathField.stringValue),
            rootFolderId: googleDriveRootFolderIdField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines),
            mountPath: defaultGoogleDriveDiskMountPath()
        )
    }

    private func defaultGoogleDriveDiskMountPath() -> String {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Drives")
            .appendingPathComponent("Google Drive")
            .path
    }

    private func normalizeGoogleDrivePath(_ value: String) -> String {
        var path = value.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\\", with: "/")
        while path.hasPrefix("/") || path.hasPrefix(":") {
            path.removeFirst()
        }
        return path
    }

    @objc private func connectGoogleDriveClicked() {
        saveVisibleSettings()
        let googleDrive = readGoogleDriveSettings()
        let isConnected = rcloneManager.isGoogleDriveConfigured(googleDrive)

        connectGoogleDriveButton.isEnabled = false
        testGoogleDriveConnectionButton.isEnabled = false
        connectGoogleDriveButton.title = isConnected ? "Disconnecting..." : "Connecting..."

        Task { @MainActor in
            defer {
                connectGoogleDriveButton.isEnabled = true
                testGoogleDriveConnectionButton.isEnabled = true
                refreshGoogleDriveConnectionUi()
            }

            do {
                if isConnected {
                    appendInfo("Disconnecting Google Drive.")
                    try rcloneManager.disconnectGoogleDrive(googleDrive)
                    appendInfo("Google Drive is disconnected.")
                } else {
                    appendInfo("Starting Google Drive authorization. Complete the sign-in in your browser.")
                    try rcloneManager.configureGoogleDrive(googleDrive)
                    appendInfo("Google Drive is connected. You can now click Save and Mount All.")
                }
            } catch {
                RuntimeLog.error("Google Drive connect/disconnect failed: \(error.localizedDescription)")
                appendError(error.localizedDescription)
            }
        }
    }

    @objc private func testGoogleDriveConnectionClicked() {
        saveVisibleSettings()
        let googleDrive = readGoogleDriveSettings()

        guard rcloneManager.isGoogleDriveConfigured(googleDrive) else {
            appendError("Google Drive is not connected. Click Connect Google Drive first.")
            refreshGoogleDriveConnectionUi()
            return
        }

        connectGoogleDriveButton.isEnabled = false
        testGoogleDriveConnectionButton.isEnabled = false
        testGoogleDriveConnectionButton.title = "Testing..."
        appendInfo("Testing Google Drive connection.")

        Task { @MainActor in
            defer {
                connectGoogleDriveButton.isEnabled = true
                testGoogleDriveConnectionButton.isEnabled = true
                testGoogleDriveConnectionButton.title = "Test Connection"
                refreshGoogleDriveConnectionUi()
            }

            do {
                try rcloneManager.testGoogleDriveConnection(googleDrive)
                appendInfo("Google Drive connection test succeeded.")
            } catch {
                RuntimeLog.error("Google Drive connection test failed: \(error.localizedDescription)")
                appendError("Google Drive connection test failed. \(error.localizedDescription)")
            }
        }
    }

    @objc private func mountAll() {
        do {
            saveVisibleSettings()
            let buckets = collectBuckets()
            let googleDrive = readGoogleDriveSettings()
            if selectedProvider() == .googleDrive &&
                buckets.isEmpty &&
                !rcloneManager.isGoogleDriveConfigured(googleDrive) {
                throw MountError.googleDriveNotConfigured
            }

            RuntimeLog.info("Mount requested from UI. bucketCount=\(buckets.count) googleDriveDisk=\(!googleDrive.mountPath.isEmpty)")
            let credentials = try loadOrSaveCredentialsIfNeeded(buckets: buckets)
            try rcloneManager.mount(applicationKeyId: credentials.applicationKeyId, applicationKey: credentials.applicationKey, buckets: buckets, googleDrive: googleDrive)
            appendInfo("Saved settings")
            appendInfo("Mount requested.")
            refreshGoogleDriveConnectionUi()
        } catch {
            RuntimeLog.error("Mount failed from UI: \(error.localizedDescription)")
            appendError(error.localizedDescription)
            if case MountError.missingMacFuse = error {
                onMacFuseHelpRequested?()
            }
        }
    }

    private func loadOrSaveCredentialsIfNeeded(buckets: [BucketMount]) throws -> B2Credentials {
        let hasB2Rows = buckets.contains {
            !$0.bucketName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !$0.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }

        if hasB2Rows {
            return try saveCredentials()
        }

        return B2Credentials(applicationKeyId: keyIdField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines), applicationKey: keyField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    private func saveCredentials() throws -> B2Credentials {
        let credentials = B2Credentials(
            applicationKeyId: keyIdField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines),
            applicationKey: keyField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        )

        guard !credentials.applicationKeyId.isEmpty, !credentials.applicationKey.isEmpty else {
            throw MountError.missingCredentials
        }

        try B2CredentialStore.save(credentials)
        RuntimeLog.info("Saved B2 credentials to Keychain")
        return credentials
    }

    @objc private func unmountAll() {
        RuntimeLog.info("Unmount requested from UI")
        rcloneManager.unmountAll()
        appendInfo("Unmounted all.")
    }

    @objc private func openLogFolder() {
        do {
            try FileManager.default.createDirectory(at: RuntimeLog.logDirectory, withIntermediateDirectories: true)
            NSWorkspace.shared.open(RuntimeLog.logDirectory)
        } catch {
            RuntimeLog.error("Could not open log folder: \(error.localizedDescription)")
            appendError("Could not open log folder. \(error.localizedDescription)")
        }
    }

    @objc private func clearLogs() {
        RuntimeLog.clear()
        logView.string = ""
    }

    @objc private func restartApp() {
        restartButton.isEnabled = false
        appendInfo("Restarting Cloud Drive Mount.")
        saveVisibleSettings()
        guard let delegate = NSApp.delegate as? AppDelegate else {
            appendError("Could not restart Cloud Drive Mount.")
            restartButton.isEnabled = true
            return
        }

        if !delegate.restartApp() {
            restartButton.isEnabled = true
        }
    }

    private func collectBuckets(allowEmpty: Bool = false) -> [BucketMount] {
        bucketStack.arrangedSubviews.compactMap { view in
            guard let row = view as? BucketRowView else { return nil }
            let bucketName = row.bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
            let mountPath = row.mountPath.trimmingCharacters(in: .whitespacesAndNewlines)
            if allowEmpty || (!bucketName.isEmpty || !mountPath.isEmpty) {
                return BucketMount(bucketName: bucketName, mountPath: mountPath)
            }
            return nil
        }
    }

    private func updateMountedState(_ mounted: Bool) {
        mountButton.isEnabled = !mounted
        unmountButton.isEnabled = mounted
    }

    private func appendLog(_ line: String) {
        let current = logView.string
        logView.string = current.isEmpty ? line : current + "\n" + line
        logView.scrollToEndOfDocument(nil)
    }

    private func appendInfo(_ message: String) {
        appendLog(formatLog(level: "INFO", message: message))
    }

    func appendError(_ message: String) {
        appendLog(formatLog(level: "ERROR", message: message))
    }

    private func formatLog(level: String, message: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return "[\(formatter.string(from: Date()))] [\(level)] \(message)"
    }

    private func growWindowForBucketCount() {
        resizeWindowToFitCurrentProvider(animated: true)
    }

    private func resizeWindowToFitCurrentProvider(animated: Bool) {
        guard let window else { return }

        let desiredHeight: CGFloat
        if selectedProvider() == .googleDrive {
            desiredHeight = 560
        } else {
            desiredHeight = CGFloat(660 + max(0, bucketStack.arrangedSubviews.count - 1) * 30)
        }

        let minimumHeight: CGFloat = 520
        let targetHeight = max(minimumHeight, desiredHeight)
        let currentFrame = window.frame
        guard abs(currentFrame.height - targetHeight) > 1 else { return }

        var frame = currentFrame
        let delta = targetHeight - frame.height
        frame.origin.y -= delta
        frame.size.height = targetHeight
        window.setFrame(frame, display: true, animate: animated)
    }
}

@MainActor
private final class BucketRowView: NSView {
    var onRemove: (() -> Void)?
    var onChanged: (() -> Void)?

    private let bucketField = NSTextField()
    private let mountPathField = NSTextField()
    private var bucketWidthConstraint: NSLayoutConstraint?
    private var mountWidthConstraint: NSLayoutConstraint?
    private var mountPathManuallyEdited = false
    private var isProgrammaticMountPathChange = false

    var bucketName: String { bucketField.stringValue }
    var mountPath: String { mountPathField.stringValue }

    init(bucketName: String, mountPath: String) {
        super.init(frame: .zero)
        bucketField.stringValue = bucketName
        mountPathField.stringValue = mountPath
        buildInterface()
        updateFieldWidths()
    }

    required init?(coder: NSCoder) {
        nil
    }

    func clear() {
        bucketField.stringValue = ""
        mountPathManuallyEdited = false
        mountPathField.stringValue = ""
        updateFieldWidths()
        onChanged?()
    }

    private func buildInterface() {
        let row = NSStackView()
        row.orientation = .horizontal
        row.spacing = 6
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)

        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: leadingAnchor),
            row.trailingAnchor.constraint(equalTo: trailingAnchor),
            row.topAnchor.constraint(equalTo: topAnchor),
            row.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        let bucketLabel = NSTextField(labelWithString: "Bucket")
        bucketLabel.widthAnchor.constraint(equalToConstant: 44).isActive = true
        bucketField.delegate = self
        bucketWidthConstraint = bucketField.widthAnchor.constraint(equalToConstant: 85)
        bucketWidthConstraint?.isActive = true

        let mountMargin = NSView()
        mountMargin.widthAnchor.constraint(equalToConstant: 12).isActive = true

        let mountLabel = NSTextField(labelWithString: "Mount")
        mountLabel.widthAnchor.constraint(equalToConstant: 42).isActive = true
        mountPathField.placeholderString = "~/Drives/"
        mountPathField.delegate = self
        mountWidthConstraint = mountPathField.widthAnchor.constraint(equalToConstant: 120)
        mountWidthConstraint?.isActive = true

        let browseButton = NSButton(title: "Browse", target: self, action: #selector(browse))
        browseButton.widthAnchor.constraint(equalToConstant: 58).isActive = true
        let removeButton = NSButton(title: "Remove", target: self, action: #selector(remove))
        removeButton.widthAnchor.constraint(equalToConstant: 64).isActive = true

        row.addArrangedSubview(bucketLabel)
        row.addArrangedSubview(bucketField)
        row.addArrangedSubview(mountMargin)
        row.addArrangedSubview(mountLabel)
        row.addArrangedSubview(mountPathField)
        row.addArrangedSubview(browseButton)
        row.addArrangedSubview(removeButton)
    }

    private func updateFieldWidths() {
        bucketWidthConstraint?.constant = measuredWidth(for: bucketField, placeholder: "bucket-name", min: 85, max: 170)
        mountWidthConstraint?.constant = measuredWidth(for: mountPathField, placeholder: "~/Drives/", min: 120, max: 230)
        layoutSubtreeIfNeeded()
    }

    private func measuredWidth(for field: NSTextField, placeholder: String, min: CGFloat, max: CGFloat) -> CGFloat {
        let text = field.stringValue.isEmpty ? placeholder : field.stringValue
        let font = field.font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let width = (text as NSString).size(withAttributes: [.font: font]).width + 28
        return Swift.max(min, Swift.min(max, ceil(width)))
    }

    @objc private func browse() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"

        if panel.runModal() == .OK, let url = panel.url {
            mountPathManuallyEdited = true
            mountPathField.stringValue = url.path
            updateFieldWidths()
            onChanged?()
        }
    }

    @objc private func remove() {
        onRemove?()
    }
}

extension BucketRowView: NSTextFieldDelegate {
    func controlTextDidChange(_ obj: Notification) {
        if let field = obj.object as? NSTextField {
            if field === bucketField && !mountPathManuallyEdited {
                isProgrammaticMountPathChange = true
                let bucketName = bucketField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
                mountPathField.stringValue = bucketName.isEmpty ? "" : "~/Drives/\(bucketName)"
                isProgrammaticMountPathChange = false
            } else if field === mountPathField && !isProgrammaticMountPathChange {
                mountPathManuallyEdited = true
            }
        }

        updateFieldWidths()
        onChanged?()
    }
}
