import Foundation
import os

struct Diagnostics: Sendable {
    static let shared = Diagnostics()

    private let logger = Logger(subsystem: AppConstants.appBundleIdentifier, category: "runtime")

    func info(_ event: String, area: String, fields: [String: String] = [:]) {
        write(level: "info", area: area, event: event, fields: fields, error: nil)
    }

    func error(_ event: String, area: String, error: Error, fields: [String: String] = [:]) {
        write(level: "error", area: area, event: event, fields: fields, error: error)
    }

    private func write(level: String, area: String, event: String, fields: [String: String], error: Error?) {
        let safeFields = fields.mapValues { value in
            String(value.prefix(160))
        }
        let payload = DiagnosticEvent(
            ts: ISO8601DateFormatter().string(from: Date()),
            level: level,
            area: area,
            event: event,
            fields: safeFields,
            error: error.map { DiagnosticError(domain: String(describing: type(of: $0)), message: String(describing: $0).redactedSecretText) }
        )

        let fieldSummary = safeFields
            .map { "\($0.key)=\($0.value)" }
            .sorted()
            .joined(separator: " ")
        let errorSummary = error.map { " error=\(String(describing: $0).redactedSecretText)" } ?? ""
        let line = "[\(area)] \(event) \(fieldSummary)\(errorSummary)"
        if level == "error" {
            logger.error("\(line, privacy: .public)")
        } else {
            logger.info("\(line, privacy: .public)")
        }

        guard let data = try? JSONEncoder().encode(payload),
              let line = String(data: data, encoding: .utf8) else {
            return
        }

        do {
            let url = try diagnosticsURL()
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if !FileManager.default.fileExists(atPath: url.path) {
                FileManager.default.createFile(atPath: url.path, contents: nil)
            }
            let handle = try FileHandle(forWritingTo: url)
            try handle.seekToEnd()
            try handle.write(contentsOf: Data((line + "\n").utf8))
            try handle.close()
        } catch {
            logger.error("diagnostics.write.failed")
        }
    }

    private func diagnosticsURL() throws -> URL {
        let baseURL = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppConstants.appGroupIdentifier)
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return baseURL
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Logs", isDirectory: true)
            .appendingPathComponent("Diagnostics", isDirectory: true)
            .appendingPathComponent(AppConstants.diagnosticsFileName)
    }
}

private struct DiagnosticEvent: Codable {
    var ts: String
    var level: String
    var area: String
    var event: String
    var fields: [String: String]
    var error: DiagnosticError?
}

private struct DiagnosticError: Codable {
    var domain: String
    var message: String
}

private extension String {
    var redactedSecretText: String {
        replacingOccurrences(of: #"(?i)(token|password|key|secret)[^,\s\]]*"#, with: "$1=<redacted>", options: .regularExpression)
    }
}
