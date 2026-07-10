import Foundation

struct CloudConnection: Codable, Equatable, Identifiable, Sendable {
    var id: String
    var provider: CloudProvider
    var displayName: String
    var isEnabled: Bool
    var createdAt: Date
    var updatedAt: Date
    var b2: B2ConnectionSettings
    var googleDrive: GoogleDriveConnectionSettings
    var oneDrive: OneDriveConnectionSettings
    var seedbox: SeedboxConnectionSettings

    init(
        id: String = UUID().uuidString,
        provider: CloudProvider,
        displayName: String = "",
        isEnabled: Bool = true,
        createdAt: Date = Date(),
        updatedAt: Date = Date(),
        b2: B2ConnectionSettings = B2ConnectionSettings(),
        googleDrive: GoogleDriveConnectionSettings = GoogleDriveConnectionSettings(),
        oneDrive: OneDriveConnectionSettings = OneDriveConnectionSettings(),
        seedbox: SeedboxConnectionSettings = SeedboxConnectionSettings()
    ) {
        self.id = id
        self.provider = provider
        self.displayName = displayName
        self.isEnabled = isEnabled
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.b2 = b2
        self.googleDrive = googleDrive
        self.oneDrive = oneDrive
        self.seedbox = seedbox
    }

    var effectiveDisplayName: String {
        let trimmed = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            return trimmed
        }

        switch provider {
        case .backblazeB2:
            let bucket = b2.bucketName.trimmingCharacters(in: .whitespacesAndNewlines)
            return bucket.isEmpty ? provider.defaultConnectionName : bucket
        case .googleDrive, .oneDrive, .seedbox:
            return provider.defaultConnectionName
        }
    }

    var hasMinimumConfiguration: Bool {
        switch provider {
        case .backblazeB2:
            !b2.applicationKeyID.trimmed.isEmpty && !b2.applicationKey.trimmed.isEmpty
        case .googleDrive:
            !googleDrive.accessToken.trimmed.isEmpty
        case .oneDrive:
            !oneDrive.accessToken.trimmed.isEmpty
        case .seedbox:
            !seedbox.host.trimmed.isEmpty && !seedbox.username.trimmed.isEmpty && !seedbox.password.trimmed.isEmpty
        }
    }

    func normalized(now: Date = Date()) -> CloudConnection {
        var copy = self
        copy.displayName = displayName.trimmed
        copy.b2 = b2.normalized()
        copy.googleDrive = googleDrive.normalized()
        copy.oneDrive = oneDrive.normalized()
        copy.seedbox = seedbox.normalized()
        copy.updatedAt = now
        return copy
    }
}

struct B2ConnectionSettings: Codable, Equatable, Sendable {
    var applicationKeyID: String = ""
    var applicationKey: String = ""
    var bucketName: String = ""

    func normalized() -> B2ConnectionSettings {
        B2ConnectionSettings(
            applicationKeyID: applicationKeyID.trimmed,
            applicationKey: applicationKey.trimmed,
            bucketName: bucketName.trimmed
        )
    }
}

struct GoogleDriveConnectionSettings: Codable, Equatable, Sendable {
    var accessToken: String = ""
    var rootFolderID: String = ""

    func normalized() -> GoogleDriveConnectionSettings {
        GoogleDriveConnectionSettings(
            accessToken: accessToken.trimmed,
            rootFolderID: rootFolderID.trimmed
        )
    }
}

struct OneDriveConnectionSettings: Codable, Equatable, Sendable {
    var accessToken: String = ""
    var rootItemID: String = ""

    func normalized() -> OneDriveConnectionSettings {
        OneDriveConnectionSettings(
            accessToken: accessToken.trimmed,
            rootItemID: rootItemID.trimmed
        )
    }
}

struct SeedboxConnectionSettings: Codable, Equatable, Sendable {
    var host: String = ""
    var username: String = ""
    var password: String = ""
    var port: Int = 21
    var remotePath: String = "downloads"
    var readOnly: Bool = true

    func normalized() -> SeedboxConnectionSettings {
        SeedboxConnectionSettings(
            host: host.trimmed
                .replacingOccurrences(of: "ftp://", with: "")
                .replacingOccurrences(of: "ftps://", with: ""),
            username: username.trimmed,
            password: password.trimmed,
            port: max(1, min(port, 65535)),
            remotePath: remotePath.normalizedRemotePath(defaultValue: "downloads"),
            readOnly: readOnly
        )
    }
}

extension String {
    var trimmed: String {
        trimmingCharacters(in: .whitespacesAndNewlines)
    }

    func normalizedRemotePath(defaultValue: String = "") -> String {
        let parts = split(separator: "/").map(String.init).filter { !$0.trimmed.isEmpty }
        let normalized = parts.joined(separator: "/")
        return normalized.isEmpty ? defaultValue : normalized
    }
}
