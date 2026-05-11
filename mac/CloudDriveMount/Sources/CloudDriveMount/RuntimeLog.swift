import Foundation

enum RuntimeLog {
    private static let lock = NSLock()
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
            let line = "[\(formatter.string(from: Date()))] [\(level)] \(message)\n"
            if FileManager.default.fileExists(atPath: logFile.path),
               let handle = try? FileHandle(forWritingTo: logFile) {
                try handle.seekToEnd()
                try handle.write(contentsOf: Data(line.utf8))
                try handle.close()
            } else {
                try Data(line.utf8).write(to: logFile, options: .atomic)
            }
        } catch {
            NSLog("CloudDriveMount log write failed: %@", error.localizedDescription)
        }
    }
}
