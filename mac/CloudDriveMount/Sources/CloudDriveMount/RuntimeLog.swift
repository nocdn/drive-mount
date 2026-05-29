import Foundation

enum RuntimeLog {
    private static let lock = NSLock()
    private static let maxLogSizeBytes: UInt64 = 5 * 1024 * 1024
    private static let formatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss"
        return formatter
    }()

    static var logDirectory: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library")
            .appendingPathComponent("Logs")
            .appendingPathComponent("CloudDriveMount")
    }

    static var logFile: URL {
        logDirectory.appendingPathComponent("app.log")
    }

    static var oldLogFile: URL {
        logDirectory.appendingPathComponent("app.old.log")
    }

    static func info(_ message: String) {
        write(level: "INFO", message)
    }

    static func error(_ message: String) {
        write(level: "ERROR", message)
    }

    static func write(level: String, _ message: String) {
        lock.lock()
        defer { lock.unlock() }

        do {
            try FileManager.default.createDirectory(at: logDirectory, withIntermediateDirectories: true)
            try rotateIfNeeded()

            let line = "[\(formatter.string(from: Date()))] [\(level)] \(message)\n"
            if FileManager.default.fileExists(atPath: logFile.path),
               let handle = try? FileHandle(forWritingTo: logFile) {
                defer { try? handle.close() }
                try handle.seekToEnd()
                try handle.write(contentsOf: Data(line.utf8))
            } else {
                try Data(line.utf8).write(to: logFile, options: .atomic)
            }
        } catch {
            NSLog("CloudDriveMount log write failed: %@", error.localizedDescription)
        }
    }

    static func clear() {
        lock.lock()
        defer { lock.unlock() }

        do {
            try FileManager.default.createDirectory(at: logDirectory, withIntermediateDirectories: true)
            try Data().write(to: logFile, options: .atomic)

            if FileManager.default.fileExists(atPath: oldLogFile.path) {
                try FileManager.default.removeItem(at: oldLogFile)
            }
        } catch {
            NSLog("CloudDriveMount log clear failed: %@", error.localizedDescription)
        }
    }

    private static func rotateIfNeeded() throws {
        guard FileManager.default.fileExists(atPath: logFile.path) else { return }

        let attributes = try FileManager.default.attributesOfItem(atPath: logFile.path)
        let size = (attributes[.size] as? NSNumber)?.uint64Value ?? 0
        guard size > maxLogSizeBytes else { return }

        if FileManager.default.fileExists(atPath: oldLogFile.path) {
            try FileManager.default.removeItem(at: oldLogFile)
        }

        try FileManager.default.moveItem(at: logFile, to: oldLogFile)
    }
}
