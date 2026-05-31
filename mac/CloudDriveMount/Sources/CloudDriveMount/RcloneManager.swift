import Darwin
import Foundation

@MainActor
final class RcloneManager {
    private struct MountSpec {
        var label: String
        var remotePath: String
        var mountPath: String
        var volumeName: String
        var vfsCacheMode: String
        var readOnly: Bool
        var extraArgs: [String] = []
    }

    private struct RunningProcess {
        var pid: pid_t
        var ppid: pid_t
        var state: String
        var user: String
        var command: String
    }

    private enum RcloneOutputSource {
        case stdout
        case stderr
    }

    private enum RcloneLineSeverity {
        case info
        case error
        case hiddenErrorDetail
    }

    var onLog: ((String) -> Void)?
    var onError: ((String) -> Void)?
    var onMountedStateChanged: ((Bool) -> Void)?

    private var processes: [String: Process] = [:]
    private var intentionalStops = Set<String>()
    private var unclearedRclonePIDs = Set<pid_t>()
    private let appSupportURL: URL
    private let configURL: URL
    private let cacheURL: URL
    private let mountStartupTimeout: TimeInterval = 90

    var isMounted: Bool {
        processes.values.contains { $0.isRunning }
    }

    init() {
        appSupportURL = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library")
            .appendingPathComponent("Application Support")
            .appendingPathComponent("CloudDriveMount")
        configURL = appSupportURL.appendingPathComponent("rclone.conf")
        cacheURL = appSupportURL.appendingPathComponent("cache")
        RuntimeLog.info("RcloneManager initialized appSupport=\(appSupportURL.path)")
    }

    static func isMacFuseInstalled() -> Bool {
        let paths = [
            "/Library/Filesystems/macfuse.fs",
            "/usr/local/lib/libfuse.2.dylib",
            "/opt/homebrew/lib/libfuse.2.dylib",
            "/Library/Filesystems/fuse-t.fs",
            "/usr/local/bin/fuse-t",
            "/opt/homebrew/bin/fuse-t"
        ]

        return paths.contains { FileManager.default.fileExists(atPath: $0) }
    }

    func isGoogleDriveConfigured(_ googleDrive: GoogleDriveSettings) -> Bool {
        hasConfigSection(getGoogleDriveRemoteName(googleDrive))
    }

    func isSeedboxConfigured(_ seedbox: SeedboxSettings) -> Bool {
        hasConfigSection(getSeedboxRemoteName(seedbox))
    }

    func cleanupExistingAppProcesses() {
        unclearedRclonePIDs = unclearedRclonePIDs.filter { isProcessRunning($0) }

        let staleProcesses = appOwnedRcloneProcesses()
            .filter { !unclearedRclonePIDs.contains($0.pid) }
        guard !staleProcesses.isEmpty else {
            if unclearedRclonePIDs.isEmpty {
                RuntimeLog.info("No stale app-owned rclone processes found")
            } else {
                RuntimeLog.error("Stale rclone processes remain uncleared: \(unclearedRclonePIDs.sorted().map { String($0) }.joined(separator: ", "))")
            }
            return
        }

        for process in staleProcesses {
            logInfo("Stopping stale rclone process PID=\(process.pid) state=\(process.state).")
            let timeout: TimeInterval = isOrphanedExitingRclone(process) ? 0.5 : 5
            if !terminate(pid: process.pid, timeout: timeout) {
                unclearedRclonePIDs.insert(process.pid)
            }
        }
    }

    func configureGoogleDrive(_ googleDrive: GoogleDriveSettings) throws {
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }

        let remoteName = getGoogleDriveRemoteName(googleDrive)
        try ensureConfigDirectory()
        let originalConfig = (try? String(contentsOf: configURL, encoding: .utf8)) ?? ""
        try removeConfigSection(remoteName)

        var args = [
            "config",
            "create",
            remoteName,
            "drive",
            "scope",
            "drive",
            "config_is_local",
            "true",
            "--no-output"
        ]

        let rootFolderId = googleDrive.rootFolderId.trimmingCharacters(in: .whitespacesAndNewlines)
        if !rootFolderId.isEmpty {
            args.append("root_folder_id")
            args.append(rootFolderId)
        }

        args.append("--config")
        args.append(configURL.path)

        logInfo("Starting Google Drive authorization.")
        logInfo("A browser window should open. Sign in and allow access to complete the rclone setup.")

        let ok = runRcloneToCompletion(rclonePath: rclonePath, args: args, label: "Google Drive authorization")
        guard ok else {
            try originalConfig.write(to: configURL, atomically: true, encoding: .utf8)
            throw MountError.googleDriveNotConfigured
        }

        guard hasConfigSection(remoteName) else {
            try originalConfig.write(to: configURL, atomically: true, encoding: .utf8)
            throw MountError.googleDriveNotConfigured
        }

        logInfo("Google Drive is configured in \(configURL.path).")
    }

    func disconnectGoogleDrive(_ googleDrive: GoogleDriveSettings) throws {
        let remoteName = getGoogleDriveRemoteName(googleDrive)
        let mountPath = expandPath(googleDrive.mountPath.isEmpty ? defaultGoogleDriveMountPath() : googleDrive.mountPath)
        if !mountPath.isEmpty {
            unmountDrive(mountPath: mountPath)
        }

        try removeConfigSection(remoteName)
        logInfo("Google Drive remote has been removed from the app rclone config.")
    }

    func testGoogleDriveConnection(_ googleDrive: GoogleDriveSettings) throws {
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }
        guard hasConfigSection(getGoogleDriveRemoteName(googleDrive)) else { throw MountError.googleDriveNotConfigured }

        let remotePath = buildGoogleDriveRemotePath(googleDrive)
        logInfo("Testing Google Drive connection using \(remotePath).")
        let ok = runRcloneToCompletion(rclonePath: rclonePath, args: ["lsd", remotePath, "--config", configURL.path], label: "Google Drive connection test")
        if ok {
            logInfo("Google Drive connection test completed successfully.")
        } else {
            throw MountError.googleDriveNotConfigured
        }
    }

    func configureSeedbox(_ seedbox: SeedboxSettings, password: String) throws {
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }
        let normalized = normalizeSeedbox(seedbox)
        guard isValid(seedbox: normalized) else { throw MountError.missingSeedboxCredentials }
        guard !password.isEmpty || hasConfigSection(getSeedboxRemoteName(normalized)) else { throw MountError.missingSeedboxCredentials }

        if password.isEmpty {
            logInfo("Using existing Seedbox rclone config.")
            return
        }

        guard let obscuredPassword = obscurePassword(rclonePath: rclonePath, password: password) else {
            throw MountError.missingSeedboxCredentials
        }

        let lines = [
            "type = ftp",
            "host = \(normalized.host)",
            "user = \(normalized.username)",
            "port = \(normalized.port)",
            "pass = \(obscuredPassword)",
            "explicit_tls = true",
            "tls = false",
            "no_check_certificate = \(normalized.allowUnverifiedCertificate ? "true" : "false")"
        ]

        try upsertConfigSection(getSeedboxRemoteName(normalized), lines: lines)
        logInfo("Seedbox FTPS rclone config written to: \(configURL.path).")
    }

    func disconnectSeedbox(_ seedbox: SeedboxSettings) throws {
        let normalized = normalizeSeedbox(seedbox)
        let mountPath = expandPath(normalized.mountPath.isEmpty ? defaultSeedboxMountPath() : normalized.mountPath)
        if !mountPath.isEmpty {
            unmountDrive(mountPath: mountPath)
        }

        try removeConfigSection(getSeedboxRemoteName(normalized))
        logInfo("Seedbox remote has been removed from the app rclone config.")
    }

    func testSeedboxConnection(_ seedbox: SeedboxSettings, password: String) throws {
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }
        let normalized = normalizeSeedbox(seedbox)
        try configureSeedbox(normalized, password: password)

        let remotePath = buildSeedboxRemotePath(normalized)
        logInfo("Testing Seedbox FTPS connection using \(remotePath).")
        let ok = runRcloneToCompletion(rclonePath: rclonePath, args: ["lsd", remotePath, "--config", configURL.path], label: "Seedbox connection test")
        if ok {
            logInfo("Seedbox connection test completed successfully.")
        } else {
            throw MountError.seedboxNotConfigured
        }
    }

    func mount(applicationKeyId: String, applicationKey: String, buckets: [BucketMount], googleDrive: GoogleDriveSettings, seedbox: SeedboxSettings) throws {
        guard Self.isMacFuseInstalled() else { throw MountError.missingMacFuse }
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }

        try ensureConfigDirectory()
        try FileManager.default.createDirectory(at: cacheURL, withIntermediateDirectories: true)

        let mountSpecs = try buildMountSpecs(applicationKeyId: applicationKeyId, applicationKey: applicationKey, buckets: buckets, googleDrive: googleDrive, seedbox: seedbox)
        guard !mountSpecs.isEmpty else { throw MountError.missingBuckets }

        try validate(mountSpecs: mountSpecs)
        unmountAll()
        cleanupExistingAppProcesses()
        if !unclearedRclonePIDs.isEmpty {
            throw MountError.staleRcloneProcesses(Array(unclearedRclonePIDs).sorted())
        }

        logInfo("Found rclone at: \(rclonePath)")

        do {
            for spec in mountSpecs {
                try mountRemote(rclonePath: rclonePath, spec: spec)
            }
        } catch {
            logError("Mount startup failed. Cleaning up any mounts started by this request.")
            cleanupStartedMounts(mountSpecs)
            throw error
        }

        onMountedStateChanged?(isMounted)
    }

    func unmountAll() {
        RuntimeLog.info("Unmounting all. processCount=\(processes.count)")
        for mountPath in Array(processes.keys) {
            unmountDrive(mountPath: mountPath)
        }

        processes.removeAll()
        cleanupExistingAppProcesses()
        onMountedStateChanged?(false)
        RuntimeLog.info("Unmount all completed")
    }

    func cleanupTemporaryFiles() {
        RuntimeLog.info("cleanupTemporaryFiles called; persistent app support files retained at \(appSupportURL.path)")
    }

    private func buildMountSpecs(applicationKeyId: String, applicationKey: String, buckets: [BucketMount], googleDrive: GoogleDriveSettings, seedbox: SeedboxSettings) throws -> [MountSpec] {
        var specs: [MountSpec] = []
        let keyId = applicationKeyId.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = applicationKey.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalizedBuckets = buckets.filter {
            !$0.bucketName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !$0.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }

        if !normalizedBuckets.isEmpty {
            guard !keyId.isEmpty, !key.isEmpty else { throw MountError.missingCredentials }
            try ensureB2Config(applicationKeyId: keyId, applicationKey: key)
            logInfo("B2 rclone config written to: \(configURL.path)")

            for bucket in normalizedBuckets {
                let bucketName = bucket.bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
                let mountPath = expandPath(bucket.mountPath)
                guard !bucketName.isEmpty, !mountPath.isEmpty else { throw MountError.missingBuckets }
                specs.append(MountSpec(label: "B2 \(bucketName)", remotePath: "b2remote:\(bucketName)", mountPath: mountPath, volumeName: bucketName, vfsCacheMode: "writes", readOnly: false))
            }
        }

        var normalizedGoogleDrive = googleDrive
        normalizedGoogleDrive.remoteName = CloudProvider.defaultGoogleDriveRemoteName
        if hasConfigSection(getGoogleDriveRemoteName(normalizedGoogleDrive)) {
            let googleMountPath = expandPath(normalizedGoogleDrive.mountPath.isEmpty ? defaultGoogleDriveMountPath() : normalizedGoogleDrive.mountPath)
            guard !googleMountPath.isEmpty else { throw MountError.missingGoogleDriveMountPath }
            specs.append(MountSpec(label: "Google Drive", remotePath: buildGoogleDriveRemotePath(normalizedGoogleDrive), mountPath: googleMountPath, volumeName: "Google Drive", vfsCacheMode: "full", readOnly: false))
        }

        let normalizedSeedbox = normalizeSeedbox(seedbox)
        let seedboxHasSettings = hasSeedboxSettings(normalizedSeedbox)
        if seedboxHasSettings {
            guard isValid(seedbox: normalizedSeedbox) else { throw MountError.missingSeedboxCredentials }
            if !hasConfigSection(getSeedboxRemoteName(normalizedSeedbox)) {
                let savedPassword = (try? SeedboxCredentialStore.loadPassword()) ?? ""
                try configureSeedbox(normalizedSeedbox, password: savedPassword)
            }

            let seedboxMountPath = expandPath(normalizedSeedbox.mountPath.isEmpty ? defaultSeedboxMountPath() : normalizedSeedbox.mountPath)
            guard !seedboxMountPath.isEmpty else { throw MountError.missingSeedboxMountPath }
            specs.append(MountSpec(
                label: "Seedbox",
                remotePath: buildSeedboxRemotePath(normalizedSeedbox),
                mountPath: seedboxMountPath,
                volumeName: "Seedbox",
                vfsCacheMode: "full",
                readOnly: normalizedSeedbox.readOnly,
                extraArgs: seedboxLargeFileMountArgs()
            ))
        }

        return specs
    }

    private func validate(mountSpecs: [MountSpec]) throws {
        var seenMountPaths = Set<String>()
        var seenBuckets = Set<String>()

        for spec in mountSpecs {
            let mountPath = expandPath(spec.mountPath)
            guard !mountPath.isEmpty, mountPath.hasPrefix("/") else { throw MountError.invalidMountPath(spec.mountPath) }

            let pathKey = URL(fileURLWithPath: mountPath).standardizedFileURL.path.lowercased()
            guard seenMountPaths.insert(pathKey).inserted else { throw MountError.duplicateMountPath(mountPath) }

            if spec.remotePath.hasPrefix("b2remote:") {
                let bucketName = String(spec.remotePath.dropFirst("b2remote:".count))
                let bucketKey = bucketName.lowercased()
                guard seenBuckets.insert(bucketKey).inserted else { throw MountError.duplicateBucket(bucketName) }
            }
        }
    }

    private func findRclone() -> String? {
        let bundled = Bundle.main.resourceURL?.appendingPathComponent("rclone").path
        if let bundled, FileManager.default.isExecutableFile(atPath: bundled) {
            return bundled
        }

        let candidates = [
            "/opt/homebrew/bin/rclone",
            "/usr/local/bin/rclone",
            "/usr/bin/rclone"
        ]

        for candidate in candidates where FileManager.default.isExecutableFile(atPath: candidate) {
            return candidate
        }

        let path = ProcessInfo.processInfo.environment["PATH"] ?? ""
        for directory in path.split(separator: ":") {
            let candidate = String(directory) + "/rclone"
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }

        return nil
    }

    private func ensureConfigDirectory() throws {
        try FileManager.default.createDirectory(at: appSupportURL, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: cacheURL, withIntermediateDirectories: true)
    }

    private func ensureB2Config(applicationKeyId: String, applicationKey: String) throws {
        let lines = [
            "type = b2",
            "account = \(applicationKeyId)",
            "key = \(applicationKey)"
        ]
        try upsertConfigSection("b2remote", lines: lines)
    }

    private func mountRemote(rclonePath: String, spec: MountSpec) throws {
        let mountPath = expandPath(spec.mountPath)
        try FileManager.default.createDirectory(atPath: mountPath, withIntermediateDirectories: true)
        if isMountPoint(mountPath) {
            waitForMountPathToRelease(mountPath, timeout: 5)
        }

        guard !isMountPoint(mountPath) else {
            throw MountError.mountPathAlreadyMounted(mountPath)
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: rclonePath)
        var args = [
            "mount",
            spec.remotePath,
            mountPath,
            "--config", configURL.path,
            "--cache-dir", cacheURL.path,
            "--vfs-cache-mode", spec.vfsCacheMode,
            "--volname", spec.volumeName,
            "--links",
            "--log-level", "NOTICE"
        ]
        if spec.readOnly {
            args.append("--read-only")
        }
        args.append(contentsOf: spec.extraArgs)
        process.arguments = args

        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error

        stream(pipe: output, prefix: "[\(spec.label)]", source: .stdout)
        stream(pipe: error, prefix: "[\(spec.label)]", source: .stderr)

        process.terminationHandler = { [weak self] process in
            output.fileHandleForReading.readabilityHandler = nil
            error.fileHandleForReading.readabilityHandler = nil
            process.waitUntilExit()

            Task { @MainActor in
                self?.processes.removeValue(forKey: mountPath)
                if self?.intentionalStops.remove(mountPath) != nil {
                    self?.logInfo("Mount process stopped (unmounted) for \(spec.label).")
                    self?.onMountedStateChanged?(self?.isMounted == true)
                    return
                }

                if process.terminationStatus == 0 {
                    self?.logInfo("Mount process exited normally for \(spec.label).")
                } else {
                    self?.logError("Mount process exited with code \(process.terminationStatus) for \(spec.label).")
                }
                self?.onMountedStateChanged?(self?.isMounted == true)
            }
        }

        logInfo("Mounting \(spec.label) at \(mountPath)")
        RuntimeLog.info("Starting rclone process label=\(spec.label) mountPath=\(mountPath) args=\(redactArguments(process.arguments ?? []).joined(separator: " "))")
        try process.run()
        processes[mountPath] = process

        do {
            try waitForMountStartup(process: process, mountPath: mountPath, label: spec.label, timeout: mountStartupTimeout)
        } catch {
            cleanupFailedMountProcess(process: process, mountPath: mountPath, label: spec.label)
            throw error
        }
    }

    private func runRcloneToCompletion(rclonePath: String, args: [String], label: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: rclonePath)
        process.arguments = args

        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error

        stream(pipe: output, prefix: "[\(label)]", source: .stdout)
        stream(pipe: error, prefix: "[\(label)]", source: .stderr)

        RuntimeLog.info("Starting rclone command label=\(label) args=\(redactArguments(args).joined(separator: " "))")

        do {
            try process.run()
            process.waitUntilExit()
            output.fileHandleForReading.readabilityHandler = nil
            error.fileHandleForReading.readabilityHandler = nil
            if process.terminationStatus == 0 {
                RuntimeLog.info("rclone command completed successfully label=\(label)")
                return true
            }

            RuntimeLog.error("rclone command failed label=\(label) code=\(process.terminationStatus)")
            logError("\(label) exited with code \(process.terminationStatus).")
            return false
        } catch {
            RuntimeLog.error("Failed to run rclone command label=\(label) error=\(error.localizedDescription)")
            logError("Failed to run \(label): \(error.localizedDescription)")
            return false
        }
    }

    private func obscurePassword(rclonePath: String, password: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: rclonePath)
        process.arguments = ["obscure", "-"]

        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error

        do {
            try process.run()
            input.fileHandleForWriting.write(Data((password + "\n").utf8))
            input.fileHandleForWriting.closeFile()
            process.waitUntilExit()

            let outputData = output.fileHandleForReading.readDataToEndOfFile()
            let errorData = error.fileHandleForReading.readDataToEndOfFile()
            let obscured = String(data: outputData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""

            guard process.terminationStatus == 0, !obscured.isEmpty else {
                let errorText = String(data: errorData, encoding: .utf8) ?? ""
                RuntimeLog.error("rclone obscure failed code=\(process.terminationStatus) error=\(Self.redactSensitiveLine(errorText))")
                logError("Could not prepare the Seedbox password for rclone.")
                return nil
            }

            return obscured
        } catch {
            RuntimeLog.error("Failed to run rclone obscure: \(error.localizedDescription)")
            logError("Could not prepare the Seedbox password for rclone: \(error.localizedDescription)")
            return nil
        }
    }

    private func stream(pipe: Pipe, prefix: String, source: RcloneOutputSource) {
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            let lines = text.split(whereSeparator: \.isNewline)
            Task { @MainActor in
                for line in lines {
                    self?.logRcloneLine(String(line), prefix: prefix, source: source)
                }
            }
        }
    }

    private func unmountDrive(mountPath: String) {
        let expanded = expandPath(mountPath)
        if let process = processes.removeValue(forKey: expanded) {
            logInfo("Unmounting \(expanded)")
            intentionalStops.insert(expanded)
            _ = runUnmount(mountPath: expanded)
            waitForMountPathToRelease(expanded, timeout: 5)
            if process.isRunning {
                stop(process: process, label: expanded, timeout: 5)
            } else {
                process.waitUntilExit()
            }
        } else {
            _ = runUnmount(mountPath: expanded)
            waitForMountPathToRelease(expanded, timeout: 5)
        }
        onMountedStateChanged?(isMounted)
    }

    @discardableResult
    private func runUnmount(mountPath: String) -> Bool {
        guard !mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return true }
        guard isMountPoint(mountPath) else { return true }

        let attempts: [(String, [String])] = [
            ("/usr/sbin/diskutil", ["unmount", mountPath]),
            ("/sbin/umount", [mountPath]),
            ("/usr/sbin/diskutil", ["unmount", "force", mountPath]),
            ("/sbin/umount", ["-f", mountPath])
        ]

        for (executable, arguments) in attempts where runProcess(executable: executable, arguments: arguments) == 0 {
            return true
        }

        return false
    }

    private func runProcess(executable: String, arguments: [String]) -> Int32? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = Pipe()
        process.standardError = Pipe()

        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus
        } catch {
            RuntimeLog.error("Failed to run \(executable): \(error.localizedDescription)")
            return nil
        }
    }

    private func logRcloneLine(_ line: String, prefix: String, source: RcloneOutputSource) {
        let safeLine = Self.redactSensitiveLine(line)
        let message = "\(prefix) \(safeLine)"

        switch Self.classifyRcloneLine(safeLine, source: source) {
        case .info:
            logInfo(message)
        case .error:
            logError(message)
        case .hiddenErrorDetail:
            RuntimeLog.error(message)
        }
    }

    private func appOwnedRcloneProcesses() -> [RunningProcess] {
        let currentPid = ProcessInfo.processInfo.processIdentifier
        return listRunningProcesses().filter { process in
            guard process.pid != currentPid else { return false }

            let command = process.command.localizedLowercase
            let isRclone = command.contains("rclone")
            let usesAppConfig = process.command.contains(configURL.path)
            let isCurrentChild = process.ppid == currentPid && (isRclone || command == "(rclone)")

            return (isRclone && usesAppConfig) || isCurrentChild || isOrphanedExitingRclone(process)
        }
    }

    private func isOrphanedExitingRclone(_ process: RunningProcess) -> Bool {
        process.ppid == 1 &&
            process.user == NSUserName() &&
            process.command.localizedLowercase == "(rclone)" &&
            (process.state.contains("E") || process.state.contains("Z"))
    }

    private func listRunningProcesses() -> [RunningProcess] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-axo", "pid=,ppid=,stat=,user=,command="]

        let output = Pipe()
        process.standardOutput = output
        process.standardError = Pipe()

        do {
            try process.run()
        } catch {
            RuntimeLog.error("Failed to list running processes: \(error.localizedDescription)")
            return []
        }

        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard let text = String(data: data, encoding: .utf8) else { return [] }

        return text.split(whereSeparator: \.isNewline).compactMap { line in
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let parts = trimmed.split(separator: " ", maxSplits: 4, omittingEmptySubsequences: true)
            guard parts.count == 5,
                  let pid = pid_t(String(parts[0])),
                  let ppid = pid_t(String(parts[1])) else {
                return nil
            }
            return RunningProcess(
                pid: pid,
                ppid: ppid,
                state: String(parts[2]),
                user: String(parts[3]),
                command: String(parts[4]).trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
    }

    @discardableResult
    private func terminate(pid: pid_t, timeout: TimeInterval) -> Bool {
        if reapExitedChildProcess(pid: pid) {
            return true
        }

        guard isProcessRunning(pid) else { return true }

        RuntimeLog.info("Terminating app-owned rclone process pid=\(pid)")
        _ = Darwin.kill(pid, SIGTERM)

        if waitForProcessExit(pid: pid, timeout: timeout) {
            RuntimeLog.info("App-owned rclone process terminated pid=\(pid)")
            return true
        }

        RuntimeLog.error("Stale rclone process did not terminate gracefully; killing pid=\(pid)")
        _ = Darwin.kill(pid, SIGKILL)
        if !waitForProcessExit(pid: pid, timeout: 2) {
            RuntimeLog.error("Stale rclone process still appears to be running after SIGKILL pid=\(pid)")
            return false
        }

        return true
    }

    private func waitForProcessExit(pid: pid_t, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while isProcessRunning(pid) && Date() < deadline {
            if reapExitedChildProcess(pid: pid) {
                return true
            }
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }

        return reapExitedChildProcess(pid: pid) || !isProcessRunning(pid)
    }

    private func isProcessRunning(_ pid: pid_t) -> Bool {
        Darwin.kill(pid, 0) == 0 || errno == EPERM
    }

    private func waitForMountPathToRelease(_ mountPath: String, timeout: TimeInterval) {
        let deadline = Date().addingTimeInterval(timeout)
        while isMountPoint(mountPath) && Date() < deadline {
            _ = runUnmount(mountPath: mountPath)
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.1))
        }
    }

    private func waitForMountStartup(process: Process, mountPath: String, label: String, timeout: TimeInterval) throws {
        let deadline = Date().addingTimeInterval(timeout)

        while Date() < deadline {
            if isMountPoint(mountPath) {
                RuntimeLog.info("Mount startup completed label=\(label) mountPath=\(mountPath)")
                return
            }

            if !process.isRunning {
                process.waitUntilExit()
                processes.removeValue(forKey: mountPath)
                throw MountError.mountProcessExited(label, process.terminationStatus)
            }

            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.1))
        }

        throw MountError.mountStartupTimedOut(label)
    }

    private func cleanupStartedMounts(_ mountSpecs: [MountSpec]) {
        for spec in mountSpecs.reversed() {
            let mountPath = expandPath(spec.mountPath)
            unmountDrive(mountPath: mountPath)
        }
        cleanupExistingAppProcesses()
    }

    private func cleanupFailedMountProcess(process: Process, mountPath: String, label: String) {
        processes.removeValue(forKey: mountPath)
        intentionalStops.insert(mountPath)
        stop(process: process, label: label, timeout: 3)
        _ = runUnmount(mountPath: mountPath)
        waitForMountPathToRelease(mountPath, timeout: 5)
        intentionalStops.remove(mountPath)
    }

    private func stop(process: Process, label: String, timeout: TimeInterval) {
        guard process.isRunning else {
            process.waitUntilExit()
            return
        }

        process.terminate()
        if wait(process: process, timeout: timeout) {
            return
        }

        process.interrupt()
        if wait(process: process, timeout: 2) {
            return
        }

        RuntimeLog.error("rclone process did not stop gracefully for \(label); killing pid=\(process.processIdentifier)")
        _ = Darwin.kill(process.processIdentifier, SIGKILL)
        if !wait(process: process, timeout: 2) {
            RuntimeLog.error("rclone process still appears to be running after SIGKILL for \(label) pid=\(process.processIdentifier)")
        }
    }

    private func isMountPoint(_ path: String) -> Bool {
        var pathStat = stat()
        guard lstat(path, &pathStat) == 0 else { return false }

        let parentPath = (path as NSString).deletingLastPathComponent
        guard !parentPath.isEmpty, parentPath != path else { return true }

        var parentStat = stat()
        guard lstat(parentPath, &parentStat) == 0 else { return false }

        return pathStat.st_dev != parentStat.st_dev
    }

    private func wait(process: Process, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }

        if !process.isRunning {
            process.waitUntilExit()
        }

        return !process.isRunning
    }

    private func reapExitedChildProcess(pid: pid_t) -> Bool {
        var status: Int32 = 0
        return Darwin.waitpid(pid, &status, WNOHANG) == pid
    }

    private func upsertConfigSection(_ sectionName: String, lines sectionLines: [String]) throws {
        try ensureConfigDirectory()
        let existing = (try? String(contentsOf: configURL, encoding: .utf8)) ?? ""
        var lines = removeSection(from: existing.components(separatedBy: .newlines), sectionName: sectionName)

        while lines.last?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == true {
            lines.removeLast()
        }

        if !lines.isEmpty {
            lines.append("")
        }

        lines.append("[\(sectionName)]")
        lines.append(contentsOf: sectionLines)
        lines.append("")
        try lines.joined(separator: "\n").write(to: configURL, atomically: true, encoding: .utf8)
    }

    private func removeConfigSection(_ sectionName: String) throws {
        try ensureConfigDirectory()
        guard FileManager.default.fileExists(atPath: configURL.path) else { return }
        let existing = try String(contentsOf: configURL, encoding: .utf8)
        let lines = removeSection(from: existing.components(separatedBy: .newlines), sectionName: sectionName)
        try lines.joined(separator: "\n").write(to: configURL, atomically: true, encoding: .utf8)
    }

    private func hasConfigSection(_ sectionName: String) -> Bool {
        guard let existing = try? String(contentsOf: configURL, encoding: .utf8) else { return false }
        for line in existing.components(separatedBy: .newlines) {
            if readSectionName(line) == sectionName {
                return true
            }
        }
        return false
    }

    private func removeSection(from lines: [String], sectionName: String) -> [String] {
        var output: [String] = []
        var skipping = false

        for line in lines {
            if let currentSection = readSectionName(line) {
                skipping = currentSection.caseInsensitiveCompare(sectionName) == .orderedSame
            }

            if !skipping {
                output.append(line)
            }
        }

        while output.last?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == true {
            output.removeLast()
        }

        return output
    }

    private func readSectionName(_ line: String) -> String? {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 3, trimmed.hasPrefix("["), trimmed.hasSuffix("]") else { return nil }
        return String(trimmed.dropFirst().dropLast())
    }

    private func buildGoogleDriveRemotePath(_ googleDrive: GoogleDriveSettings) -> String {
        let remoteName = getGoogleDriveRemoteName(googleDrive)
        let remotePath = normalizeRemotePath(googleDrive.remotePath)
        return remotePath.isEmpty ? "\(remoteName):" : "\(remoteName):\(remotePath)"
    }

    private func getGoogleDriveRemoteName(_ googleDrive: GoogleDriveSettings) -> String {
        CloudProvider.defaultGoogleDriveRemoteName
    }

    private func buildSeedboxRemotePath(_ seedbox: SeedboxSettings) -> String {
        let remoteName = getSeedboxRemoteName(seedbox)
        let remotePath = normalizeRemotePath(seedbox.remotePath)
        return remotePath.isEmpty ? "\(remoteName):" : "\(remoteName):\(remotePath)"
    }

    private func getSeedboxRemoteName(_ seedbox: SeedboxSettings) -> String {
        CloudProvider.defaultSeedboxRemoteName
    }

    private func normalizeSeedbox(_ seedbox: SeedboxSettings) -> SeedboxSettings {
        var normalized = seedbox
        normalized.remoteName = CloudProvider.defaultSeedboxRemoteName
        normalized.host = SeedboxSettings.normalizeHost(normalized.host)
        normalized.username = normalized.username.trimmingCharacters(in: .whitespacesAndNewlines)
        normalized.remotePath = normalizeRemotePath(normalized.remotePath)
        if normalized.port <= 0 || normalized.port > 65535 {
            normalized.port = 21
        }
        if normalized.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            normalized.mountPath = defaultSeedboxMountPath()
        }
        return normalized
    }

    private func hasSeedboxSettings(_ seedbox: SeedboxSettings) -> Bool {
        !seedbox.host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
        !seedbox.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func isValid(seedbox: SeedboxSettings) -> Bool {
        let normalized = normalizeSeedbox(seedbox)
        return !normalized.host.isEmpty &&
               !normalized.username.isEmpty &&
               normalized.port > 0 &&
               normalized.port <= 65535 &&
               !expandPath(normalized.mountPath).isEmpty
    }

    private func seedboxLargeFileMountArgs() -> [String] {
        [
            "--buffer-size", "32M",
            "--vfs-read-ahead", "128M",
            "--vfs-read-chunk-size", "64M",
            "--vfs-read-chunk-size-limit", "1G",
            "--multi-thread-cutoff", "256M",
            "--multi-thread-streams", "4",
            "--vfs-fast-fingerprint"
        ]
    }

    private func normalizeRemotePath(_ path: String) -> String {
        var normalized = path.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\\", with: "/")
        while normalized.hasPrefix("/") || normalized.hasPrefix(":") {
            normalized.removeFirst()
        }
        return normalized
    }

    private func defaultGoogleDriveMountPath() -> String {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Drives")
            .appendingPathComponent("Google Drive")
            .path
    }

    private func defaultSeedboxMountPath() -> String {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Drives")
            .appendingPathComponent("Seedbox")
            .path
    }

    private func expandPath(_ path: String) -> String {
        NSString(string: path.trimmingCharacters(in: .whitespacesAndNewlines)).expandingTildeInPath
    }

    private func logInfo(_ message: String) {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        let line = "[\(formatter.string(from: Date()))] [INFO] \(message)"
        onLog?(line)
        RuntimeLog.info(message)
    }

    private func logError(_ message: String) {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        let line = "[\(formatter.string(from: Date()))] [ERROR] \(message)"
        onLog?(line)
        onError?(message)
        RuntimeLog.error(message)
    }

    private func redactArguments(_ args: [String]) -> [String] {
        args.map { Self.redactSensitiveLine($0) }
    }

    private static func redactSensitiveLine(_ line: String) -> String {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        let sensitiveKeys = [
            "token",
            "access_token",
            "refresh_token",
            "client_secret",
            "service_account_credentials",
            "pass",
            "password",
            "key",
            "account"
        ]

        for key in sensitiveKeys {
            if trimmed.lowercased().hasPrefix("\(key) =") || trimmed.lowercased().hasPrefix("\(key)=") {
                return "\(key) = <redacted>"
            }
        }

        if line.localizedCaseInsensitiveContains("\"access_token\"") ||
            line.localizedCaseInsensitiveContains("\"refresh_token\"") ||
            line.localizedCaseInsensitiveContains("\"client_secret\"") {
            return "<redacted sensitive output>"
        }

        return line
    }

    private static func classifyRcloneLine(_ line: String, source: RcloneOutputSource) -> RcloneLineSeverity {
        guard source == .stderr else { return .info }

        if isRcloneErrorDetailLine(line) {
            return .hiddenErrorDetail
        }

        switch timestampedRcloneSeverity(in: line) {
        case "NOTICE", "INFO":
            return .info
        case "ERROR", "CRITICAL", "FATAL":
            return .error
        default:
            return looksLikeRcloneError(line) ? .error : .info
        }
    }

    private static func timestampedRcloneSeverity(in line: String) -> String? {
        guard line.count > 20 else { return nil }

        let prefix = Array(line.prefix(20))
        guard prefix.count == 20,
              prefix[4] == "/",
              prefix[7] == "/",
              prefix[10] == " ",
              prefix[13] == ":",
              prefix[16] == ":",
              prefix[19] == " " else {
            return nil
        }

        let rest = String(line.dropFirst(20))
        for severity in ["NOTICE", "INFO", "ERROR", "CRITICAL", "FATAL"] where rest.hasPrefix(severity) {
            return severity
        }

        return nil
    }

    private static func looksLikeRcloneError(_ line: String) -> Bool {
        let lowercased = line.lowercased()
        return lowercased.contains(" error ") ||
               lowercased.contains(" critical ") ||
               lowercased.contains(" fatal ") ||
               lowercased.contains("failed") ||
               lowercased.contains("error")
    }

    private static func isRcloneErrorDetailLine(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        let exactDetailLines: Set<String> = ["Details:", "[", "]", "{", "}", "},", "],"]
        if exactDetailLines.contains(trimmed) {
            return true
        }

        let lowercased = trimmed.lowercased()
        return trimmed.hasPrefix("\"") ||
               lowercased.hasPrefix("@type") ||
               lowercased.hasPrefix("metadata") ||
               lowercased.hasPrefix(", ratelimitexceeded")
    }
}
