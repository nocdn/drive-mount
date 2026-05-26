import Foundation

enum CloudProvider: String, Codable {
    case backblazeB2 = "B2"
    case googleDrive = "GoogleDrive"

    static let defaultGoogleDriveRemoteName = "gdrive"
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

enum MountError: LocalizedError {
    case missingRclone
    case missingMacFuse
    case missingCredentials
    case missingBuckets
    case missingGoogleDriveMountPath
    case googleDriveNotConfigured
    case duplicateBucket(String)
    case duplicateMountPath(String)
    case invalidMountPath(String)

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
        case .duplicateBucket(let bucket):
            return "Bucket '\(bucket)' is listed more than once."
        case .duplicateMountPath(let path):
            return "Mount folder '\(path)' is used more than once."
        case .invalidMountPath(let path):
            return "Mount folder '\(path)' is invalid."
        }
    }
}
