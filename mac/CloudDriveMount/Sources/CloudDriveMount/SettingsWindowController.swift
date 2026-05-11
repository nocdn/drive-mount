import AppKit

@MainActor
final class SettingsWindowController: NSWindowController, NSWindowDelegate {
    var onMacFuseHelpRequested: (() -> Void)?

    private let rcloneManager: RcloneManager
    private let keyIdField = NSTextField()
    private let keyField = NSSecureTextField()
    private let bucketStack = NSStackView()
    private let mountButton = NSButton(title: "Save and Mount All", target: nil, action: nil)
    private let unmountButton = NSButton(title: "Unmount All", target: nil, action: nil)
    private let startAtLoginCheckbox = NSButton(checkboxWithTitle: "Start at login", target: nil, action: nil)
    private let startMinimizedCheckbox = NSButton(checkboxWithTitle: "Start minimized to menu bar", target: nil, action: nil)
    private let logView = NSTextView()

    init(rcloneManager: RcloneManager) {
        self.rcloneManager = rcloneManager
        RuntimeLog.info("SettingsWindowController init starting")

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 540, height: 560),
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
        wireRcloneManager()
        addBucketRow()
        updateMountedState(false)
        RuntimeLog.info("SettingsWindowController init completed frame=\(window.frame)")
    }

    required init?(coder: NSCoder) {
        nil
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        RuntimeLog.info("Settings window close requested; hiding window")
        sender.orderOut(nil)
        return false
    }

    private func buildInterface() {
        RuntimeLog.info("Building settings interface")
        guard let contentView = window?.contentView else { return }

        let root = NSStackView()
        root.orientation = .vertical
        root.spacing = 10
        root.alignment = .leading
        root.translatesAutoresizingMaskIntoConstraints = false
        contentView.addSubview(root)

        NSLayoutConstraint.activate([
            root.leadingAnchor.constraint(equalTo: contentView.leadingAnchor, constant: 16),
            root.trailingAnchor.constraint(equalTo: contentView.trailingAnchor, constant: -16),
            root.topAnchor.constraint(equalTo: contentView.topAnchor, constant: 16),
            root.bottomAnchor.constraint(equalTo: contentView.bottomAnchor, constant: -16)
        ])

        let keyIdRow = makeFieldRow(label: "B2 Application Key ID", field: keyIdField)
        root.addArrangedSubview(keyIdRow)
        keyIdRow.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true

        let keyRow = makeFieldRow(label: "B2 Application Key", field: keyField)
        root.addArrangedSubview(keyRow)
        keyRow.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true

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
        root.setCustomSpacing(16, after: macFuseRow)

        let bucketsLabel = makeLabel("Buckets")
        root.addArrangedSubview(bucketsLabel)
        root.setCustomSpacing(3, after: bucketsLabel)

        let bucketsDescription = makeDescription("Each bucket mounts to a local folder.")
        root.addArrangedSubview(bucketsDescription)

        bucketStack.orientation = .vertical
        bucketStack.spacing = 4
        bucketStack.alignment = .leading
        root.addArrangedSubview(bucketStack)
        bucketStack.widthAnchor.constraint(equalTo: root.widthAnchor).isActive = true

        let addBucketRow = NSStackView()
        addBucketRow.orientation = .horizontal
        addBucketRow.alignment = .centerY
        let addBucketButton = NSButton(title: "+ Add Bucket", target: self, action: #selector(addBucketButtonClicked))
        addBucketRow.addArrangedSubview(addBucketButton)
        root.addArrangedSubview(addBucketRow)
        root.setCustomSpacing(14, after: addBucketRow)

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
        let clearLogsButton = NSButton(title: "Clear Logs", target: self, action: #selector(clearLogs))
        buttonRow.addArrangedSubview(mountButton)
        buttonRow.addArrangedSubview(unmountButton)
        buttonRow.addArrangedSubview(clearLogsButton)
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

    private func wireRcloneManager() {
        rcloneManager.onLog = { [weak self] line in
            self?.appendLog(line)
        }
        rcloneManager.onMountedStateChanged = { [weak self] mounted in
            self?.updateMountedState(mounted)
        }
    }

    private func makeLabel(_ text: String) -> NSTextField {
        let label = NSTextField(labelWithString: text)
        return label
    }

    private func makeDescription(_ text: String) -> NSTextField {
        let label = NSTextField(wrappingLabelWithString: text)
        return label
    }

    private func configurePreferenceCheckbox(_ checkbox: NSButton) {
        checkbox.controlSize = .small
        checkbox.cell?.controlSize = .small
        checkbox.title = " \(checkbox.title)"
        checkbox.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
    }

    private func makeFieldRow(label: String, field: NSTextField) -> NSView {
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

    @objc private func showMacFuseHelp() {
        onMacFuseHelpRequested?()
    }

    @objc private func addBucketButtonClicked() {
        addBucketRow()
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
        }
        bucketStack.addArrangedSubview(row)
        growWindowForBucketCount()
    }

    private func defaultMountPath(for bucketName: String) -> String {
        let folder = bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
        return folder.isEmpty ? "~/Drives/" : "~/Drives/\(folder)"
    }

    @objc private func mountAll() {
        do {
            let buckets = collectBuckets()
            RuntimeLog.info("Mount requested from UI. bucketCount=\(buckets.count)")
            try rcloneManager.mount(applicationKeyId: keyIdField.stringValue, applicationKey: keyField.stringValue, buckets: buckets)
            appendInfo("Saved settings")
            appendInfo("Mount requested for \(buckets.count) bucket(s).")
        } catch {
            RuntimeLog.error("Mount failed from UI: \(error.localizedDescription)")
            appendError(error.localizedDescription)
            if case MountError.missingMacFuse = error {
                onMacFuseHelpRequested?()
            }
        }
    }

    @objc private func unmountAll() {
        RuntimeLog.info("Unmount requested from UI")
        rcloneManager.unmountAll()
        appendInfo("Unmounted all buckets.")
    }

    @objc private func clearLogs() {
        logView.string = ""
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

    private func appendError(_ message: String) {
        appendLog(formatLog(level: "ERROR", message: message))
    }

    private func formatLog(level: String, message: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return "[\(formatter.string(from: Date()))] [\(level)] \(message)"
    }

    private func growWindowForBucketCount() {
        guard let window else { return }
        let desiredHeight = CGFloat(560 + max(0, bucketStack.arrangedSubviews.count - 1) * 30)
        guard window.frame.height < desiredHeight else { return }

        var frame = window.frame
        let delta = desiredHeight - frame.height
        frame.origin.y -= delta
        frame.size.height = desiredHeight
        window.setFrame(frame, display: true, animate: true)
    }
}

@MainActor
private final class BucketRowView: NSView {
    var onRemove: (() -> Void)?

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
        mountPathField.stringValue = "~/Drives/"
        updateFieldWidths()
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
                mountPathField.stringValue = bucketName.isEmpty ? "~/Drives/" : "~/Drives/\(bucketName)"
                isProgrammaticMountPathChange = false
            } else if field === mountPathField && !isProgrammaticMountPathChange {
                mountPathManuallyEdited = true
            }
        }

        updateFieldWidths()
    }
}
