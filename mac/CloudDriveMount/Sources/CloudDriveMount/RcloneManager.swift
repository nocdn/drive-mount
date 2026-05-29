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
    }

    private struct RunningProcess {
        var pid: pid_t
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
    var onMountedStateChanged: ((Bool) -> Void)?

    private var processes: [String: Process] = [:]
    private var intentionalStops = Set<String>()
    private let appSupportURL: URL
    private let configURL: URL
    private let cacheURL: URL

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
            "/Library/Filesystems/osxfuse.fs",
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

    func cleanupExistingAppProcesses() {
        let staleProcesses = appOwnedRcloneProcesses()
        guard !staleProcesses.isEmpty else {
            RuntimeLog.info("No stale app-owned rclone processes found")
            return
        }

        for process in staleProcesses {
            logInfo("Stopping stale rclone process PID=\(process.pid).")
            terminate(pid: process.pid, timeout: 5)
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

    func mount(applicationKeyId: String, applicationKey: String, buckets: [BucketMount], googleDrive: GoogleDriveSettings) throws {
        guard Self.isMacFuseInstalled() else { throw MountError.missingMacFuse }
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }

        try ensureConfigDirectory()
        try FileManager.default.createDirectory(at: cacheURL, withIntermediateDirectories: true)

        let mountSpecs = try buildMountSpecs(applicationKeyId: applicationKeyId, applicationKey: applicationKey, buckets: buckets, googleDrive: googleDrive)
        guard !mountSpecs.isEmpty else { throw MountError.missingBuckets }

        try validate(mountSpecs: mountSpecs)
        unmountAll()
        cleanupExistingAppProcesses()

        logInfo("Found rclone at: \(rclonePath)")
        for spec in mountSpecs {
            try mountRemote(rclonePath: rclonePath, spec: spec)
        }

        onMountedStateChanged?(isMounted)
    }

    func unmountAll() {
        RuntimeLog.info("Unmounting all. processCount=\(processes.count)")
        for mountPath in Array(processes.keys) {
            unmountDrive(mountPath: mountPath)
        }

        processes.removeAll()
        onMountedStateChanged?(false)
        RuntimeLog.info("Unmount all completed")
    }

    func cleanupTemporaryFiles() {
        RuntimeLog.info("cleanupTemporaryFiles called; persistent app support files retained at \(appSupportURL.path)")
    }

    private func buildMountSpecs(applicationKeyId: String, applicationKey: String, buckets: [BucketMount], googleDrive: GoogleDriveSettings) throws -> [MountSpec] {
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
                specs.append(MountSpec(label: "B2 \(bucketName)", remotePath: "b2remote:\(bucketName)", mountPath: mountPath, volumeName: bucketName, vfsCacheMode: "writes"))
            }
        }

        var normalizedGoogleDrive = googleDrive
        normalizedGoogleDrive.remoteName = CloudProvider.defaultGoogleDriveRemoteName
        if hasConfigSection(getGoogleDriveRemoteName(normalizedGoogleDrive)) {
            let googleMountPath = expandPath(normalizedGoogleDrive.mountPath.isEmpty ? defaultGoogleDriveMountPath() : normalizedGoogleDrive.mountPath)
            guard !googleMountPath.isEmpty else { throw MountError.missingGoogleDriveMountPath }
            specs.append(MountSpec(label: "Google Drive", remotePath: buildGoogleDriveRemotePath(normalizedGoogleDrive), mountPath: googleMountPath, volumeName: "Google Drive", vfsCacheMode: "full"))
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
        process.arguments = [
            "mount",
            spec.remotePath,
            mountPath,
            "--config", configURL.path,
            "--cache-dir", cacheURL.path,
            "--vfs-cache-mode", spec.vfsCacheMode,
            "--volname", spec.volumeName,
            "--links",
            "--log-level", "INFO"
        ]

        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error

        stream(pipe: output, prefix: "[\(spec.label)]", source: .stdout)
        stream(pipe: error, prefix: "[\(spec.label)]", source: .stderr)

        process.terminationHandler = { [weak self] process in
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
            if process.isRunning {
                process.terminate()
                _ = wait(process: process, timeout: 5)
                if process.isRunning {
                    process.interrupt()
                }
            }
        }

        runUnmount(mountPath: expanded)
        onMountedStateChanged?(isMounted)
    }

    private func runUnmount(mountPath: String) {
        guard !mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }

        let unmount = Process()
        unmount.executableURL = URL(fileURLWithPath: "/sbin/umount")
        unmount.arguments = [mountPath]
        try? unmount.run()
        unmount.waitUntilExit()
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
        listRunningProcesses().filter { process in
            process.pid != ProcessInfo.processInfo.processIdentifier &&
            process.command.localizedCaseInsensitiveContains("rclone") &&
            process.command.contains(configURL.path)
        }
    }

    private func listRunningProcesses() -> [RunningProcess] {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = ["-axo", "pid=,command="]

        let output = Pipe()
        process.standardOutput = output
        process.standardError = Pipe()

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            RuntimeLog.error("Failed to list running processes: \(error.localizedDescription)")
            return []
        }

        let data = output.fileHandleForReading.readDataToEndOfFile()
        guard let text = String(data: data, encoding: .utf8) else { return [] }

        return text.split(whereSeparator: \.isNewline).compactMap { line in
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            let parts = trimmed.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
            guard parts.count == 2, let pid = pid_t(String(parts[0])) else { return nil }
            return RunningProcess(pid: pid, command: String(parts[1]))
        }
    }

    private func terminate(pid: pid_t, timeout: TimeInterval) {
        guard isProcessRunning(pid) else { return }

        RuntimeLog.info("Terminating app-owned rclone process pid=\(pid)")
        _ = Darwin.kill(pid, SIGTERM)

        if waitForProcessExit(pid: pid, timeout: timeout) {
            RuntimeLog.info("App-owned rclone process terminated pid=\(pid)")
            return
        }

        RuntimeLog.error("Stale rclone process did not terminate gracefully; killing pid=\(pid)")
        _ = Darwin.kill(pid, SIGKILL)
        _ = waitForProcessExit(pid: pid, timeout: 2)
    }

    private func waitForProcessExit(pid: pid_t, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while isProcessRunning(pid) && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }

        return !isProcessRunning(pid)
    }

    private func isProcessRunning(_ pid: pid_t) -> Bool {
        Darwin.kill(pid, 0) == 0 || errno == EPERM
    }

    private func waitForMountPathToRelease(_ mountPath: String, timeout: TimeInterval) {
        let deadline = Date().addingTimeInterval(timeout)
        while isMountPoint(mountPath) && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.1))
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

        return !process.isRunning
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
