import Foundation

final class ProviderItemCache: @unchecked Sendable {
    static let shared = ProviderItemCache()

    private let fileURL: URL
    private let lock = NSLock()

    init(fileURL: URL = ProviderItemCache.defaultCacheURL()) {
        self.fileURL = fileURL
    }

    func store(_ key: ProviderItemKey, identifier: String) {
        lock.withLock {
            var cache = loadUnlocked()
            cache[identifier] = key
            saveUnlocked(cache)
        }
    }

    func key(for identifier: String) -> ProviderItemKey? {
        lock.withLock {
            loadUnlocked()[identifier]
        }
    }

    static func defaultCacheURL() -> URL {
        let baseURL = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppConstants.appGroupIdentifier)
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return baseURL
            .appendingPathComponent("FileProvider", isDirectory: true)
            .appendingPathComponent(AppConstants.providerItemCacheFileName)
    }

    private func loadUnlocked() -> [String: ProviderItemKey] {
        guard FileManager.default.fileExists(atPath: fileURL.path),
              let data = try? Data(contentsOf: fileURL),
              let cache = try? Self.decoder.decode([String: ProviderItemKey].self, from: data) else {
            return [:]
        }
        return cache
    }

    private func saveUnlocked(_ cache: [String: ProviderItemKey]) {
        do {
            try FileManager.default.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            let data = try Self.encoder.encode(cache)
            try data.write(to: fileURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        } catch {
            Diagnostics.shared.error("providerItemCache.save.failed", area: "fileprovider", error: error)
        }
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}

private extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}
