import Foundation

final class B2GroupedRemoteFileBrowser: RemoteFileBrowsing, @unchecked Sendable {
    private let browsersByConnectionID: [String: any RemoteFileBrowsing]

    init(connections: [CloudConnection]) {
        var browsers: [String: any RemoteFileBrowsing] = [:]
        for connection in connections {
            browsers[connection.id] = B2RemoteFileBrowser(connection: connection)
        }
        browsersByConnectionID = browsers
    }

    init(browsersByConnectionID: [String: any RemoteFileBrowsing]) {
        self.browsersByConnectionID = browsersByConnectionID
    }

    func item(for identifier: String) async throws -> RemoteFileItem {
        if identifier == RemoteFileItem.rootID {
            return .root(displayName: AppConstants.b2FileProviderDomainDisplayName)
        }
        return try await browser(for: identifier).item(for: identifier)
    }

    func children(of identifier: String) async throws -> [RemoteFileItem] {
        guard identifier == RemoteFileItem.rootID else {
            return try await browser(for: identifier).children(of: identifier)
        }

        var bucketsByName: [String: RemoteFileItem] = [:]
        var firstError: Error?
        for entry in browsersByConnectionID.sorted(by: { $0.key < $1.key }) {
            do {
                for bucket in try await entry.value.children(of: RemoteFileItem.rootID) where bucket.isDirectory {
                    if bucketsByName[bucket.filename] == nil {
                        bucketsByName[bucket.filename] = bucket
                    }
                }
            } catch {
                firstError = firstError ?? error
                Diagnostics.shared.error(
                    "b2.buckets.list.failed",
                    area: "b2",
                    error: error,
                    fields: ["connection": entry.key]
                )
            }
        }

        if bucketsByName.isEmpty, let firstError {
            throw firstError
        }
        return bucketsByName.values.sorted {
            $0.filename.localizedStandardCompare($1.filename) == .orderedAscending
        }
    }

    func contents(of identifier: String) async throws -> URL {
        try await browser(for: identifier).contents(of: identifier)
    }

    private func browser(for identifier: String) throws -> any RemoteFileBrowsing {
        if let connectionID = ProviderItemKey.decode(identifier)?.extra["connectionID"],
           let browser = browsersByConnectionID[connectionID] {
            return browser
        }
        if browsersByConnectionID.count == 1, let browser = browsersByConnectionID.values.first {
            return browser
        }
        throw RemoteFileError.notFound(identifier)
    }
}

final class B2RemoteFileBrowser: RemoteFileBrowsing, @unchecked Sendable {
    private let connection: CloudConnection
    private let client = HTTPClient()
    private var authorization: B2Authorization?

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
        let auth = try await authorize()
        if identifier == RemoteFileItem.rootID {
            let buckets = try await listBuckets(auth: auth)
            let filtered = connection.b2.bucketName.trimmed
            return buckets
                .filter { filtered.isEmpty || $0.bucketName == filtered }
                .map { bucket in
                    makeItem(
                        kind: .folder,
                        name: bucket.bucketName,
                        remoteID: "bucket:\(bucket.bucketID)",
                        parentRemoteID: RemoteFileItem.rootID,
                        parentItemID: RemoteFileItem.rootID,
                        extra: ["bucketID": bucket.bucketID, "bucketName": bucket.bucketName, "prefix": ""]
                    )
                }
        }

        guard let key = ProviderItemKey.decode(identifier),
              let bucketID = key.extra["bucketID"],
              let bucketName = key.extra["bucketName"] else {
            return []
        }
        let prefix = key.extra["prefix"] ?? ""
        return try await listFileNames(auth: auth, bucketID: bucketID, bucketName: bucketName, prefix: prefix, parentItemID: identifier)
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier),
              key.kind == .file,
              let fileID = key.extra["fileID"] else {
            throw RemoteFileError.notFound(identifier)
        }
        let auth = try await authorize()
        var components = URLComponents(string: auth.downloadURL + "/b2api/v4/b2_download_file_by_id")
        components?.queryItems = [URLQueryItem(name: "fileId", value: fileID)]
        guard let url = components?.url else {
            throw RemoteFileError.invalidResponse("Invalid B2 download URL.")
        }
        var request = URLRequest(url: url)
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        return try await client.download(
            for: request,
            suggestedFilename: key.name,
            expectedSize: key.size
        )
    }

    private func authorize() async throws -> B2Authorization {
        if let authorization {
            return authorization
        }
        let keyID = connection.b2.applicationKeyID.trimmed
        let key = connection.b2.applicationKey.trimmed
        guard !keyID.isEmpty, !key.isEmpty else {
            throw RemoteFileError.missingCredentials(connection.provider.displayName)
        }

        var request = URLRequest(url: URL(string: "https://api.backblazeb2.com/b2api/v4/b2_authorize_account")!)
        let credential = Data("\(keyID):\(key)".utf8).base64EncodedString()
        request.setValue("Basic \(credential)", forHTTPHeaderField: "Authorization")

        let (data, _) = try await client.data(for: request)
        let decoded = try JSONDecoder().decode(B2Authorization.self, from: data)
        authorization = decoded
        return decoded
    }

    private func listBuckets(auth: B2Authorization) async throws -> [B2Bucket] {
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_list_buckets")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2ListBucketsRequest(accountID: auth.accountID, bucketName: connection.b2.bucketName.nilIfEmpty)
        )
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        let (data, _) = try await client.data(for: request)
        return try JSONDecoder().decode(B2ListBucketsResponse.self, from: data).buckets
    }

    private func listFileNames(auth: B2Authorization, bucketID: String, bucketName: String, prefix: String, parentItemID: String) async throws -> [RemoteFileItem] {
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_list_file_names")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2ListFileNamesRequest(bucketID: bucketID, prefix: prefix, delimiter: "/", maxFileCount: 1000)
        )
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        let (data, _) = try await client.data(for: request)
        let response = try JSONDecoder().decode(B2ListFileNamesResponse.self, from: data)

        return response.files.compactMap { file in
            let name = displayName(forB2FileName: file.fileName, prefix: prefix)
            guard !name.isEmpty else {
                return nil
            }

            if file.action == "folder" || file.fileName.hasSuffix("/") {
                return makeItem(
                    kind: .folder,
                    name: name,
                    remoteID: "folder:\(bucketID):\(file.fileName)",
                    parentRemoteID: prefix.isEmpty ? "bucket:\(bucketID)" : "folder:\(bucketID):\(prefix)",
                    parentItemID: parentItemID,
                    extra: ["bucketID": bucketID, "bucketName": bucketName, "prefix": file.fileName]
                )
            }

            return makeItem(
                kind: .file,
                name: name,
                remoteID: "file:\(file.fileID ?? file.fileName)",
                parentRemoteID: prefix.isEmpty ? "bucket:\(bucketID)" : "folder:\(bucketID):\(prefix)",
                parentItemID: parentItemID,
                size: file.contentLength,
                modifiedAt: file.uploadTimestamp.map { Date(timeIntervalSince1970: TimeInterval($0) / 1000) },
                contentType: file.contentType,
                extra: ["bucketID": bucketID, "bucketName": bucketName, "fileID": file.fileID ?? "", "fileName": file.fileName]
            )
        }
    }

    private func displayName(forB2FileName fileName: String, prefix: String) -> String {
        let suffix = fileName.hasPrefix(prefix) ? String(fileName.dropFirst(prefix.count)) : fileName
        return suffix.split(separator: "/").first.map(String.init) ?? suffix
    }

    private func makeItem(
        kind: ProviderItemKey.Kind,
        name: String,
        remoteID: String,
        parentRemoteID: String,
        parentItemID: String,
        size: Int64? = nil,
        modifiedAt: Date? = nil,
        contentType: String? = nil,
        extra: [String: String]
    ) -> RemoteFileItem {
        var routedExtra = extra
        routedExtra["connectionID"] = connection.id
        let key = ProviderItemKey(
            provider: .backblazeB2,
            kind: kind,
            name: name,
            remoteID: remoteID,
            parentRemoteID: parentRemoteID,
            parentItemID: parentItemID,
            size: size,
            modifiedAt: modifiedAt,
            contentType: contentType,
            extra: routedExtra
        )
        return RemoteFileItem(
            key: key,
            parentID: parentItemID,
            filename: name,
            isDirectory: kind == .folder,
            size: size,
            modifiedAt: modifiedAt,
            contentType: contentType
        )
    }
}

private struct B2Authorization: Decodable {
    var accountID: String
    var authorizationToken: String
    var apiURL: String
    var downloadURL: String

    enum CodingKeys: String, CodingKey {
        case accountID = "accountId"
        case authorizationToken
        case apiInfo
        case apiURL = "apiUrl"
        case downloadURL = "downloadUrl"
    }

    enum APIInfoKeys: String, CodingKey {
        case storageApi
    }

    enum StorageAPIKeys: String, CodingKey {
        case apiURL = "apiUrl"
        case downloadURL = "downloadUrl"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        accountID = try container.decode(String.self, forKey: .accountID)
        authorizationToken = try container.decode(String.self, forKey: .authorizationToken)

        if let topLevelAPIURL = try container.decodeIfPresent(String.self, forKey: .apiURL),
           let topLevelDownloadURL = try container.decodeIfPresent(String.self, forKey: .downloadURL) {
            apiURL = topLevelAPIURL
            downloadURL = topLevelDownloadURL
        } else {
            let apiInfo = try container.nestedContainer(keyedBy: APIInfoKeys.self, forKey: .apiInfo)
            let storageApi = try apiInfo.nestedContainer(keyedBy: StorageAPIKeys.self, forKey: .storageApi)
            apiURL = try storageApi.decode(String.self, forKey: .apiURL)
            downloadURL = try storageApi.decode(String.self, forKey: .downloadURL)
        }
    }
}

private struct B2ListBucketsRequest: Encodable {
    var accountID: String
    var bucketName: String?

    enum CodingKeys: String, CodingKey {
        case accountID = "accountId"
        case bucketName
    }
}

private struct B2ListBucketsResponse: Decodable {
    var buckets: [B2Bucket]
}

private struct B2Bucket: Decodable {
    var bucketID: String
    var bucketName: String

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
        case bucketName
    }
}

private extension String {
    var nilIfEmpty: String? {
        let value = trimmed
        return value.isEmpty ? nil : value
    }
}

private struct B2ListFileNamesRequest: Encodable {
    var bucketID: String
    var prefix: String
    var delimiter: String
    var maxFileCount: Int

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
        case prefix
        case delimiter
        case maxFileCount
    }
}

private struct B2ListFileNamesResponse: Decodable {
    var files: [B2File]
}

private struct B2File: Decodable {
    var fileID: String?
    var fileName: String
    var action: String?
    var contentLength: Int64?
    var contentType: String?
    var uploadTimestamp: Int64?

    enum CodingKeys: String, CodingKey {
        case fileID = "fileId"
        case fileName
        case action
        case contentLength
        case contentType
        case uploadTimestamp
    }
}
