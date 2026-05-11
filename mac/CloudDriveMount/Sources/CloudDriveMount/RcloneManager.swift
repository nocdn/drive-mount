import Foundation

@MainActor
final class RcloneManager {
    var onLog: ((String) -> Void)?
    var onMountedStateChanged: ((Bool) -> Void)?

    private var processes: [String: Process] = [:]
    private let tempRoot: URL
    private let configURL: URL
    private let cacheURL: URL

    var isMounted: Bool {
        processes.values.contains { $0.isRunning }
    }

    init() {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CloudDriveMount")
            .appendingPathComponent(UUID().uuidString)
        tempRoot = root
        configURL = root.appendingPathComponent("rclone.conf")
        cacheURL = root.appendingPathComponent("cache")
        RuntimeLog.info("RcloneManager initialized tempRoot=\(tempRoot.path)")
    }

    static func isMacFuseInstalled() -> Bool {
        let paths = [
            "/Library/Filesystems/macfuse.fs",
            "/Library/Filesystems/osxfuse.fs",
            "/usr/local/lib/libfuse.2.dylib",
            "/opt/homebrew/lib/libfuse.2.dylib"
        ]

        return paths.contains { FileManager.default.fileExists(atPath: $0) }
    }

    func mount(applicationKeyId: String, applicationKey: String, buckets: [BucketMount]) throws {
        let keyId = applicationKeyId.trimmingCharacters(in: .whitespacesAndNewlines)
        let key = applicationKey.trimmingCharacters(in: .whitespacesAndNewlines)

        guard !keyId.isEmpty, !key.isEmpty else { throw MountError.missingCredentials }
        guard !buckets.isEmpty else { throw MountError.missingBuckets }
        guard Self.isMacFuseInstalled() else { throw MountError.missingMacFuse }
        guard let rclonePath = findRclone() else { throw MountError.missingRclone }

        RuntimeLog.info("Mount validation passed. bucketCount=\(buckets.count) rclonePath=\(rclonePath)")
        try validate(buckets: buckets)
        try writeConfig(applicationKeyId: keyId, applicationKey: key)
        unmountAll()

        logInfo("Found rclone at: \(rclonePath)")
        for bucket in buckets {
            try mountBucket(rclonePath: rclonePath, bucket: bucket)
        }

        onMountedStateChanged?(isMounted)
    }

    func unmountAll() {
        RuntimeLog.info("Unmounting all. processCount=\(processes.count)")
        for (mountPath, process) in processes {
            logInfo("Unmounting \(mountPath)")
            if process.isRunning {
                process.terminate()
                _ = wait(process: process, timeout: 5)
                if process.isRunning {
                    process.interrupt()
                }
            }

            runUnmount(mountPath: mountPath)
        }

        processes.removeAll()
        onMountedStateChanged?(false)
        RuntimeLog.info("Unmount all completed")
    }

    func cleanupTemporaryFiles() {
        RuntimeLog.info("Cleaning temporary files at \(tempRoot.path)")
        try? FileManager.default.removeItem(at: tempRoot)
    }

    private func validate(buckets: [BucketMount]) throws {
        var seenBuckets = Set<String>()
        var seenPaths = Set<String>()

        for bucket in buckets {
            let bucketName = bucket.bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
            let mountPath = NSString(string: bucket.mountPath.trimmingCharacters(in: .whitespacesAndNewlines)).expandingTildeInPath

            guard !bucketName.isEmpty, !mountPath.isEmpty else { throw MountError.missingBuckets }
            guard mountPath.hasPrefix("/") else { throw MountError.invalidMountPath(mountPath) }

            let bucketKey = bucketName.lowercased()
            guard seenBuckets.insert(bucketKey).inserted else { throw MountError.duplicateBucket(bucketName) }

            let pathKey = URL(fileURLWithPath: mountPath).standardizedFileURL.path.lowercased()
            guard seenPaths.insert(pathKey).inserted else { throw MountError.duplicateMountPath(mountPath) }
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

    private func writeConfig(applicationKeyId: String, applicationKey: String) throws {
        try FileManager.default.createDirectory(at: tempRoot, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: cacheURL, withIntermediateDirectories: true)

        let config = """
        [b2remote]
        type = b2
        account = \(applicationKeyId)
        key = \(applicationKey)

        """

        try config.write(to: configURL, atomically: true, encoding: .utf8)
        logInfo("Temporary rclone config written.")
    }

    private func mountBucket(rclonePath: String, bucket: BucketMount) throws {
        let bucketName = bucket.bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
        let mountPath = NSString(string: bucket.mountPath.trimmingCharacters(in: .whitespacesAndNewlines)).expandingTildeInPath

        try FileManager.default.createDirectory(atPath: mountPath, withIntermediateDirectories: true)

        let process = Process()
        process.executableURL = URL(fileURLWithPath: rclonePath)
        process.arguments = [
            "mount",
            "b2remote:\(bucketName)",
            mountPath,
            "--config", configURL.path,
            "--cache-dir", cacheURL.path,
            "--vfs-cache-mode", "writes",
            "--volname", bucketName,
            "--log-level", "INFO"
        ]

        let output = Pipe()
        let error = Pipe()
        process.standardOutput = output
        process.standardError = error

        stream(pipe: output, prefix: "[\(bucketName)]")
        stream(pipe: error, prefix: "[\(bucketName)]")

        process.terminationHandler = { [weak self] process in
            Task { @MainActor in
                self?.processes.removeValue(forKey: mountPath)
                self?.logInfo("Mount process exited for \(bucketName) with code \(process.terminationStatus).")
                self?.onMountedStateChanged?(self?.isMounted == true)
            }
        }

        logInfo("Mounting \(bucketName) at \(mountPath)")
        RuntimeLog.info("Starting rclone process bucket=\(bucketName) mountPath=\(mountPath) args=\(process.arguments?.joined(separator: " ") ?? "")")
        try process.run()
        processes[mountPath] = process
    }

    private func stream(pipe: Pipe, prefix: String) {
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty, let text = String(data: data, encoding: .utf8) else { return }
            let lines = text.split(whereSeparator: \.isNewline)
            Task { @MainActor in
                for line in lines {
                    self?.logInfo("\(prefix) \(line)")
                }
            }
        }
    }

    private func runUnmount(mountPath: String) {
        let unmount = Process()
        unmount.executableURL = URL(fileURLWithPath: "/sbin/umount")
        unmount.arguments = [mountPath]
        try? unmount.run()
        unmount.waitUntilExit()
    }

    private func wait(process: Process, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.05))
        }

        return !process.isRunning
    }

    private func logInfo(_ message: String) {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        onLog?("[\(formatter.string(from: Date()))] [INFO] \(message)")
    }
}
