import Citadel
import Foundation
import NIOCore

/// Process-wide SFTP sessions so Files can list and then stream a multi-GB file
/// without repeating the SSH handshake on every File Provider call.
enum SeedboxSFTPSessionPool {
    private static let lock = NSLock()
    private static var transports: [String: SeedboxSFTPTransport] = [:]

    static func transport(for settings: SeedboxConnectionSettings) -> SeedboxSFTPTransport {
        let normalized = settings.normalized()
        let key = [
            normalized.host.lowercased(),
            String(normalized.effectiveSFTPPort),
            normalized.username,
            String(normalized.password.hashValue)
        ].joined(separator: "|")

        lock.lock()
        defer { lock.unlock() }
        if let existing = transports[key] {
            return existing
        }
        let created = SeedboxSFTPTransport(settings: normalized)
        transports[key] = created
        return created
    }
}

final class SeedboxSFTPTransport: SeedboxTransporting, @unchecked Sendable {
    private let settings: SeedboxConnectionSettings
    private let lock = NSLock()
    private var client: SSHClient?
    private var sftp: SFTPClient?

    /// 256 KiB reads keep File Provider memory flat while still moving 1–12 GB files quickly.
    static let downloadChunkSize: UInt32 = 256 * 1024

    init(settings: SeedboxConnectionSettings) {
        self.settings = settings
    }

    func list(path: String) async throws -> [SeedboxEntry] {
        try await withSFTP { sftp in
            let listings = try await sftp.listDirectory(atPath: path)
            var entries: [SeedboxEntry] = []
            for listing in listings {
                for component in listing.components {
                    if component.filename == "." || component.filename == ".." {
                        continue
                    }
                    entries.append(Self.entry(from: component, parentPath: path))
                }
            }
            return entries.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        }
    }

    func stat(path: String) async throws -> SeedboxEntry {
        try await withSFTP { sftp in
            let attributes = try await sftp.getAttributes(at: path)
            let name = path.split(separator: "/").last.map(String.init) ?? path
            return SeedboxEntry(
                name: name,
                path: path,
                isDirectory: Self.isDirectory(attributes),
                size: attributes.size.map { Int64(clamping: $0) },
                modifiedAt: Self.modificationDate(attributes)
            )
        }
    }

    func download(path: String, to destination: URL, expectedSize: Int64?) async throws {
        try await withSFTP { sftp in
            try await sftp.withFile(filePath: path, flags: .read) { file in
                if FileManager.default.fileExists(atPath: destination.path) {
                    try FileManager.default.removeItem(at: destination)
                }
                FileManager.default.createFile(atPath: destination.path, contents: nil)
                let handle = try FileHandle(forWritingTo: destination)
                defer { try? handle.close() }

                var offset: UInt64 = 0
                while true {
                    try Task.checkCancellation()
                    var buffer = try await file.read(from: offset, length: Self.downloadChunkSize)
                    let byteCount = buffer.readableBytes
                    if byteCount == 0 {
                        break
                    }
                    if let data = buffer.readData(length: byteCount) {
                        try handle.write(contentsOf: data)
                    }
                    offset += UInt64(byteCount)
                    if byteCount < Self.downloadChunkSize {
                        break
                    }
                }

                if let expectedSize, offset != UInt64(expectedSize) {
                    throw RemoteFileError.invalidResponse(
                        "Downloaded \(offset) bytes, but the provider reported \(expectedSize) bytes."
                    )
                }
                Diagnostics.shared.info(
                    "seedbox.download.finished",
                    area: "seedbox",
                    fields: ["bytes": "\(offset)", "path": path]
                )
            }
        }
    }

    func createDirectory(path: String) async throws {
        try await withSFTP { sftp in
            try await sftp.createDirectory(atPath: path)
        }
    }

    func remove(path: String, isDirectory: Bool) async throws {
        try await withSFTP { sftp in
            if isDirectory {
                try await self.removeDirectory(sftp: sftp, path: path)
            } else {
                try await sftp.remove(at: path)
            }
        }
    }

    func rename(from oldPath: String, to newPath: String) async throws {
        try await withSFTP { sftp in
            try await sftp.rename(at: oldPath, to: newPath)
        }
    }

    func upload(from source: URL, to path: String) async throws {
        try await withSFTP { sftp in
            try await sftp.withFile(filePath: path, flags: [.write, .create, .truncate]) { file in
                let handle = try FileHandle(forReadingFrom: source)
                defer { try? handle.close() }
                var offset: UInt64 = 0
                while true {
                    try Task.checkCancellation()
                    guard let data = try handle.read(upToCount: 32_000), !data.isEmpty else {
                        break
                    }
                    try await file.write(ByteBuffer(data: data), at: offset)
                    offset += UInt64(data.count)
                }
            }
        }
    }

    private func removeDirectory(sftp: SFTPClient, path: String) async throws {
        let listings = try await sftp.listDirectory(atPath: path)
        for listing in listings {
            for component in listing.components where component.filename != "." && component.filename != ".." {
                let childPath = SeedboxRemoteFileBrowser.join(path, component.filename)
                if Self.isDirectory(component.attributes) {
                    try await removeDirectory(sftp: sftp, path: childPath)
                } else {
                    try await sftp.remove(at: childPath)
                }
            }
        }
        try await sftp.rmdir(at: path)
    }

    private func withSFTP<T>(_ body: (SFTPClient) async throws -> T) async throws -> T {
        do {
            let sftp = try await connectedSFTP()
            return try await body(sftp)
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            await invalidate()
            do {
                let sftp = try await connectedSFTP()
                return try await body(sftp)
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                throw Self.mapError(error)
            }
        }
    }

    private func connectedSFTP() async throws -> SFTPClient {
        if let sftp, let client, client.isConnected, sftp.isActive {
            return sftp
        }
        await invalidate()

        Diagnostics.shared.info(
            "seedbox.sftp.connect",
            area: "seedbox",
            fields: [
                "host": settings.host,
                "port": "\(settings.effectiveSFTPPort)"
            ]
        )

        let client = try await SSHClient.connect(
            host: settings.host,
            port: settings.effectiveSFTPPort,
            authenticationMethod: .passwordBased(username: settings.username, password: settings.password),
            hostKeyValidator: .acceptAnything(),
            reconnect: .never,
            algorithms: .all,
            connectTimeout: .seconds(20)
        )
        let sftp = try await client.openSFTP()
        lock.lock()
        self.client = client
        self.sftp = sftp
        lock.unlock()
        return sftp
    }

    private func invalidate() async {
        let existingSFTP: SFTPClient?
        let existingClient: SSHClient?
        lock.lock()
        existingSFTP = sftp
        existingClient = client
        sftp = nil
        client = nil
        lock.unlock()
        try? await existingSFTP?.close()
        try? await existingClient?.close()
    }

    private static func entry(from component: SFTPPathComponent, parentPath: String) -> SeedboxEntry {
        SeedboxEntry(
            name: component.filename,
            path: SeedboxRemoteFileBrowser.join(parentPath, component.filename),
            isDirectory: isDirectory(component.attributes),
            size: component.attributes.size.map { Int64(clamping: $0) },
            modifiedAt: modificationDate(component.attributes)
        )
    }

    private static func isDirectory(_ attributes: SFTPFileAttributes) -> Bool {
        if let permissions = attributes.permissions {
            return (permissions & 0o170000) == 0o040000
        }
        return false
    }

    private static func modificationDate(_ attributes: SFTPFileAttributes) -> Date? {
        attributes.accessModificationTime?.modificationTime
    }

    private static func mapError(_ error: Error) -> Error {
        if let remote = error as? RemoteFileError {
            return remote
        }
        let message = String(describing: error)
        let lowercased = message.lowercased()
        if lowercased.contains("auth") || lowercased.contains("permission denied") || lowercased.contains("password") {
            return RemoteFileError.missingCredentials("Seedbox")
        }
        if lowercased.contains("no such file") || lowercased.contains("not found") {
            return RemoteFileError.notFound(message)
        }
        return RemoteFileError.server(message)
    }
}
