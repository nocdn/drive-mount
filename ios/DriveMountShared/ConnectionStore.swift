import Foundation

struct ConnectionStore: Sendable {
    var fileURL: URL

    init(fileURL: URL? = nil) {
        self.fileURL = fileURL ?? Self.defaultStoreURL()
        Self.migrateLegacyStoreIfNeeded(to: self.fileURL)
    }

    func load() throws -> [CloudConnection] {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return []
        }
        let data = try Data(contentsOf: fileURL)
        return try Self.decoder.decode([CloudConnection].self, from: data)
    }

    func save(_ connections: [CloudConnection]) throws {
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try Self.encoder.encode(connections)
        try data.write(to: fileURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    static func defaultStoreURL() -> URL {
        sharedContainerURL()
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("Connections", isDirectory: true)
            .appendingPathComponent(AppConstants.connectionStoreFileName)
    }

    /// Older builds stored connections at App Group root; migrate once into Library/.
    private static func migrateLegacyStoreIfNeeded(to destination: URL) {
        let legacy = sharedContainerURL()
            .appendingPathComponent("Connections", isDirectory: true)
            .appendingPathComponent(AppConstants.connectionStoreFileName)
        guard legacy.path != destination.path,
              FileManager.default.fileExists(atPath: legacy.path),
              !FileManager.default.fileExists(atPath: destination.path) else {
            return
        }
        do {
            try FileManager.default.createDirectory(
                at: destination.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try FileManager.default.copyItem(at: legacy, to: destination)
        } catch {
            Diagnostics.shared.error("connections.migrate.failed", area: "settings", error: error)
        }
    }

    static func sharedContainerURL() -> URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: AppConstants.appGroupIdentifier)
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
