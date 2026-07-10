import FileProvider
import Foundation

protocol RemoteFileBrowsing: Sendable {
    func item(for identifier: String) async throws -> RemoteFileItem
    func children(of identifier: String) async throws -> [RemoteFileItem]
    func contents(of identifier: String) async throws -> URL
    func createItem(
        name: String,
        parentIdentifier: String,
        isDirectory: Bool,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem
    func modifyItem(
        identifier: String,
        newName: String?,
        newParentIdentifier: String?,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem
    func deleteItem(identifier: String) async throws
}

extension RemoteFileBrowsing {
    func createItem(
        name: String,
        parentIdentifier: String,
        isDirectory: Bool,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        throw RemoteFileError.unsupported("Creating items is not supported for this provider yet.")
    }

    func modifyItem(
        identifier: String,
        newName: String?,
        newParentIdentifier: String?,
        contentsURL: URL?,
        contentType: String?
    ) async throws -> RemoteFileItem {
        throw RemoteFileError.unsupported("Modifying items is not supported for this provider yet.")
    }

    func deleteItem(identifier: String) async throws {
        throw RemoteFileError.unsupported("Deleting items is not supported for this provider yet.")
    }
}

enum RemoteFileError: LocalizedError, Equatable {
    case missingConnection
    case missingCredentials(String)
    case unsupported(String)
    case notFound(String)
    case invalidResponse(String)
    case server(String)

    var errorDescription: String? {
        switch self {
        case .missingConnection:
            "Connection is no longer configured."
        case .missingCredentials(let provider):
            "\(provider) credentials are missing."
        case .unsupported(let message):
            message
        case .notFound(let identifier):
            "Item was not found: \(identifier)"
        case .invalidResponse(let message):
            "Invalid response: \(message)"
        case .server(let message):
            message
        }
    }

    /// Map to File Provider error codes so Files can show the right retry/auth UI.
    var asFileProviderError: NSError {
        switch self {
        case .missingConnection, .missingCredentials:
            return fileProviderError(.notAuthenticated)
        case .notFound:
            return fileProviderError(.noSuchItem)
        case .unsupported:
            // Unsupported actions are capability/feature mismatches, not sync
            // failures. Reporting cannotSynchronize makes Files throttle the
            // domain and display a persistent circular error badge.
            return NSError(
                domain: NSCocoaErrorDomain,
                code: NSFeatureUnsupportedError,
                userInfo: [NSLocalizedDescriptionKey: errorDescription ?? "Unsupported operation"]
            )
        case .invalidResponse:
            return fileProviderError(.serverUnreachable)
        case .server(let message):
            if Self.isQuotaError(message) {
                return fileProviderError(.insufficientQuota)
            }
            if message.contains("HTTP 401") {
                return fileProviderError(.notAuthenticated)
            }
            return fileProviderError(.serverUnreachable)
        }
    }

    private func fileProviderError(_ code: NSFileProviderError.Code) -> NSError {
        return NSError(
            domain: NSFileProviderErrorDomain,
            code: code.rawValue,
            userInfo: [NSLocalizedDescriptionKey: errorDescription ?? "File Provider error"]
        )
    }

    private static func isQuotaError(_ message: String) -> Bool {
        let lowercased = message.lowercased()
        return lowercased.contains("download_cap_exceeded")
            || lowercased.contains("storage_cap_exceeded")
            || lowercased.contains("quota exceeded")
    }
}

extension Error {
    var asFileProviderError: NSError {
        if let remote = self as? RemoteFileError {
            return remote.asFileProviderError
        }
        let nsError = self as NSError
        if nsError.domain == NSFileProviderErrorDomain {
            return nsError
        }
        return NSError(
            domain: NSFileProviderErrorDomain,
            code: NSFileProviderError.Code.serverUnreachable.rawValue,
            userInfo: [
                NSLocalizedDescriptionKey: localizedDescription,
                NSUnderlyingErrorKey: self
            ]
        )
    }
}

struct RemoteFileBrowserFactory {
    static func browser(
        forDomainIdentifier domainIdentifier: String,
        displayName: String,
        connections: [CloudConnection]
    ) -> any RemoteFileBrowsing {
        let enabledConnections = connections.map { $0.normalized() }.filter(\.isEnabled)

        if domainIdentifier == AppConstants.b2FileProviderDomainIdentifier {
            let b2Connections = enabledConnections.filter { $0.provider == .backblazeB2 }
            guard !b2Connections.isEmpty else {
                return FixtureRemoteFileBrowser(
                    connection: CloudConnection(provider: .backblazeB2, displayName: displayName),
                    reason: .missingConnection
                )
            }
            return B2GroupedRemoteFileBrowser(connections: b2Connections)
        }

        guard let connection = enabledConnections.first(where: { $0.id == domainIdentifier }) else {
            return FixtureRemoteFileBrowser(
                connection: CloudConnection(provider: .backblazeB2, displayName: displayName),
                reason: .missingConnection
            )
        }
        return browser(for: connection)
    }

    static func browser(for connection: CloudConnection) -> any RemoteFileBrowsing {
        guard connection.hasMinimumConfiguration else {
            return FixtureRemoteFileBrowser(connection: connection, reason: .missingCredentials(connection.provider.displayName))
        }

        switch connection.provider {
        case .backblazeB2:
            return B2RemoteFileBrowser(connection: connection)
        case .googleDrive:
            return GoogleDriveRemoteFileBrowser(connection: connection)
        case .oneDrive:
            return OneDriveRemoteFileBrowser(connection: connection)
        case .seedbox:
            return FixtureRemoteFileBrowser(
                connection: connection,
                reason: .unsupported("Seedbox FTP browsing needs a native FTP transport implementation.")
            )
        }
    }
}
