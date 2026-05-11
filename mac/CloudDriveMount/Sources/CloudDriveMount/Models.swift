import Foundation

struct BucketMount: Equatable {
    var bucketName: String
    var mountPath: String
}

enum MountError: LocalizedError {
    case missingRclone
    case missingMacFuse
    case missingCredentials
    case missingBuckets
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
        case .duplicateBucket(let bucket):
            return "Bucket '\(bucket)' is listed more than once."
        case .duplicateMountPath(let path):
            return "Mount folder '\(path)' is used more than once."
        case .invalidMountPath(let path):
            return "Mount folder '\(path)' is invalid."
        }
    }
}
