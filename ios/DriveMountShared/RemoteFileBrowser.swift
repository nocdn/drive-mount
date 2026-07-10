import Foundation

protocol RemoteFileBrowsing: Sendable {
    func item(for identifier: String) async throws -> RemoteFileItem
    func children(of identifier: String) async throws -> [RemoteFileItem]
    func contents(of identifier: String) async throws -> URL
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
