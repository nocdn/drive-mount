import FileProvider
import Foundation
import Observation
import SwiftUI

@MainActor
@Observable
final class ConnectionListViewModel {
    var connections: [CloudConnection] = []
    var statusMessage = ""
    var registeredDomainCount = 0

    private let store: ConnectionStore

    init(store: ConnectionStore = ConnectionStore()) {
        self.store = store
    }

    func bootstrap() async {
        do {
            connections = try store.load()
            if ProcessInfo.processInfo.arguments.contains("--seed-b2-from-environment") {
                try await seedB2FromEnvironment()
            }
            let defaults = UserDefaults(suiteName: AppConstants.appGroupIdentifier) ?? .standard
            let storedRevision = defaults.integer(forKey: AppConstants.fileProviderStateRevisionKey)
            let resettingDomainIDs = Self.requiresFileProviderStateReset(storedRevision: storedRevision)
                ? Set(Self.fileProviderDomains(for: connections).map { $0.identifier.rawValue })
                : []
            if await syncFileProviderDomains(resettingDomainIDs: resettingDomainIDs) {
                defaults.set(
                    AppConstants.fileProviderStateRevision,
                    forKey: AppConstants.fileProviderStateRevisionKey
                )
            }
        } catch {
            statusMessage = "Could not load settings."
            Diagnostics.shared.error("settings.load.failed", area: "settings", error: error)
        }
    }

    var b2Connections: [CloudConnection] {
        connections.filter { $0.provider == .backblazeB2 }
    }

    var otherConnections: [CloudConnection] {
        connections.filter { $0.provider != .backblazeB2 }
    }

    var b2BucketRows: [B2BucketRow] {
        b2Connections.flatMap { connection in
            connection.b2.normalizedBucketNames.map { name in
                B2BucketRow(connectionID: connection.id, name: name)
            }
        }
    }

    func addConnection(provider: CloudProvider) async {
        if provider == .backblazeB2, connections.contains(where: { $0.provider == .backblazeB2 }) {
            statusMessage = "Add more buckets in the Backblaze B2 section."
            return
        }
        var connection = CloudConnection(provider: provider, displayName: provider.defaultConnectionName)
        if provider == .seedbox {
            connection.seedbox.remotePath = "downloads"
        }
        connections.append(connection.normalized())
        await persistAndSync(status: "Added \(provider.displayName).")
    }

    func saveConnection(_ connection: CloudConnection) async {
        guard let index = connections.firstIndex(where: { $0.id == connection.id }) else {
            return
        }
        let previous = connections[index]
        connections[index] = connection.normalized()
        await persistAndSync(
            status: "Saved \(connection.provider == .backblazeB2 ? "Backblaze B2" : connection.effectiveDisplayName).",
            resettingDomainIDs: Self.domainIdentifiers(for: [previous, connections[index]])
        )
    }

    func deleteConnection(id: String) async {
        guard let index = connections.firstIndex(where: { $0.id == id }) else {
            return
        }
        let removed = connections.remove(at: index)
        await persistAndSync(
            status: "Removed \(removed.provider.displayName).",
            resettingDomainIDs: Self.domainIdentifiers(for: [removed])
        )
    }

    func deleteConnections(at offsets: IndexSet) async {
        let removed = offsets.map { connections[$0] }
        connections.remove(atOffsets: offsets)
        await persistAndSync(
            status: "Removed \(removed.count) connection(s).",
            resettingDomainIDs: Self.domainIdentifiers(for: removed)
        )
    }

    func deleteOtherConnections(at offsets: IndexSet) async {
        let others = otherConnections
        let removed = offsets.map { others[$0] }
        let removedIDs = Set(removed.map(\.id))
        connections.removeAll { removedIDs.contains($0.id) }
        await persistAndSync(
            status: "Removed \(removed.count) connection(s).",
            resettingDomainIDs: Self.domainIdentifiers(for: removed)
        )
    }

    func deleteB2Buckets(at offsets: IndexSet) async {
        let rows = b2BucketRows
        let removedRows = offsets.map { rows[$0] }
        for row in removedRows {
            guard let index = connections.firstIndex(where: { $0.id == row.connectionID }) else {
                continue
            }
            connections[index].b2.bucketNames.removeAll {
                $0.trimmed.compare(row.name, options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame
            }
            connections[index] = connections[index].normalized()
        }
        await persistAndSync(
            status: "Removed \(removedRows.count) bucket(s).",
            resettingDomainIDs: Set(removedRows.map {
                B2FileProviderDomainIdentity.identifier(connectionID: $0.connectionID, bucketName: $0.name)
            })
        )
    }

    func binding(for id: String) -> Binding<CloudConnection>? {
        guard let index = connections.firstIndex(where: { $0.id == id }) else {
            return nil
        }
        return Binding(
            get: { self.connections[index] },
            set: { self.connections[index] = $0 }
        )
    }

    func persistAndSync(status: String, resettingDomainIDs: Set<String> = []) async {
        do {
            try store.save(connections.map { $0.normalized() })
            statusMessage = status
            await syncFileProviderDomains(resettingDomainIDs: resettingDomainIDs)
        } catch {
            statusMessage = "Could not save settings."
            Diagnostics.shared.error("settings.save.failed", area: "settings", error: error)
        }
    }

    @discardableResult
    func syncFileProviderDomains(resettingDomainIDs: Set<String> = []) async -> Bool {
        do {
            let existingDomains = try await currentDomains()
            let targetDomains = Self.fileProviderDomains(for: connections)
            let domainIDsToRemove = Self.domainIdentifiersToRemove(
                existingDomains: existingDomains,
                targetDomains: targetDomains,
                resettingDomainIDs: resettingDomainIDs
            )

            for domain in existingDomains where domainIDsToRemove.contains(domain.identifier.rawValue) {
                try await remove(domain: domain)
            }

            let existingDomainsByID = Dictionary(uniqueKeysWithValues: existingDomains
                .filter { !domainIDsToRemove.contains($0.identifier.rawValue) }
                .map { ($0.identifier.rawValue, $0) })
            for targetDomain in targetDomains {
                if let existingDomain = existingDomainsByID[targetDomain.identifier.rawValue] {
                    if existingDomain.displayName != targetDomain.displayName || !existingDomain.isReplicated {
                        try await add(domain: targetDomain)
                    }
                } else {
                    try await add(domain: targetDomain)
                }
            }

            let domains = try await currentDomains()
            registeredDomainCount = domains.count
            for domain in domains {
                await signalEnumerator(for: domain)
            }
            Diagnostics.shared.info("domains.sync.finished", area: "fileprovider", fields: ["count": "\(registeredDomainCount)"])
            return true
        } catch {
            statusMessage = "Files registration needs a signed File Provider build."
            Diagnostics.shared.error("domains.sync.failed", area: "fileprovider", error: error)
            return false
        }
    }

    /// Ask Files to re-enumerate after a successful settings sync.
    private func signalEnumerator(for domain: NSFileProviderDomain) async {
        guard let manager = NSFileProviderManager(for: domain) else {
            return
        }
        do {
            try await manager.signalEnumerator(for: .workingSet)
            try await manager.signalEnumerator(for: .rootContainer)
            Diagnostics.shared.info(
                "domains.signal.finished",
                area: "fileprovider",
                fields: ["domain": domain.identifier.rawValue]
            )
        } catch {
            Diagnostics.shared.error(
                "domains.signal.failed",
                area: "fileprovider",
                error: error,
                fields: ["domain": domain.identifier.rawValue]
            )
        }
    }

    private func currentDomains() async throws -> [NSFileProviderDomain] {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<[NSFileProviderDomain], Error>) in
            NSFileProviderManager.getDomainsWithCompletionHandler { domains, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: domains)
                }
            }
        }
    }

    private func add(domain: NSFileProviderDomain) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            NSFileProviderManager.add(domain) { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
    }

    static func fileProviderDomain(for connection: CloudConnection) -> NSFileProviderDomain {
        fileProviderDomains(for: [connection]).first ?? NSFileProviderDomain(
            identifier: NSFileProviderDomainIdentifier(connection.id),
            displayName: connection.effectiveDisplayName
        )
    }

    static func fileProviderDomains(for connections: [CloudConnection]) -> [NSFileProviderDomain] {
        connections
            .map { $0.normalized() }
            .filter { $0.isEnabled && $0.provider.supportsIOSFileProvider }
            .flatMap { connection -> [NSFileProviderDomain] in
                if connection.provider == .backblazeB2 {
                    return connection.b2.normalizedBucketNames.map { bucketName in
                        NSFileProviderDomain(
                            identifier: NSFileProviderDomainIdentifier(
                                B2FileProviderDomainIdentity.identifier(
                                    connectionID: connection.id,
                                    bucketName: bucketName
                                )
                            ),
                            displayName: bucketName
                        )
                    }
                }
                return [
                    NSFileProviderDomain(
                        identifier: NSFileProviderDomainIdentifier(connection.id),
                        displayName: connection.effectiveDisplayName
                    )
                ]
            }
            .sorted {
            $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
        }
    }

    static func domainIdentifiers(for connections: [CloudConnection]) -> Set<String> {
        Set(fileProviderDomains(for: connections).map { $0.identifier.rawValue })
    }

    static func domainIdentifiersToRemove(
        existingDomains: [NSFileProviderDomain],
        targetDomains: [NSFileProviderDomain],
        resettingDomainIDs: Set<String>
    ) -> Set<String> {
        let targetIDs = Set(targetDomains.map { $0.identifier.rawValue })
        return Set(existingDomains.compactMap { domain in
            let identifier = domain.identifier.rawValue
            return !targetIDs.contains(identifier) || resettingDomainIDs.contains(identifier)
                ? identifier
                : nil
        })
    }

    static func requiresFileProviderStateReset(storedRevision: Int) -> Bool {
        storedRevision < AppConstants.fileProviderStateRevision
    }

    private func remove(domain: NSFileProviderDomain) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            NSFileProviderManager.remove(domain, mode: .removeAll) { _, error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume(returning: ())
                }
            }
        }
    }

    private func seedB2FromEnvironment() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let keyID = env["DRIVEMOUNT_TEST_B2_KEY_ID"], !keyID.isEmpty,
              let applicationKey = env["DRIVEMOUNT_TEST_B2_APPLICATION_KEY"], !applicationKey.isEmpty else {
            return
        }

        let extraBuckets = env["DRIVEMOUNT_TEST_B2_BUCKETS"]?
            .split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty } ?? []
        let bucketName = extraBuckets.first ?? env["DRIVEMOUNT_TEST_B2_BUCKET"] ?? "nocdn-main"
        let bucketNames = extraBuckets.isEmpty ? [bucketName] : extraBuckets
        let existingIndex = connections.firstIndex { $0.provider == .backblazeB2 }
        var connection = existingIndex.map { connections[$0] } ?? CloudConnection(provider: .backblazeB2)
        connection.displayName = CloudProvider.backblazeB2.defaultConnectionName
        connection.isEnabled = true
        connection.b2 = B2ConnectionSettings(
            applicationKeyID: keyID,
            applicationKey: applicationKey,
            bucketNames: bucketNames
        )
        connection = connection.normalized()

        if let existingIndex {
            connections[existingIndex] = connection
        } else {
            connections.append(connection)
        }
        try store.save(connections)
        Diagnostics.shared.info("settings.seeded.b2", area: "settings", fields: ["bucket": bucketNames.joined(separator: ",")])
    }

    static var preview: ConnectionListViewModel {
        let model = ConnectionListViewModel()
        model.connections = [
            CloudConnection(
                provider: .backblazeB2,
                displayName: "B2",
                b2: B2ConnectionSettings(bucketNames: ["nocdn-main", "nocdn-music"])
            ),
            CloudConnection(provider: .googleDrive, displayName: "Google Drive")
        ]
        return model
    }
}

struct B2BucketRow: Identifiable, Equatable, Hashable {
    var connectionID: String
    var name: String

    var id: String { "\(connectionID)|\(name)" }
}
