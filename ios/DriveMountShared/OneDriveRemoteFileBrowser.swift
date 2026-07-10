import Foundation

final class OneDriveRemoteFileBrowser: RemoteFileBrowsing, @unchecked Sendable {
    private let connection: CloudConnection
    private let client = HTTPClient()

    init(connection: CloudConnection) {
        self.connection = connection
    }

    func item(for identifier: String) async throws -> RemoteFileItem {
        if identifier == RemoteFileItem.rootID {
            return .root(displayName: connection.effectiveDisplayName)
        }
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        return RemoteFileItem(
            key: key,
            parentID: key.parentItemID ?? key.parentRemoteID,
            filename: key.name,
            isDirectory: key.kind == .folder,
            size: key.size,
            modifiedAt: key.modifiedAt,
            contentType: key.contentType
        )
    }

    func children(of identifier: String) async throws -> [RemoteFileItem] {
        let url: URL
        let parentRemoteID: String
        let parentItemID: String
        if identifier == RemoteFileItem.rootID {
            parentRemoteID = RemoteFileItem.rootID
            parentItemID = RemoteFileItem.rootID
            if connection.oneDrive.rootItemID.isEmpty {
                url = URL(string: "https://graph.microsoft.com/v1.0/me/drive/root/children")!
            } else {
                url = URL(string: "https://graph.microsoft.com/v1.0/me/drive/items/\(connection.oneDrive.rootItemID)/children")!
            }
        } else if let key = ProviderItemKey.decode(identifier) {
            parentRemoteID = key.remoteID
            parentItemID = identifier
            url = URL(string: "https://graph.microsoft.com/v1.0/me/drive/items/\(key.remoteID)/children")!
        } else {
            return []
        }

        let (data, _) = try await client.data(for: .authenticatedGet(url: url, bearerToken: connection.oneDrive.accessToken))
        let response = try JSONDecoder.oneDriveDecoder.decode(OneDriveChildrenResponse.self, from: data)

        return response.value.map { item in
            let isFolder = item.folder != nil
            let key = ProviderItemKey(
                provider: .oneDrive,
                kind: isFolder ? .folder : .file,
                name: item.name,
                remoteID: item.id,
                parentRemoteID: parentRemoteID,
                parentItemID: parentItemID,
                size: item.size,
                modifiedAt: item.lastModifiedDateTime,
                contentType: item.file?.mimeType,
                extra: [:]
            )
            return RemoteFileItem(
                key: key,
                parentID: parentItemID,
                filename: item.name,
                isDirectory: isFolder,
                size: item.size,
                modifiedAt: item.lastModifiedDateTime,
                contentType: item.file?.mimeType
            )
        }
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier), key.kind == .file else {
            throw RemoteFileError.notFound(identifier)
        }
        let url = URL(string: "https://graph.microsoft.com/v1.0/me/drive/items/\(key.remoteID)/content")!
        return try await client.download(for: .authenticatedGet(url: url, bearerToken: connection.oneDrive.accessToken))
    }
}

private struct OneDriveChildrenResponse: Decodable {
    var value: [OneDriveItem]
}

private struct OneDriveItem: Decodable {
    var id: String
    var name: String
    var size: Int64?
    var lastModifiedDateTime: Date?
    var folder: OneDriveFolder?
    var file: OneDriveFile?
}

private struct OneDriveFolder: Decodable {}

private struct OneDriveFile: Decodable {
    var mimeType: String?
}

private extension JSONDecoder {
    static let oneDriveDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
