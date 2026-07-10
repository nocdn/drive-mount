import Foundation

struct FixtureRemoteFileBrowser: RemoteFileBrowsing {
    var connection: CloudConnection
    var reason: RemoteFileError?

    func item(for identifier: String) async throws -> RemoteFileItem {
        if identifier == RemoteFileItem.rootID {
            return .root(displayName: connection.effectiveDisplayName)
        }
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        return RemoteFileItem(
            key: key,
            parentID: key.parentRemoteID,
            filename: key.name,
            isDirectory: key.kind == .folder,
            size: key.size,
            modifiedAt: key.modifiedAt,
            contentType: key.contentType
        )
    }

    func children(of identifier: String) async throws -> [RemoteFileItem] {
        guard identifier == RemoteFileItem.rootID else {
            return []
        }

        var items: [RemoteFileItem] = []
        if let reason {
            items.append(makeFile(name: "Connection Status.txt", text: reason.localizedDescription))
        }
        items.append(makeFile(name: "Drive Mount.txt", text: "This File Provider domain is registered by Drive Mount. Add valid connection credentials in the app to browse remote files."))
        return items
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        let text = key.extra["text"] ?? "Drive Mount"
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("txt")
        try Data(text.utf8).write(to: url, options: [.atomic])
        return url
    }

    private func makeFile(name: String, text: String) -> RemoteFileItem {
        let key = ProviderItemKey(
            provider: connection.provider,
            kind: .file,
            name: name,
            remoteID: "fixture:\(name)",
            parentRemoteID: RemoteFileItem.rootID,
            size: Int64(text.utf8.count),
            modifiedAt: Date(timeIntervalSince1970: 0),
            contentType: "text/plain",
            extra: ["text": text]
        )
        return RemoteFileItem(
            key: key,
            parentID: RemoteFileItem.rootID,
            filename: name,
            isDirectory: false,
            size: Int64(text.utf8.count),
            modifiedAt: key.modifiedAt,
            contentType: key.contentType
        )
    }
}
