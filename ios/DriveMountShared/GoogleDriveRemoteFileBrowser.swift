import Foundation

final class GoogleDriveRemoteFileBrowser: RemoteFileBrowsing, @unchecked Sendable {
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
        let parentID: String
        let parentRemoteID: String
        let parentItemID: String
        if identifier == RemoteFileItem.rootID {
            parentID = connection.googleDrive.rootFolderID.isEmpty ? "root" : connection.googleDrive.rootFolderID
            parentRemoteID = RemoteFileItem.rootID
            parentItemID = RemoteFileItem.rootID
        } else if let key = ProviderItemKey.decode(identifier) {
            parentID = key.remoteID
            parentRemoteID = key.remoteID
            parentItemID = identifier
        } else {
            return []
        }

        var components = URLComponents(string: "https://www.googleapis.com/drive/v3/files")!
        components.queryItems = [
            URLQueryItem(name: "q", value: "'\(parentID)' in parents and trashed = false"),
            URLQueryItem(name: "fields", value: "files(id,name,mimeType,size,modifiedTime)"),
            URLQueryItem(name: "pageSize", value: "1000"),
            URLQueryItem(name: "supportsAllDrives", value: "true"),
            URLQueryItem(name: "includeItemsFromAllDrives", value: "true")
        ]
        let (data, _) = try await client.data(for: .authenticatedGet(url: components.url!, bearerToken: connection.googleDrive.accessToken))
        let response = try JSONDecoder.googleDriveDecoder.decode(GoogleDriveFilesResponse.self, from: data)

        return response.files.map { file in
            let isFolder = file.mimeType == "application/vnd.google-apps.folder"
            let key = ProviderItemKey(
                provider: .googleDrive,
                kind: isFolder ? .folder : .file,
                name: file.name,
                remoteID: file.id,
                parentRemoteID: parentRemoteID,
                parentItemID: parentItemID,
                size: file.size.flatMap(Int64.init),
                modifiedAt: file.modifiedTime,
                contentType: file.mimeType,
                extra: ["mimeType": file.mimeType]
            )
            return RemoteFileItem(
                key: key,
                parentID: parentItemID,
                filename: file.name,
                isDirectory: isFolder,
                size: key.size,
                modifiedAt: file.modifiedTime,
                contentType: file.mimeType
            )
        }
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier), key.kind == .file else {
            throw RemoteFileError.notFound(identifier)
        }
        let mimeType = key.extra["mimeType"] ?? ""
        let url: URL
        if mimeType.hasPrefix("application/vnd.google-apps.") {
            var components = URLComponents(string: "https://www.googleapis.com/drive/v3/files/\(key.remoteID)/export")!
            components.queryItems = [URLQueryItem(name: "mimeType", value: "application/pdf")]
            url = components.url!
        } else {
            var components = URLComponents(string: "https://www.googleapis.com/drive/v3/files/\(key.remoteID)")!
            components.queryItems = [URLQueryItem(name: "alt", value: "media")]
            url = components.url!
        }
        return try await client.download(for: .authenticatedGet(url: url, bearerToken: connection.googleDrive.accessToken))
    }
}

private struct GoogleDriveFilesResponse: Decodable {
    var files: [GoogleDriveFile]
}

private struct GoogleDriveFile: Decodable {
    var id: String
    var name: String
    var mimeType: String
    var size: String?
    var modifiedTime: Date?
}

private extension JSONDecoder {
    static let googleDriveDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
