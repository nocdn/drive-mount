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
            let bucket = b2.normalizedBucketNames.first ?? ""
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

    func scopedToB2Bucket(_ bucketName: String) -> CloudConnection {
        var copy = self
        copy.b2.bucketNames = [bucketName]
        copy.displayName = bucketName
        return copy
    }
}

struct B2ConnectionSettings: Codable, Equatable, Sendable {
    var applicationKeyID: String
    var applicationKey: String
    var bucketNames: [String]

    init(
        applicationKeyID: String = "",
        applicationKey: String = "",
        bucketName: String = "",
        bucketNames: [String]? = nil
    ) {
        self.applicationKeyID = applicationKeyID
        self.applicationKey = applicationKey
        if let bucketNames {
            self.bucketNames = bucketNames
        } else {
            self.bucketNames = [bucketName]
        }
    }

    /// First configured bucket. Kept so existing B2 listing/download code can stay single-bucket scoped.
    var bucketName: String {
        get { normalizedBucketNames.first ?? bucketNames.first?.trimmed ?? "" }
        set { bucketNames = [newValue] }
    }

    var normalizedBucketNames: [String] {
        var seen = Set<String>()
        return bucketNames.compactMap { name in
            let trimmed = name.trimmed
            guard !trimmed.isEmpty, seen.insert(trimmed.lowercased()).inserted else {
                return nil
            }
            return trimmed
        }
    }

    func normalized() -> B2ConnectionSettings {
        let names = normalizedBucketNames
        return B2ConnectionSettings(
            applicationKeyID: applicationKeyID.trimmed,
            applicationKey: applicationKey.trimmed,
            bucketNames: names.isEmpty ? [""] : names
        )
    }

    enum CodingKeys: String, CodingKey {
        case applicationKeyID
        case applicationKey
        case bucketName
        case bucketNames
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        applicationKeyID = try container.decodeIfPresent(String.self, forKey: .applicationKeyID) ?? ""
        applicationKey = try container.decodeIfPresent(String.self, forKey: .applicationKey) ?? ""
        if let names = try container.decodeIfPresent([String].self, forKey: .bucketNames) {
            bucketNames = names
        } else if let name = try container.decodeIfPresent(String.self, forKey: .bucketName) {
            bucketNames = [name]
        } else {
            bucketNames = [""]
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(applicationKeyID, forKey: .applicationKeyID)
        try container.encode(applicationKey, forKey: .applicationKey)
        try container.encode(bucketNames, forKey: .bucketNames)
        try container.encode(bucketName, forKey: .bucketName)
    }
}

enum B2FileProviderDomainIdentity {
    static func identifier(connectionID: String, bucketName: String) -> String {
        "b2|\(connectionID)|\(bucketName)"
    }

    static func parse(_ rawValue: String) -> (connectionID: String, bucketName: String)? {
        let parts = rawValue.split(separator: "|", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 3, parts[0] == "b2", !parts[1].isEmpty, !parts[2].isEmpty else {
            return nil
        }
        return (parts[1], parts[2])
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
    var port: Int = 22
    var remotePath: String = "downloads"
    var readOnly: Bool = true

    /// Port 21 is the old FTPS default from desktop. iOS Seedbox uses SFTP, which is almost always 22.
    var effectiveSFTPPort: Int {
        port == 21 ? 22 : port
    }

    func normalized() -> SeedboxConnectionSettings {
        SeedboxConnectionSettings(
            host: host.normalizedSeedboxHost,
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

    var normalizedSeedboxHost: String {
        var value = trimmed
        let schemes = ["sftp://", "ftps://", "ftp://", "https://", "http://"]
        for scheme in schemes {
            if value.lowercased().hasPrefix(scheme) {
                value = String(value.dropFirst(scheme.count))
                break
            }
        }
        while value.hasSuffix("/") {
            value.removeLast()
        }
        return value
    }

    func normalizedRemotePath(defaultValue: String = "") -> String {
        let parts = split(separator: "/").map(String.init).filter { !$0.trimmed.isEmpty }
        let normalized = parts.joined(separator: "/")
        return normalized.isEmpty ? defaultValue : normalized
    }
}
