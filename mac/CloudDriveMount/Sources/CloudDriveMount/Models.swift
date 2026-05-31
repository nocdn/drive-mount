import Foundation

enum CloudProvider: String, Codable {
    case backblazeB2 = "B2"
    case googleDrive = "GoogleDrive"
    case seedbox = "Seedbox"

    static let defaultGoogleDriveRemoteName = "gdrive"
    static let defaultSeedboxRemoteName = "seedbox"
}

struct BucketMount: Codable, Equatable {
    var bucketName: String
    var mountPath: String
}

struct GoogleDriveSettings: Codable, Equatable {
    var remoteName: String = CloudProvider.defaultGoogleDriveRemoteName
    var remotePath: String = ""
    var rootFolderId: String = ""
    var mountPath: String = ""
}

struct SeedboxSettings: Codable, Equatable {
    var remoteName: String = CloudProvider.defaultSeedboxRemoteName
    var host: String = ""
    var username: String = ""
    var port: Int = 21
    var remotePath: String = "downloads"
    var mountPath: String = ""
    var allowUnverifiedCertificate: Bool = true
    var readOnly: Bool = true

    static func normalizeHost(_ host: String) -> String {
        var normalized = host.trimmingCharacters(in: .whitespacesAndNewlines)
        let schemes = ["https://", "http://", "ftps://", "ftp://"]
        for scheme in schemes where normalized.lowercased().hasPrefix(scheme) {
            normalized = String(normalized.dropFirst(scheme.count))
                .trimmingCharacters(in: .whitespacesAndNewlines)
            break
        }

        while normalized.hasSuffix("/") {
            normalized.removeLast()
        }

        return normalized.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

enum MountError: LocalizedError {
    case missingRclone
    case missingMacFuse
    case missingCredentials
    case missingBuckets
    case missingGoogleDriveMountPath
    case googleDriveNotConfigured
    case missingSeedboxCredentials
    case missingSeedboxMountPath
    case seedboxNotConfigured
    case invalidSeedboxPort
    case duplicateBucket(String)
    case duplicateMountPath(String)
    case mountPathAlreadyMounted(String)
    case invalidMountPath(String)
    case mountProcessExited(String, Int32)
    case mountStartupTimedOut(String)
    case staleRcloneProcesses([pid_t])

    var errorDescription: String? {
        switch self {
        case .missingRclone:
            return "rclone was not found. Bundle rclone with the app or install it with Homebrew."
        case .missingMacFuse:
            return "macFUSE is not installed or has not been enabled."
        case .missingCredentials:
            return "Enter your Backblaze B2 Application Key ID and Application Key."
        case .missingBuckets:
            return "Add at least one bucket and mount folder."
        case .missingGoogleDriveMountPath:
            return "Enter a mount folder for Google Drive."
        case .googleDriveNotConfigured:
            return "Google Drive is not connected. Click Connect Google Drive first."
        case .missingSeedboxCredentials:
            return "Enter your Seedbox host, username, and FTPS password."
        case .missingSeedboxMountPath:
            return "Enter a mount folder for Seedbox."
        case .seedboxNotConfigured:
            return "Seedbox is not configured. Enter your FTPS password and test the connection first."
        case .invalidSeedboxPort:
            return "Seedbox port must be between 1 and 65535."
        case .duplicateBucket(let bucket):
            return "Bucket '\(bucket)' is listed more than once."
        case .duplicateMountPath(let path):
            return "Mount folder '\(path)' is used more than once."
        case .mountPathAlreadyMounted(let path):
            return "Mount folder '\(path)' is already mounted. Unmount it or choose another folder."
        case .invalidMountPath(let path):
            return "Mount folder '\(path)' is invalid."
        case .mountProcessExited(let label, let code):
            return "\(label) failed to start mounting. rclone exited with code \(code)."
        case .mountStartupTimedOut(let label):
            return "\(label) did not finish mounting in time. The partial mount was cleaned up."
        case .staleRcloneProcesses(let pids):
            let pidList = pids.map { String($0) }.joined(separator: ", ")
            return "Stale rclone mount processes could not be stopped: \(pidList). Restart macOS or reload macFUSE before mounting again."
        }
    }
}
