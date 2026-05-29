import AppKit
import Foundation

final class ErrorNotificationManager: NSObject, NSUserNotificationCenterDelegate {
    private var lastMessage = ""
    private var lastNotificationAt = Date.distantPast

    override init() {
        super.init()
        NSUserNotificationCenter.default.delegate = self
    }

    func showError(_ message: String) {
        guard shouldShowNotification(for: message) else { return }

        let notification = NSUserNotification()
        notification.title = "Cloud Drive Mount Error"
        notification.informativeText = Self.notificationBody(for: message)
        notification.soundName = NSUserNotificationDefaultSoundName
        NSUserNotificationCenter.default.deliver(notification)
    }

    func userNotificationCenter(
        _ center: NSUserNotificationCenter,
        shouldPresent notification: NSUserNotification
    ) -> Bool {
        true
    }

    private func shouldShowNotification(for message: String) -> Bool {
        let now = Date()
        let trimmed = Self.stripRcloneLabel(message).trimmingCharacters(in: .whitespacesAndNewlines)

        if Self.isRcloneErrorDetailLine(trimmed) {
            return false
        }

        if message == lastMessage && now.timeIntervalSince(lastNotificationAt) < 60 {
            return false
        }

        if message.range(of: "Mount process exited with code", options: .caseInsensitive) != nil &&
            now.timeIntervalSince(lastNotificationAt) < 10 {
            return false
        }

        lastMessage = message
        lastNotificationAt = now
        return true
    }

    private static func notificationBody(for message: String) -> String {
        let body = stripRcloneLabel(message).trimmingCharacters(in: .whitespacesAndNewlines)
        guard body.count > 240 else { return body }

        let endIndex = body.index(body.startIndex, offsetBy: 237)
        return String(body[..<endIndex]) + "..."
    }

    private static func stripRcloneLabel(_ message: String) -> String {
        let trimmed = message.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("["),
              let endIndex = trimmed.firstIndex(of: "]") else {
            return trimmed
        }

        return String(trimmed[trimmed.index(after: endIndex)...])
    }

    private static func isRcloneErrorDetailLine(_ line: String) -> Bool {
        let exactDetailLines: Set<String> = ["Details:", "[", "]", "{", "}", "},", "],"]
        if exactDetailLines.contains(line) {
            return true
        }

        let lowercased = line.lowercased()
        return line.hasPrefix("\"") ||
            lowercased.hasPrefix("@type") ||
            lowercased.hasPrefix("metadata") ||
            lowercased.hasPrefix(", ratelimitexceeded")
    }
}
