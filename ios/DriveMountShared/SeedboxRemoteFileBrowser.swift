import Foundation
import UniformTypeIdentifiers

struct SeedboxEntry: Equatable, Sendable {
    var name: String
    var path: String
    var isDirectory: Bool
    var size: Int64?
    var modifiedAt: Date?
}

protocol SeedboxTransporting: Sendable {
    func list(path: String) async throws -> [SeedboxEntry]
    func stat(path: String) async throws -> SeedboxEntry
    func download(path: String, to destination: URL, expectedSize: Int64?) async throws
    func createDirectory(path: String) async throws
    func remove(path: String, isDirectory: Bool) async throws
    func rename(from oldPath: String, to newPath: String) async throws
    func upload(from source: URL, to path: String) async throws
}

final class SeedboxRemoteFileBrowser: RemoteFileBrowsing, @unchecked Sendable {
    private let connection: CloudConnection
    private let transport: any SeedboxTransporting

    init(connection: CloudConnection, transport: (any SeedboxTransporting)? = nil) {
        self.connection = connection
        self.transport = transport ?? SeedboxSFTPSessionPool.transport(for: connection.seedbox)
    }

    func item(for identifier: String) async throws -> RemoteFileItem {
        if identifier == RemoteFileItem.rootID {
            return .root(
                displayName: connection.effectiveDisplayName,
                allowsAddingSubItems: !connection.seedbox.readOnly
            )
        }
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        return makeItem(from: key, parentItemID: key.parentItemID ?? key.parentRemoteID)
    }

    func children(of identifier: String) async throws -> [RemoteFileItem] {
        let parentPath: String
        let parentItemID: String
        let parentRemoteID: String
        if identifier == RemoteFileItem.rootID {
            parentPath = rootPath
            parentItemID = RemoteFileItem.rootID
            parentRemoteID = RemoteFileItem.rootID
        } else if let key = ProviderItemKey.decode(identifier) {
            parentPath = remotePath(from: key)
            parentItemID = identifier
            parentRemoteID = key.remoteID
        } else {
            return []
        }

        return try await transport.list(path: parentPath).map { entry in
            makeItem(
                entry: entry,
                parentRemoteID: parentRemoteID,
                parentItemID: parentItemID
            )
        }
    }

    func contents(of identifier: String) async throws -> URL {
        guard let key = ProviderItemKey.decode(identifier), key.kind == .file else {
            throw RemoteFileError.notFound(identifier)
        }
        let destination = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString)
        let pathExtension = (key.name as NSString).pathExtension
        let destinationURL = pathExtension.isEmpty
            ? destination
            : destination.appendingPathExtension(pathExtension)
        try await transport.download(
            path: remotePath(from: key),
            to: destinationURL,
            expectedSize: key.size
        )
        return destinationURL
    }

    func createItem(
        name: String,
        parentIdentifier: String,
        isDirectory: Bool,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        try ensureWritable()
        let parentPath = try resolvedPath(for: parentIdentifier)
        let fileName = try Self.sanitizedFileName(name)
        let path = Self.join(parentPath, fileName)
        if isDirectory {
            try await transport.createDirectory(path: path)
            return makeItem(
                entry: SeedboxEntry(name: fileName, path: path, isDirectory: true, size: nil, modifiedAt: Date()),
                parentRemoteID: parentIdentifier == RemoteFileItem.rootID ? RemoteFileItem.rootID : parentIdentifier,
                parentItemID: parentIdentifier == RemoteFileItem.rootID ? RemoteFileItem.rootID : parentIdentifier
            )
        }

        if let contentsURL {
            try await transport.upload(from: contentsURL, to: path)
        } else {
            let empty = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            FileManager.default.createFile(atPath: empty.path, contents: Data())
            defer { try? FileManager.default.removeItem(at: empty) }
            try await transport.upload(from: empty, to: path)
        }
        let uploaded = try await transport.stat(path: path)
        return makeItem(
            entry: uploaded,
            parentRemoteID: parentIdentifier == RemoteFileItem.rootID ? RemoteFileItem.rootID : parentIdentifier,
            parentItemID: parentIdentifier == RemoteFileItem.rootID ? RemoteFileItem.rootID : parentIdentifier
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
        let currentParent = key.parentItemID ?? key.parentRemoteID
        let wantsMove = newParentIdentifier.map { $0 != currentParent } ?? false
        let wantsContent = contentsURL != nil

        if !wantsRename && !wantsMove && !wantsContent {
            return try await item(for: identifier)
        }
        try ensureWritable()

        var destinationParent = currentParent
        var destinationParentPath = parentPath(of: remotePath(from: key))
        if wantsMove, let newParentIdentifier {
            destinationParent = newParentIdentifier
            destinationParentPath = try resolvedPath(for: newParentIdentifier)
        }
        let destinationName = wantsRename ? try Self.sanitizedFileName(newName ?? key.name) : key.name
        let destinationPath = Self.join(destinationParentPath, destinationName)
        let sourcePath = remotePath(from: key)

        if wantsRename || wantsMove, destinationPath != sourcePath {
            try await transport.rename(from: sourcePath, to: destinationPath)
        }
        if let contentsURL {
            try await transport.upload(from: contentsURL, to: destinationPath)
        }

        let result = try await transport.stat(path: destinationPath)
        return makeItem(
            entry: result,
            parentRemoteID: destinationParent,
            parentItemID: destinationParent
        )
    }

    func deleteItem(identifier: String) async throws {
        try ensureWritable()
        guard identifier != RemoteFileItem.rootID else {
            throw RemoteFileError.unsupported("The Seedbox root cannot be deleted.")
        }
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        try await transport.remove(path: remotePath(from: key), isDirectory: key.kind == .folder)
    }

    private var rootPath: String {
        let path = connection.seedbox.remotePath.trimmed
        return path.isEmpty ? "." : path
    }

    private func ensureWritable() throws {
        if connection.seedbox.readOnly {
            throw RemoteFileError.unsupported("This Seedbox connection is read-only.")
        }
    }

    private func resolvedPath(for identifier: String) throws -> String {
        if identifier == RemoteFileItem.rootID {
            return rootPath
        }
        guard let key = ProviderItemKey.decode(identifier) else {
            throw RemoteFileError.notFound(identifier)
        }
        return remotePath(from: key)
    }

    private func remotePath(from key: ProviderItemKey) -> String {
        key.extra["path"] ?? key.remoteID.replacingOccurrences(of: "sftp:", with: "")
    }

    private func parentPath(of path: String) -> String {
        let parts = path.split(separator: "/").map(String.init)
        guard parts.count > 1 else {
            return rootPath
        }
        return parts.dropLast().joined(separator: "/")
    }

    private func makeItem(from key: ProviderItemKey, parentItemID: String) -> RemoteFileItem {
        RemoteFileItem(
            key: key,
            parentID: parentItemID,
            filename: key.name,
            isDirectory: key.kind == .folder,
            size: key.size,
            modifiedAt: key.modifiedAt,
            contentType: key.contentType
        )
    }

    private func makeItem(entry: SeedboxEntry, parentRemoteID: String, parentItemID: String) -> RemoteFileItem {
        let key = ProviderItemKey(
            provider: .seedbox,
            kind: entry.isDirectory ? .folder : .file,
            name: entry.name,
            remoteID: "sftp:\(entry.path)",
            parentRemoteID: parentRemoteID,
            parentItemID: parentItemID,
            size: entry.size,
            modifiedAt: entry.modifiedAt,
            contentType: entry.isDirectory ? nil : UTType(filenameExtension: (entry.name as NSString).pathExtension)?.preferredMIMEType,
            extra: [
                "path": entry.path,
                "writable": connection.seedbox.readOnly ? "0" : "1"
            ]
        )
        return RemoteFileItem(
            key: key,
            parentID: parentItemID,
            filename: entry.name,
            isDirectory: entry.isDirectory,
            size: entry.size,
            modifiedAt: entry.modifiedAt,
            contentType: key.contentType
        )
    }

    static func sanitizedFileName(_ name: String) throws -> String {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty || trimmed == "." || trimmed == ".." || trimmed.contains("/") || trimmed.contains("\\") {
            throw RemoteFileError.invalidResponse("Invalid file name.")
        }
        return trimmed
    }

    static func join(_ base: String, _ name: String) -> String {
        if base.isEmpty || base == "." {
            return name
        }
        if base.hasSuffix("/") {
            return base + name
        }
        return base + "/" + name
    }
}
