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

    func createItem(
        name: String,
        parentIdentifier: String,
        isDirectory: Bool,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        try await browser(forParent: parentIdentifier).createItem(
            name: name,
            parentIdentifier: parentIdentifier,
            isDirectory: isDirectory,
            contentsURL: contentsURL,
            contentType: contentType
        )
    }

    func modifyItem(
        identifier: String,
        newName: String?,
        newParentIdentifier: String?,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        try await browser(for: identifier).modifyItem(
            identifier: identifier,
            newName: newName,
            newParentIdentifier: newParentIdentifier,
            contentsURL: contentsURL,
            contentType: contentType
        )
    }

    func deleteItem(identifier: String) async throws {
        try await browser(for: identifier).deleteItem(identifier: identifier)
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

    private func browser(forParent parentIdentifier: String) throws -> any RemoteFileBrowsing {
        if parentIdentifier == RemoteFileItem.rootID {
            throw RemoteFileError.unsupported("Create items inside a bucket, not at the Backblaze B2 root.")
        }
        return try browser(for: parentIdentifier)
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
            return .root(displayName: connection.effectiveDisplayName, allowsAddingSubItems: true)
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
            let bucket = try await rootBucket(auth: auth)
            return try await listFileNames(
                auth: auth,
                bucketID: bucket.bucketID,
                bucketName: bucket.bucketName,
                prefix: "",
                delimiter: "/",
                parentItemID: RemoteFileItem.rootID
            )
        }

        guard let key = ProviderItemKey.decode(identifier),
              let bucketID = key.extra["bucketID"],
              let bucketName = key.extra["bucketName"] else {
            return []
        }
        let prefix = key.extra["prefix"] ?? ""
        return try await listFileNames(
            auth: auth,
            bucketID: bucketID,
            bucketName: bucketName,
            prefix: prefix,
            delimiter: "/",
            parentItemID: identifier
        )
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier),
              key.kind == .file,
              let fileID = key.extra["fileID"],
              !fileID.isEmpty else {
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

    func createItem(
        name: String,
        parentIdentifier: String,
        isDirectory: Bool,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        let parent = try await parentLocation(for: parentIdentifier)
        let sanitizedName = try Self.sanitizedFileName(name)
        if isDirectory {
            let folderName = parent.prefix + sanitizedName + "/"
            let uploaded = try await upload(
                bucketID: parent.bucketID,
                bucketName: parent.bucketName,
                fileName: folderName,
                contentsURL: nil,
                emptyBody: true,
                contentType: "application/x-directory",
                parentItemID: parentIdentifier
            )
            // Represent as a folder even though B2 stores a trailing-slash marker.
            return makeItem(
                kind: .folder,
                name: sanitizedName,
                remoteID: "folder:\(parent.bucketID):\(folderName)",
                parentRemoteID: parent.remoteID,
                parentItemID: parentIdentifier,
                size: 0,
                modifiedAt: uploaded.modifiedAt,
                contentType: nil,
                extra: [
                    "bucketID": parent.bucketID,
                    "bucketName": parent.bucketName,
                    "prefix": folderName
                ]
            )
        }

        let remoteName = parent.prefix + sanitizedName
        return try await upload(
            bucketID: parent.bucketID,
            bucketName: parent.bucketName,
            fileName: remoteName,
            contentsURL: contentsURL,
            emptyBody: contentsURL == nil,
            contentType: contentType ?? "b2/x-auto",
            parentItemID: parentIdentifier
        )
    }

    func modifyItem(
        identifier: String,
        newName: String?,
        newParentIdentifier: String?,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }

        let wantsRename = newName.map { $0 != key.name } ?? false
        let wantsMove = newParentIdentifier.map { $0 != (key.parentItemID ?? key.parentRemoteID) } ?? false
        let wantsContent = contentsURL != nil

        // Files often probes metadata updates on the bucket folder. Never fail those —
        // throwing here leaves the domain stuck with the circular error badge.
        if key.remoteID.hasPrefix("bucket:") {
            if wantsRename || wantsMove || wantsContent {
                Diagnostics.shared.info(
                    "b2.modify.bucket.ignored",
                    area: "b2",
                    fields: [
                        "bucket": key.extra["bucketName"] ?? key.name,
                        "rename": wantsRename ? "1" : "0",
                        "move": wantsMove ? "1" : "0",
                        "content": wantsContent ? "1" : "0"
                    ]
                )
            }
            return try await item(for: identifier)
        }

        if key.kind == .folder {
            if wantsContent {
                throw RemoteFileError.unsupported("Folders do not have file contents.")
            }
            if !wantsRename && !wantsMove {
                return try await item(for: identifier)
            }
            Diagnostics.shared.info(
                "b2.folder.rename.started",
                area: "b2",
                fields: [
                    "from": key.extra["prefix"] ?? key.name,
                    "toName": newName ?? key.name,
                    "move": wantsMove ? "1" : "0"
                ]
            )
            let result = try await renameOrMoveFolder(
                key: key,
                identifier: identifier,
                newName: wantsRename ? newName : nil,
                newParentIdentifier: wantsMove ? newParentIdentifier : nil
            )
            Diagnostics.shared.info(
                "b2.folder.rename.finished",
                area: "b2",
                fields: ["to": result.key?.extra["prefix"] ?? result.filename]
            )
            return result
        }

        // Content replace at the same object key: upload new version, then hard-delete older versions.
        if wantsContent && !wantsRename && !wantsMove {
            let fileName = key.extra["fileName"] ?? key.name
            guard let bucketID = key.extra["bucketID"], let bucketName = key.extra["bucketName"] else {
                throw RemoteFileError.notFound(identifier)
            }
            let parentItemID = key.parentItemID ?? key.parentRemoteID
            Diagnostics.shared.info(
                "b2.file.replace.started",
                area: "b2",
                fields: ["fileName": fileName]
            )
            let uploaded = try await upload(
                bucketID: bucketID,
                bucketName: bucketName,
                fileName: fileName,
                contentsURL: contentsURL,
                emptyBody: false,
                contentType: contentType ?? key.contentType ?? "b2/x-auto",
                parentItemID: parentItemID
            )
            let keepID = uploaded.key?.extra["fileID"]
            try await permanentlyDeleteAllVersions(
                bucketID: bucketID,
                fileName: fileName,
                keepingFileID: keepID
            )
            Diagnostics.shared.info(
                "b2.file.replace.finished",
                area: "b2",
                fields: ["fileName": fileName, "fileID": keepID ?? ""]
            )
            return uploaded
        }

        // Rename/move: server-side copy + hard-delete source (never download+reupload).
        if wantsRename || wantsMove {
            Diagnostics.shared.info(
                "b2.file.rename.started",
                area: "b2",
                fields: [
                    "from": key.extra["fileName"] ?? key.name,
                    "toName": newName ?? key.name,
                    "move": wantsMove ? "1" : "0",
                    "withContent": wantsContent ? "1" : "0"
                ]
            )
            let result = try await renameOrMoveFile(
                key: key,
                identifier: identifier,
                newName: wantsRename ? newName : nil,
                newParentIdentifier: wantsMove ? newParentIdentifier : nil,
                contentsURL: contentsURL,
                contentType: contentType
            )
            Diagnostics.shared.info(
                "b2.file.rename.finished",
                area: "b2",
                fields: [
                    "to": result.key?.extra["fileName"] ?? result.filename,
                    "fileID": result.key?.extra["fileID"] ?? ""
                ]
            )
            return result
        }

        return try await item(for: identifier)
    }

    func deleteItem(identifier: String) async throws {
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        if key.remoteID.hasPrefix("bucket:") {
            // Same as modify: Files may probe deletes; don't poison the domain.
            Diagnostics.shared.info(
                "b2.delete.bucket.ignored",
                area: "b2",
                fields: ["bucket": key.extra["bucketName"] ?? key.name]
            )
            throw RemoteFileError.unsupported("Buckets cannot be deleted from Files.")
        }

        if key.kind == .folder {
            guard let bucketID = key.extra["bucketID"],
                  let prefix = key.extra["prefix"] else {
                throw RemoteFileError.notFound(identifier)
            }
            Diagnostics.shared.info(
                "b2.folder.delete.started",
                area: "b2",
                fields: ["prefix": prefix, "bucketID": bucketID]
            )
            let deleted = try await permanentlyDeletePrefix(bucketID: bucketID, prefix: prefix)
            Diagnostics.shared.info(
                "b2.folder.delete.finished",
                area: "b2",
                fields: ["prefix": prefix, "versionsDeleted": "\(deleted)"]
            )
            return
        }

        guard let bucketID = key.extra["bucketID"] else {
            throw RemoteFileError.notFound(identifier)
        }
        let fileName = key.extra["fileName"] ?? key.name
        let knownID = key.extra["fileID"]
        Diagnostics.shared.info(
            "b2.file.delete.started",
            area: "b2",
            fields: ["fileName": fileName, "fileID": knownID ?? ""]
        )
        let deleted = try await permanentlyDeleteAllVersions(
            bucketID: bucketID,
            fileName: fileName,
            knownFileID: knownID
        )
        Diagnostics.shared.info(
            "b2.file.delete.finished",
            area: "b2",
            fields: [
                "fileName": fileName,
                "versionsDeleted": "\(deleted)",
                "mode": "b2_delete_file_version"
            ]
        )
    }

    private func parentLocation(for parentIdentifier: String) async throws -> B2ParentLocation {
        if parentIdentifier == RemoteFileItem.rootID {
            let auth = try await authorize()
            let bucket = try await rootBucket(auth: auth)
            return B2ParentLocation(
                bucketID: bucket.bucketID,
                bucketName: bucket.bucketName,
                prefix: "",
                remoteID: "bucket:\(bucket.bucketID)"
            )
        }
        guard let key = ProviderItemKey.decode(parentIdentifier),
              let bucketID = key.extra["bucketID"],
              let bucketName = key.extra["bucketName"] else {
            throw RemoteFileError.notFound(parentIdentifier)
        }
        if key.remoteID.hasPrefix("bucket:") {
            return B2ParentLocation(
                bucketID: bucketID,
                bucketName: bucketName,
                prefix: "",
                remoteID: key.remoteID
            )
        }
        let prefix = key.extra["prefix"] ?? ""
        let normalizedPrefix = prefix.isEmpty || prefix.hasSuffix("/") ? prefix : prefix + "/"
        return B2ParentLocation(
            bucketID: bucketID,
            bucketName: bucketName,
            prefix: normalizedPrefix,
            remoteID: key.remoteID
        )
    }

    private func rootBucket(auth: B2Authorization) async throws -> B2Bucket {
        let buckets = try await listBuckets(auth: auth)
        let configuredName = connection.b2.bucketName.nilIfEmpty
            ?? connection.displayName.nilIfEmpty
        if let configuredName,
           let bucket = buckets.first(where: { $0.bucketName == configuredName }) {
            return bucket
        }
        guard buckets.count == 1, let bucket = buckets.first else {
            throw RemoteFileError.invalidResponse(
                "Choose one Backblaze B2 bucket for this Files location."
            )
        }
        return bucket
    }

    private func upload(
        bucketID: String,
        bucketName: String,
        fileName: String,
        contentsURL: URL?,
        emptyBody: Bool,
        contentType: String,
        parentItemID: String
    ) async throws -> RemoteFileItem {
        let auth = try await authorize()
        let uploadTarget = try await getUploadURL(auth: auth, bucketID: bucketID)

        var request = URLRequest(url: uploadTarget.uploadURL)
        request.httpMethod = "POST"
        request.setValue(uploadTarget.authorizationToken, forHTTPHeaderField: "Authorization")
        request.setValue(fileName.b2PercentEncodedFileName, forHTTPHeaderField: "X-Bz-File-Name")
        request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        request.setValue("do_not_verify", forHTTPHeaderField: "X-Bz-Content-Sha1")

        let data: Data
        if emptyBody || contentsURL == nil {
            data = Data()
            request.setValue("0", forHTTPHeaderField: "Content-Length")
            let (responseData, _) = try await client.upload(for: request, from: data)
            return try makeUploadedItem(
                from: responseData,
                bucketID: bucketID,
                bucketName: bucketName,
                parentItemID: parentItemID,
                fallbackName: (fileName as NSString).lastPathComponent.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            )
        }

        guard let contentsURL else {
            throw RemoteFileError.invalidResponse("Missing file contents for upload.")
        }
        let attributes = try FileManager.default.attributesOfItem(atPath: contentsURL.path)
        let size = (attributes[.size] as? NSNumber)?.int64Value ?? 0
        request.setValue("\(size)", forHTTPHeaderField: "Content-Length")
        let (responseData, _) = try await client.upload(for: request, fromFile: contentsURL)
        return try makeUploadedItem(
            from: responseData,
            bucketID: bucketID,
            bucketName: bucketName,
            parentItemID: parentItemID,
            fallbackName: (fileName as NSString).lastPathComponent
        )
    }

    private func makeUploadedItem(
        from data: Data,
        bucketID: String,
        bucketName: String,
        parentItemID: String,
        fallbackName: String
    ) throws -> RemoteFileItem {
        let uploaded = try JSONDecoder().decode(B2UploadedFile.self, from: data)
        let name = displayName(forB2FileName: uploaded.fileName, prefix: parentPrefix(from: uploaded.fileName))
        let display = name.isEmpty ? fallbackName : name
        let parentRemoteID: String
        if let slash = uploaded.fileName.lastIndex(of: "/") {
            let parentPath = String(uploaded.fileName[..<uploaded.fileName.index(after: slash)])
            parentRemoteID = "folder:\(bucketID):\(parentPath)"
        } else {
            parentRemoteID = "bucket:\(bucketID)"
        }
        return makeItem(
            kind: .file,
            name: display,
            remoteID: "file:\(uploaded.fileID)",
            parentRemoteID: parentRemoteID,
            parentItemID: parentItemID,
            size: uploaded.contentLength,
            modifiedAt: uploaded.uploadTimestamp.map { Date(timeIntervalSince1970: TimeInterval($0) / 1000) },
            contentType: uploaded.contentType,
            extra: [
                "bucketID": bucketID,
                "bucketName": bucketName,
                "fileID": uploaded.fileID,
                "fileName": uploaded.fileName
            ]
        )
    }

    private func parentPrefix(from fileName: String) -> String {
        guard let slash = fileName.lastIndex(of: "/") else {
            return ""
        }
        return String(fileName[..<fileName.index(after: slash)])
    }

    private func getUploadURL(auth: B2Authorization, bucketID: String) async throws -> B2UploadURL {
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_get_upload_url")!
        var request = try URLRequest.jsonPost(url: url, body: B2GetUploadURLRequest(bucketID: bucketID))
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        let (data, _) = try await client.data(for: request)
        return try JSONDecoder().decode(B2UploadURL.self, from: data)
    }

    /// Hard-deletes one object version via `b2_delete_file_version` (not `b2_hide_file`).
    private func deleteFileVersion(fileName: String, fileID: String) async throws {
        let auth = try await authorize()
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_delete_file_version")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2DeleteFileVersionRequest(fileName: fileName, fileID: fileID)
        )
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        _ = try await client.data(for: request)
        Diagnostics.shared.info(
            "b2.api.delete_file_version",
            area: "b2",
            fields: ["fileName": fileName, "fileID": fileID]
        )
    }

    /// Deletes every stored version of an object name (true hard delete; no hide marker).
    @discardableResult
    private func permanentlyDeleteAllVersions(
        bucketID: String,
        fileName: String,
        knownFileID: String? = nil,
        keepingFileID: String? = nil
    ) async throws -> Int {
        let auth = try await authorize()
        var startFileName: String? = fileName
        var startFileID: String? = nil
        var deleted = 0
        var pages = 0

        while let currentStart = startFileName, pages < 50 {
            pages += 1
            let response = try await listFileVersions(
                auth: auth,
                bucketID: bucketID,
                startFileName: currentStart,
                startFileID: startFileID,
                prefix: fileName,
                maxFileCount: 1000
            )

            var sawTarget = false
            for file in response.files {
                guard file.fileName == fileName, let fileID = file.fileID, !fileID.isEmpty else {
                    if file.fileName > fileName {
                        startFileName = nil
                        startFileID = nil
                    }
                    continue
                }
                sawTarget = true
                if let keepingFileID, fileID == keepingFileID {
                    continue
                }
                try await deleteFileVersion(fileName: file.fileName, fileID: fileID)
                deleted += 1
            }

            if !sawTarget && deleted > 0 {
                break
            }
            if response.nextFileName == nil || (response.nextFileName != fileName && deleted > 0) {
                break
            }
            startFileName = response.nextFileName
            startFileID = response.nextFileID
        }

        if deleted == 0, let knownFileID, knownFileID != keepingFileID {
            try await deleteFileVersion(fileName: fileName, fileID: knownFileID)
            deleted = 1
        }

        Diagnostics.shared.info(
            "b2.delete.hard",
            area: "b2",
            fields: [
                "fileName": fileName,
                "versionsDeleted": "\(deleted)",
                "keptFileID": keepingFileID ?? ""
            ]
        )
        return deleted
    }

    /// Hard-deletes every object version under a folder prefix.
    @discardableResult
    private func permanentlyDeletePrefix(bucketID: String, prefix: String) async throws -> Int {
        let auth = try await authorize()
        var startFileName: String? = nil
        var startFileID: String? = nil
        var deleted = 0
        var pages = 0

        while pages < 200 {
            pages += 1
            let response = try await listFileVersions(
                auth: auth,
                bucketID: bucketID,
                startFileName: startFileName,
                startFileID: startFileID,
                prefix: prefix,
                maxFileCount: 1000
            )
            if response.files.isEmpty {
                break
            }
            for file in response.files {
                guard file.fileName.hasPrefix(prefix), let fileID = file.fileID, !fileID.isEmpty else {
                    continue
                }
                try await deleteFileVersion(fileName: file.fileName, fileID: fileID)
                deleted += 1
            }
            guard let nextName = response.nextFileName else { break }
            startFileName = nextName
            startFileID = response.nextFileID
        }

        return deleted
    }

    private func listFileVersions(
        auth: B2Authorization,
        bucketID: String,
        startFileName: String?,
        startFileID: String?,
        prefix: String?,
        maxFileCount: Int
    ) async throws -> B2ListFileVersionsResponse {
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_list_file_versions")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2ListFileVersionsRequest(
                bucketID: bucketID,
                startFileName: startFileName,
                startFileID: startFileID,
                prefix: prefix,
                maxFileCount: maxFileCount
            )
        )
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        let (data, _) = try await client.data(for: request)
        return try JSONDecoder().decode(B2ListFileVersionsResponse.self, from: data)
    }

    /// Rename/move a file with `b2_copy_file` + hard-delete of the source object (all versions).
    private func renameOrMoveFile(
        key: ProviderItemKey,
        identifier: String,
        newName: String?,
        newParentIdentifier: String?,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        guard let sourceFileID = key.extra["fileID"], !sourceFileID.isEmpty,
              let sourceFileName = key.extra["fileName"] ?? Optional(key.name),
              let sourceBucketID = key.extra["bucketID"] else {
            throw RemoteFileError.notFound(identifier)
        }
        let parentIdentifier = newParentIdentifier ?? key.parentItemID ?? key.parentRemoteID
        let parent = try await parentLocation(for: parentIdentifier)
        let finalName = try Self.sanitizedFileName(newName ?? key.name)
        let destinationName = parent.prefix + finalName

        if destinationName == sourceFileName, contentsURL == nil {
            return try await item(for: identifier)
        }

        let result: RemoteFileItem
        if let contentsURL {
            // New bytes from Files: upload to the destination key, then wipe source.
            result = try await upload(
                bucketID: parent.bucketID,
                bucketName: parent.bucketName,
                fileName: destinationName,
                contentsURL: contentsURL,
                emptyBody: false,
                contentType: contentType ?? key.contentType ?? "b2/x-auto",
                parentItemID: parentIdentifier
            )
        } else {
            // Pure rename/move: server-side copy (no download), then hard-delete source.
            let copied = try await copyFile(
                sourceFileID: sourceFileID,
                destinationFileName: destinationName,
                destinationBucketID: parent.bucketID
            )
            result = makeItem(
                kind: .file,
                name: finalName,
                remoteID: "file:\(copied.fileID)",
                parentRemoteID: parent.remoteID,
                parentItemID: parentIdentifier,
                size: copied.contentLength ?? key.size,
                modifiedAt: copied.uploadTimestamp.map { Date(timeIntervalSince1970: TimeInterval($0) / 1000) } ?? Date(),
                contentType: copied.contentType ?? key.contentType,
                extra: [
                    "bucketID": parent.bucketID,
                    "bucketName": parent.bucketName,
                    "fileID": copied.fileID,
                    "fileName": copied.fileName
                ]
            )
            Diagnostics.shared.info(
                "b2.api.copy_file",
                area: "b2",
                fields: [
                    "from": sourceFileName,
                    "to": destinationName,
                    "sourceFileID": sourceFileID,
                    "destFileID": copied.fileID
                ]
            )
        }

        // Always hard-delete the old object name (all versions). Never leave a hide marker.
        if destinationName != sourceFileName {
            let deleted = try await permanentlyDeleteAllVersions(
                bucketID: sourceBucketID,
                fileName: sourceFileName,
                knownFileID: sourceFileID
            )
            guard deleted > 0 else {
                throw RemoteFileError.server(
                    "Renamed to \(destinationName), but failed to hard-delete source \(sourceFileName)."
                )
            }
        }
        return result
    }

    private func renameOrMoveFolder(
        key: ProviderItemKey,
        identifier: String,
        newName: String?,
        newParentIdentifier: String?
    ) async throws -> RemoteFileItem {
        guard let bucketID = key.extra["bucketID"],
              let oldPrefix = key.extra["prefix"] else {
            throw RemoteFileError.notFound(identifier)
        }
        let parentIdentifier = newParentIdentifier ?? key.parentItemID ?? key.parentRemoteID
        let parent = try await parentLocation(for: parentIdentifier)
        let finalName = try Self.sanitizedFileName(newName ?? key.name)
        let newPrefix = parent.prefix + finalName + "/"

        if newPrefix == oldPrefix {
            return try await item(for: identifier)
        }

        let auth = try await authorize()
        var startFileName: String? = nil
        var startFileID: String? = nil
        var movedAny = false
        var pages = 0

        // Walk versions under the old prefix so hidden/historical objects don't linger.
        while pages < 200 {
            pages += 1
            let response = try await listFileVersions(
                auth: auth,
                bucketID: bucketID,
                startFileName: startFileName,
                startFileID: startFileID,
                prefix: oldPrefix,
                maxFileCount: 1000
            )
            if response.files.isEmpty {
                break
            }
            for file in response.files {
                guard file.fileName.hasPrefix(oldPrefix), let fileID = file.fileID, !fileID.isEmpty else {
                    continue
                }
                let suffix = String(file.fileName.dropFirst(oldPrefix.count))
                let destinationName = newPrefix + suffix
                _ = try await copyFile(
                    sourceFileID: fileID,
                    destinationFileName: destinationName,
                    destinationBucketID: parent.bucketID
                )
                try await deleteFileVersion(fileName: file.fileName, fileID: fileID)
                movedAny = true
            }
            guard let nextName = response.nextFileName else { break }
            startFileName = nextName
            startFileID = response.nextFileID
        }

        if !movedAny {
            // Empty folder: create a marker at the new location.
            _ = try await upload(
                bucketID: parent.bucketID,
                bucketName: parent.bucketName,
                fileName: newPrefix,
                contentsURL: nil,
                emptyBody: true,
                contentType: "application/x-directory",
                parentItemID: parentIdentifier
            )
        }

        Diagnostics.shared.info(
            "b2.folder.rename.copied",
            area: "b2",
            fields: [
                "from": oldPrefix,
                "to": newPrefix,
                "movedAny": movedAny ? "1" : "0"
            ]
        )

        return makeItem(
            kind: .folder,
            name: finalName,
            remoteID: "folder:\(parent.bucketID):\(newPrefix)",
            parentRemoteID: parent.remoteID,
            parentItemID: parentIdentifier,
            size: 0,
            modifiedAt: Date(),
            contentType: nil,
            extra: [
                "bucketID": parent.bucketID,
                "bucketName": parent.bucketName,
                "prefix": newPrefix
            ]
        )
    }

    @discardableResult
    private func copyFile(sourceFileID: String, destinationFileName: String, destinationBucketID: String) async throws -> B2UploadedFile {
        let auth = try await authorize()
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_copy_file")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2CopyFileRequest(
                sourceFileID: sourceFileID,
                fileName: destinationFileName,
                destinationBucketID: destinationBucketID
            )
        )
        request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
        let (data, _) = try await client.data(for: request)
        return try JSONDecoder().decode(B2UploadedFile.self, from: data)
    }

    private static func sanitizedFileName(_ name: String) throws -> String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw RemoteFileError.invalidResponse("File name cannot be empty.")
        }
        guard !trimmed.contains("/") else {
            throw RemoteFileError.invalidResponse("File name cannot contain “/”.")
        }
        return trimmed
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
        do {
            let decoded = try JSONDecoder().decode(B2Authorization.self, from: data)
            authorization = decoded
            return decoded
        } catch {
            let preview = String(data: data.prefix(240), encoding: .utf8) ?? "<non-utf8>"
            throw RemoteFileError.invalidResponse("Could not decode B2 authorize response: \(error.localizedDescription). Body: \(preview)")
        }
    }

    private func listBuckets(auth: B2Authorization) async throws -> [B2Bucket] {
        let configuredName = connection.b2.bucketName.nilIfEmpty
        let allowedBuckets = auth.allowedBuckets

        // Restricted application keys often only expose buckets via the authorize `allowed` field.
        // Prefer that when we already have a full id+name pair, especially when the key is bucket-scoped.
        if let configuredName {
            if let allowed = allowedBuckets.first(where: { $0.bucketName == configuredName }) {
                return [allowed]
            }
        } else if !allowedBuckets.isEmpty, auth.isBucketRestricted {
            return allowedBuckets
        }

        do {
            let url = URL(string: auth.apiURL + "/b2api/v4/b2_list_buckets")!
            var request = try URLRequest.jsonPost(
                url: url,
                body: B2ListBucketsRequest(
                    accountID: auth.accountID,
                    bucketID: allowedBuckets.first(where: { $0.bucketName == configuredName })?.bucketID
                        ?? (configuredName == nil && allowedBuckets.count == 1 ? allowedBuckets[0].bucketID : nil),
                    bucketName: configuredName
                )
            )
            request.setValue(auth.authorizationToken, forHTTPHeaderField: "Authorization")
            let (data, _) = try await client.data(for: request)
            let buckets = try JSONDecoder().decode(B2ListBucketsResponse.self, from: data).buckets
            if !buckets.isEmpty {
                return buckets
            }
        } catch {
            // Fall through to allowed-bucket recovery for restricted keys.
            if allowedBuckets.isEmpty {
                throw error
            }
            Diagnostics.shared.error(
                "b2.list_buckets.failed.using_allowed",
                area: "b2",
                error: error,
                fields: ["allowedCount": "\(allowedBuckets.count)"]
            )
        }

        if let configuredName {
            if let allowed = allowedBuckets.first(where: { $0.bucketName == configuredName }) {
                return [allowed]
            }
            throw RemoteFileError.server(
                "B2 key cannot list bucket “\(configuredName)”. Check the bucket name and that the application key can access it."
            )
        }

        if !allowedBuckets.isEmpty {
            return allowedBuckets
        }

        throw RemoteFileError.server("B2 returned no buckets for this application key.")
    }

    private func listFileNames(
        auth: B2Authorization,
        bucketID: String,
        bucketName: String,
        prefix: String,
        delimiter: String?,
        parentItemID: String
    ) async throws -> [RemoteFileItem] {
        let url = URL(string: auth.apiURL + "/b2api/v4/b2_list_file_names")!
        var request = try URLRequest.jsonPost(
            url: url,
            body: B2ListFileNamesRequest(bucketID: bucketID, prefix: prefix, delimiter: delimiter, maxFileCount: 1000)
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
    var allowedBuckets: [B2Bucket]
    var isBucketRestricted: Bool

    enum CodingKeys: String, CodingKey {
        case accountID = "accountId"
        case authorizationToken
        case apiInfo
        case apiURL = "apiUrl"
        case downloadURL = "downloadUrl"
        case allowed
    }

    enum APIInfoKeys: String, CodingKey {
        case storageApi
    }

    enum StorageAPIKeys: String, CodingKey {
        case apiURL = "apiUrl"
        case downloadURL = "downloadUrl"
    }

    enum AllowedKeys: String, CodingKey {
        case buckets
        case bucketID = "bucketId"
        case bucketName
    }

    enum AllowedBucketKeys: String, CodingKey {
        case id
        case name
        case bucketID = "bucketId"
        case bucketName
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

        if container.contains(.allowed) {
            let allowed = try container.nestedContainer(keyedBy: AllowedKeys.self, forKey: .allowed)
            if let buckets = try allowed.decodeIfPresent([B2AllowedBucket].self, forKey: .buckets) {
                allowedBuckets = buckets.compactMap(\.bucket)
                isBucketRestricted = !allowedBuckets.isEmpty
            } else if let bucketID = try allowed.decodeIfPresent(String.self, forKey: .bucketID),
                      let bucketName = try allowed.decodeIfPresent(String.self, forKey: .bucketName) {
                allowedBuckets = [B2Bucket(bucketID: bucketID, bucketName: bucketName)]
                isBucketRestricted = true
            } else {
                allowedBuckets = []
                isBucketRestricted = false
            }
        } else {
            allowedBuckets = []
            isBucketRestricted = false
        }
    }
}

private struct B2AllowedBucket: Decodable {
    var bucketID: String?
    var bucketName: String?

    enum CodingKeys: String, CodingKey {
        case id
        case name
        case bucketID = "bucketId"
        case bucketName
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        bucketID = try container.decodeIfPresent(String.self, forKey: .bucketID)
            ?? container.decodeIfPresent(String.self, forKey: .id)
        bucketName = try container.decodeIfPresent(String.self, forKey: .bucketName)
            ?? container.decodeIfPresent(String.self, forKey: .name)
    }

    var bucket: B2Bucket? {
        guard let bucketID, let bucketName, !bucketID.isEmpty, !bucketName.isEmpty else {
            return nil
        }
        return B2Bucket(bucketID: bucketID, bucketName: bucketName)
    }
}

private struct B2ListBucketsRequest: Encodable {
    var accountID: String
    var bucketID: String?
    var bucketName: String?

    enum CodingKeys: String, CodingKey {
        case accountID = "accountId"
        case bucketID = "bucketId"
        case bucketName
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(accountID, forKey: .accountID)
        try container.encodeIfPresent(bucketID, forKey: .bucketID)
        try container.encodeIfPresent(bucketName, forKey: .bucketName)
    }
}

private struct B2ListBucketsResponse: Decodable {
    var buckets: [B2Bucket]
}

private struct B2Bucket: Decodable, Equatable {
    var bucketID: String
    var bucketName: String

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
        case bucketName
    }
}

private struct B2ListFileNamesRequest: Encodable {
    var bucketID: String
    var prefix: String
    var delimiter: String?
    var maxFileCount: Int
    var startFileName: String? = nil

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
        case prefix
        case delimiter
        case maxFileCount
        case startFileName
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(bucketID, forKey: .bucketID)
        try container.encode(prefix, forKey: .prefix)
        try container.encodeIfPresent(delimiter, forKey: .delimiter)
        try container.encode(maxFileCount, forKey: .maxFileCount)
        try container.encodeIfPresent(startFileName, forKey: .startFileName)
    }
}

private struct B2ListFileNamesResponse: Decodable {
    var files: [B2File]
    var nextFileName: String?
}

private struct B2ListFileVersionsRequest: Encodable {
    var bucketID: String
    var startFileName: String?
    var startFileID: String?
    var prefix: String?
    var maxFileCount: Int

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
        case startFileName
        case startFileID = "startFileId"
        case prefix
        case maxFileCount
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(bucketID, forKey: .bucketID)
        try container.encodeIfPresent(startFileName, forKey: .startFileName)
        try container.encodeIfPresent(startFileID, forKey: .startFileID)
        try container.encodeIfPresent(prefix, forKey: .prefix)
        try container.encode(maxFileCount, forKey: .maxFileCount)
    }
}

private struct B2ListFileVersionsResponse: Decodable {
    var files: [B2File]
    var nextFileName: String?
    var nextFileID: String?

    enum CodingKeys: String, CodingKey {
        case files
        case nextFileName
        case nextFileID = "nextFileId"
    }
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

private struct B2ParentLocation {
    var bucketID: String
    var bucketName: String
    var prefix: String
    var remoteID: String
}

private struct B2GetUploadURLRequest: Encodable {
    var bucketID: String

    enum CodingKeys: String, CodingKey {
        case bucketID = "bucketId"
    }
}

private struct B2UploadURL: Decodable {
    var uploadURL: URL
    var authorizationToken: String

    enum CodingKeys: String, CodingKey {
        case uploadURL = "uploadUrl"
        case authorizationToken
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let uploadURLString = try container.decode(String.self, forKey: .uploadURL)
        guard let url = URL(string: uploadURLString) else {
            throw DecodingError.dataCorruptedError(forKey: .uploadURL, in: container, debugDescription: "Invalid upload URL")
        }
        uploadURL = url
        authorizationToken = try container.decode(String.self, forKey: .authorizationToken)
    }
}

private struct B2UploadedFile: Decodable {
    var fileID: String
    var fileName: String
    var contentLength: Int64?
    var contentType: String?
    var uploadTimestamp: Int64?

    enum CodingKeys: String, CodingKey {
        case fileID = "fileId"
        case fileName
        case contentLength
        case contentType
        case uploadTimestamp
    }
}

private struct B2DeleteFileVersionRequest: Encodable {
    var fileName: String
    var fileID: String

    enum CodingKeys: String, CodingKey {
        case fileName
        case fileID = "fileId"
    }
}

private struct B2CopyFileRequest: Encodable {
    var sourceFileID: String
    var fileName: String
    var destinationBucketID: String

    enum CodingKeys: String, CodingKey {
        case sourceFileID = "sourceFileId"
        case fileName
        case destinationBucketID = "destinationBucketId"
    }
}

private extension String {
    var nilIfEmpty: String? {
        let value = trimmed
        return value.isEmpty ? nil : value
    }

    /// B2 requires percent-encoded UTF-8 file names in the X-Bz-File-Name header.
    var b2PercentEncodedFileName: String {
        var allowed = CharacterSet.urlPathAllowed
        allowed.remove(charactersIn: "/")
        // Encode path segments but keep `/` separators for nested object keys.
        return split(separator: "/", omittingEmptySubsequences: false)
            .map { component in
                String(component).addingPercentEncoding(withAllowedCharacters: allowed) ?? String(component)
            }
            .joined(separator: "/")
    }
}
