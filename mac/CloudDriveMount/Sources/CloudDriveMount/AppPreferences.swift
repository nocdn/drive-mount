import Foundation
import Security

enum AppPreferences {
    private static let startAtLoginKey = "StartAtLogin"
    private static let startMinimizedKey = "StartMinimized"
    private static let selectedProviderKey = "SelectedProvider"
    private static let b2BucketsKey = "B2Buckets"
    private static let googleDriveSettingsKey = "GoogleDriveSettings"
    private static let seedboxSettingsKey = "SeedboxSettings"

    static func registerDefaults() {
        UserDefaults.standard.register(defaults: [
            startAtLoginKey: true,
            startMinimizedKey: false,
            selectedProviderKey: CloudProvider.backblazeB2.rawValue
        ])
    }

    static var startAtLogin: Bool {
        get { UserDefaults.standard.bool(forKey: startAtLoginKey) }
        set { UserDefaults.standard.set(newValue, forKey: startAtLoginKey) }
    }

    static var startMinimized: Bool {
        get { UserDefaults.standard.bool(forKey: startMinimizedKey) }
        set { UserDefaults.standard.set(newValue, forKey: startMinimizedKey) }
    }

    static var selectedProvider: CloudProvider {
        get {
            let rawValue = UserDefaults.standard.string(forKey: selectedProviderKey) ?? CloudProvider.backblazeB2.rawValue
            return CloudProvider(rawValue: rawValue) ?? .backblazeB2
        }
        set { UserDefaults.standard.set(newValue.rawValue, forKey: selectedProviderKey) }
    }

    static var b2Buckets: [BucketMount] {
        get {
            guard let data = UserDefaults.standard.data(forKey: b2BucketsKey) else { return [] }
            return (try? JSONDecoder().decode([BucketMount].self, from: data)) ?? []
        }
        set {
            if let data = try? JSONEncoder().encode(newValue) {
                UserDefaults.standard.set(data, forKey: b2BucketsKey)
            }
        }
    }

    static var googleDriveSettings: GoogleDriveSettings {
        get {
            guard let data = UserDefaults.standard.data(forKey: googleDriveSettingsKey),
                  var settings = try? JSONDecoder().decode(GoogleDriveSettings.self, from: data) else {
                return GoogleDriveSettings()
            }

            settings.remoteName = CloudProvider.defaultGoogleDriveRemoteName
            return settings
        }
        set {
            var normalized = newValue
            normalized.remoteName = CloudProvider.defaultGoogleDriveRemoteName
            if let data = try? JSONEncoder().encode(normalized) {
                UserDefaults.standard.set(data, forKey: googleDriveSettingsKey)
            }
        }
    }

    static var seedboxSettings: SeedboxSettings {
        get {
            guard let data = UserDefaults.standard.data(forKey: seedboxSettingsKey),
                  var settings = try? JSONDecoder().decode(SeedboxSettings.self, from: data) else {
                var settings = SeedboxSettings()
                settings.mountPath = defaultSeedboxMountPath()
                return settings
            }

            settings.remoteName = CloudProvider.defaultSeedboxRemoteName
            settings.host = SeedboxSettings.normalizeHost(settings.host)
            settings.remotePath = normalizeRemotePath(settings.remotePath)
            if settings.port <= 0 || settings.port > 65535 {
                settings.port = 21
            }
            if settings.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                settings.mountPath = defaultSeedboxMountPath()
            }
            return settings
        }
        set {
            var normalized = newValue
            normalized.remoteName = CloudProvider.defaultSeedboxRemoteName
            normalized.host = SeedboxSettings.normalizeHost(normalized.host)
            normalized.remotePath = normalizeRemotePath(normalized.remotePath)
            if normalized.port <= 0 || normalized.port > 65535 {
                normalized.port = 21
            }
            if normalized.mountPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                normalized.mountPath = defaultSeedboxMountPath()
            }
            if let data = try? JSONEncoder().encode(normalized) {
                UserDefaults.standard.set(data, forKey: seedboxSettingsKey)
            }
        }
    }

    private static func defaultSeedboxMountPath() -> String {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Drives")
            .appendingPathComponent("Seedbox")
            .path
    }

    private static func normalizeRemotePath(_ path: String) -> String {
        var normalized = path.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\\", with: "/")
        while normalized.hasPrefix("/") || normalized.hasPrefix(":") {
            normalized.removeFirst()
        }
        return normalized
    }
}

struct B2Credentials: Codable {
    var applicationKeyId: String
    var applicationKey: String
}

enum B2CredentialStoreError: LocalizedError {
    case invalidSavedCredentials
    case keychainReadFailed(OSStatus)
    case keychainSaveFailed(OSStatus)

    var errorDescription: String? {
        switch self {
        case .invalidSavedCredentials:
            return "Saved Backblaze B2 credentials could not be read."
        case .keychainReadFailed(let status):
            return "Could not read saved Backblaze B2 credentials from Keychain (status \(status))."
        case .keychainSaveFailed(let status):
            return "Could not save Backblaze B2 credentials to Keychain (status \(status))."
        }
    }
}

enum B2CredentialStore {
    private static let service = "com.bartek.clouddrivemount.b2credentials"
    private static let account = "backblaze-b2"

    static func load() throws -> B2Credentials? {
        var query = keychainQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)

        if status == errSecItemNotFound {
            return nil
        }

        guard status == errSecSuccess else {
            throw B2CredentialStoreError.keychainReadFailed(status)
        }

        guard let data = item as? Data else {
            throw B2CredentialStoreError.invalidSavedCredentials
        }

        do {
            return try JSONDecoder().decode(B2Credentials.self, from: data)
        } catch {
            throw B2CredentialStoreError.invalidSavedCredentials
        }
    }

    static func save(_ credentials: B2Credentials) throws {
        let data = try JSONEncoder().encode(credentials)
        let query = keychainQuery()
        let update = [kSecValueData as String: data]

        let updateStatus = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updateStatus == errSecSuccess {
            return
        }

        guard updateStatus == errSecItemNotFound else {
            throw B2CredentialStoreError.keychainSaveFailed(updateStatus)
        }

        var addQuery = query
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw B2CredentialStoreError.keychainSaveFailed(addStatus)
        }
    }

    private static func keychainQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
    }
}

enum SeedboxCredentialStoreError: LocalizedError {
    case invalidSavedPassword
    case keychainReadFailed(OSStatus)
    case keychainSaveFailed(OSStatus)

    var errorDescription: String? {
        switch self {
        case .invalidSavedPassword:
            return "Saved Seedbox password could not be read."
        case .keychainReadFailed(let status):
            return "Could not read saved Seedbox password from Keychain (status \(status))."
        case .keychainSaveFailed(let status):
            return "Could not save Seedbox password to Keychain (status \(status))."
        }
    }
}

enum SeedboxCredentialStore {
    private static let service = "com.bartek.clouddrivemount.seedboxcredentials"
    private static let account = "seedbox-ftps"

    static func loadPassword() throws -> String? {
        var query = keychainQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne

        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)

        if status == errSecItemNotFound {
            return nil
        }

        guard status == errSecSuccess else {
            throw SeedboxCredentialStoreError.keychainReadFailed(status)
        }

        guard let data = item as? Data,
              let password = String(data: data, encoding: .utf8) else {
            throw SeedboxCredentialStoreError.invalidSavedPassword
        }

        return password
    }

    static func savePassword(_ password: String) throws {
        guard !password.isEmpty else { return }

        let data = Data(password.utf8)
        let query = keychainQuery()
        let update = [kSecValueData as String: data]

        let updateStatus = SecItemUpdate(query as CFDictionary, update as CFDictionary)
        if updateStatus == errSecSuccess {
            return
        }

        guard updateStatus == errSecItemNotFound else {
            throw SeedboxCredentialStoreError.keychainSaveFailed(updateStatus)
        }

        var addQuery = query
        addQuery[kSecValueData as String] = data
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly

        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        guard addStatus == errSecSuccess else {
            throw SeedboxCredentialStoreError.keychainSaveFailed(addStatus)
        }
    }

    static func deletePassword() throws {
        let status = SecItemDelete(keychainQuery() as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw SeedboxCredentialStoreError.keychainSaveFailed(status)
        }
    }

    private static func keychainQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
    }
}
