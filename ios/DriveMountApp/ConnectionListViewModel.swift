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

    func addConnection(provider: CloudProvider) async {
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
        connections[index] = connection.normalized()
        let domainID = Self.fileProviderDomain(for: connection).identifier.rawValue
        await persistAndSync(
            status: "Saved \(connection.effectiveDisplayName).",
            resettingDomainIDs: [domainID]
        )
    }

    func deleteConnections(at offsets: IndexSet) async {
        let removedIDs = offsets.map { connections[$0].id }
        let removedDomainIDs = Set(offsets.map {
            Self.fileProviderDomain(for: connections[$0]).identifier.rawValue
        })
        connections.remove(atOffsets: offsets)
        await persistAndSync(
            status: "Removed \(removedIDs.count) connection(s).",
            resettingDomainIDs: removedDomainIDs
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
        return NSFileProviderDomain(
            identifier: NSFileProviderDomainIdentifier(connection.id),
            displayName: connection.effectiveDisplayName
        )
    }

    static func fileProviderDomains(for connections: [CloudConnection]) -> [NSFileProviderDomain] {
        connections
            .map { $0.normalized() }
            .filter { $0.isEnabled && $0.provider.supportsIOSFileProvider }
            .map(fileProviderDomain(for:))
            .sorted {
            $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
        }
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

        let bucketName = env["DRIVEMOUNT_TEST_B2_BUCKET"] ?? "nocdn-main"
        let existingIndex = connections.firstIndex {
            $0.provider == .backblazeB2 && $0.b2.bucketName == bucketName
        }
        var connection = existingIndex.map { connections[$0] } ?? CloudConnection(provider: .backblazeB2)
        connection.displayName = bucketName
        connection.isEnabled = true
        connection.b2 = B2ConnectionSettings(applicationKeyID: keyID, applicationKey: applicationKey, bucketName: bucketName)
        connection = connection.normalized()

        if let existingIndex {
            connections[existingIndex] = connection
        } else {
            connections.append(connection)
        }
        try store.save(connections)
        Diagnostics.shared.info("settings.seeded.b2", area: "settings", fields: ["bucket": bucketName])
    }

    static var preview: ConnectionListViewModel {
        let model = ConnectionListViewModel()
        model.connections = [
            CloudConnection(provider: .backblazeB2, displayName: "nocdn-main", b2: B2ConnectionSettings(bucketName: "nocdn-main")),
            CloudConnection(provider: .googleDrive, displayName: "Google Drive")
        ]
        return model
    }
}
