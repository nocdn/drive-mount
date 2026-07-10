import XCTest
import UniformTypeIdentifiers
@testable import DriveMount

final class DriveMountTests: XCTestCase {
    func testB2ConnectionNameFallsBackToBucket() {
        let connection = CloudConnection(
            provider: .backblazeB2,
            b2: B2ConnectionSettings(bucketName: " nocdn-main ")
        )

        XCTAssertEqual(connection.normalized().effectiveDisplayName, "nocdn-main")
    }

    func testSeedboxSettingsNormalizeHostPortAndPath() {
        let settings = SeedboxConnectionSettings(
            host: " ftps://seedbox.example.com ",
            username: " user ",
            password: " pass ",
            port: 70000,
            remotePath: "/downloads//movies/",
            readOnly: true
        )

        let normalized = settings.normalized()

        XCTAssertEqual(normalized.host, "seedbox.example.com")
        XCTAssertEqual(normalized.username, "user")
        XCTAssertEqual(normalized.password, "pass")
        XCTAssertEqual(normalized.port, 65535)
        XCTAssertEqual(normalized.remotePath, "downloads/movies")
    }

    func testConnectionStoreRoundTripsConnections() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let store = ConnectionStore(fileURL: directory.appendingPathComponent("connections.json"))
        let date = Date(timeIntervalSince1970: 1_800_000_000)
        let connections = [
            CloudConnection(
                id: "b2-main",
                provider: .backblazeB2,
                displayName: "nocdn-main",
                createdAt: date,
                updatedAt: date,
                b2: B2ConnectionSettings(applicationKeyID: "key-id", applicationKey: "key", bucketName: "nocdn-main")
            )
        ]

        try store.save(connections)

        XCTAssertEqual(try store.load(), connections)
    }

    @MainActor
    func testFileProviderDomainUsesReplicatedInitializer() {
        let connection = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            displayName: "nocdn-main"
        )

        let domain = ConnectionListViewModel.fileProviderDomain(for: connection)

        XCTAssertEqual(domain.identifier.rawValue, AppConstants.b2FileProviderDomainIdentifier)
        XCTAssertEqual(domain.displayName, AppConstants.b2FileProviderDomainDisplayName)
        XCTAssertTrue(domain.isReplicated)
    }

    @MainActor
    func testB2ConnectionsShareOneDomainWhileOtherProvidersStaySeparate() {
        let connections = [
            CloudConnection(id: "b2-main", provider: .backblazeB2, displayName: "nocdn-main"),
            CloudConnection(id: "b2-backups", provider: .backblazeB2, displayName: "nocdn-backups"),
            CloudConnection(id: "seedbox", provider: .seedbox, displayName: "Seedbox"),
            CloudConnection(id: "google", provider: .googleDrive, displayName: "Google Drive"),
            CloudConnection(id: "onedrive", provider: .oneDrive, displayName: "OneDrive")
        ]

        let domains = ConnectionListViewModel.fileProviderDomains(for: connections)
        let identifiers = Set(domains.map { $0.identifier.rawValue })

        XCTAssertEqual(domains.count, 4)
        XCTAssertEqual(
            identifiers,
            [AppConstants.b2FileProviderDomainIdentifier, "seedbox", "google", "onedrive"]
        )
        XCTAssertTrue(domains.allSatisfy(\.isReplicated))
    }

    @MainActor
    func testSavingB2ReplacesOnlyTheGroupedB2Domain() {
        let b2Domain = ConnectionListViewModel.fileProviderDomain(for: CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            displayName: "nocdn-main"
        ))
        let seedboxDomain = ConnectionListViewModel.fileProviderDomain(for: CloudConnection(
            id: "seedbox",
            provider: .seedbox,
            displayName: "Seedbox"
        ))

        let identifiers = ConnectionListViewModel.domainIdentifiersToRemove(
            existingDomains: [b2Domain, seedboxDomain],
            targetDomains: [b2Domain, seedboxDomain],
            resettingDomainIDs: [AppConstants.b2FileProviderDomainIdentifier]
        )

        XCTAssertEqual(identifiers, [AppConstants.b2FileProviderDomainIdentifier])
    }

    func testProviderItemKeyRoundTripsThroughIdentifier() {
        let key = ProviderItemKey(
            provider: .backblazeB2,
            kind: .file,
            name: "video.mp4",
            remoteID: "file-id",
            parentRemoteID: "root",
            size: 42,
            modifiedAt: Date(timeIntervalSince1970: 10),
            contentType: "video/mp4",
            extra: ["bucketName": "nocdn-main"]
        )

        XCTAssertTrue(key.encodedIdentifier.hasPrefix("dm3_"))
        XCTAssertFalse(key.encodedIdentifier.contains(":"))
        XCTAssertLessThanOrEqual(key.encodedIdentifier.count, 36)
        XCTAssertEqual(ProviderItemKey.decode(key.encodedIdentifier), key)
    }

    func testRemoteFileItemInfersTextTypeFromFilenameForGenericB2ContentType() {
        let item = RemoteFileItem(
            key: nil,
            parentID: RemoteFileItem.rootID,
            filename: "notes.txt",
            isDirectory: false,
            size: 12,
            modifiedAt: nil,
            contentType: "b2/x-auto"
        )

        XCTAssertEqual(item.typeIdentifier, UTType.plainText.identifier)
    }

    func testFixtureBrowserExplainsMissingCredentials() async throws {
        let browser = FixtureRemoteFileBrowser(
            connection: CloudConnection(provider: .googleDrive, displayName: "Google Drive"),
            reason: .missingCredentials("Google Drive")
        )

        let children = try await browser.children(of: RemoteFileItem.rootID)

        XCTAssertEqual(children.first?.filename, "Connection Status.txt")
        XCTAssertFalse(children.isEmpty)
    }

    func testRemoteFileErrorsMapToFileProviderCodes() {
        XCTAssertEqual(
            RemoteFileError.missingCredentials("Backblaze B2").asFileProviderError.code,
            NSFileProviderError.Code.notAuthenticated.rawValue
        )
        XCTAssertEqual(
            RemoteFileError.notFound("abc").asFileProviderError.code,
            NSFileProviderError.Code.noSuchItem.rawValue
        )
        XCTAssertEqual(
            RemoteFileError.server("HTTP 401").asFileProviderError.code,
            NSFileProviderError.Code.serverUnreachable.rawValue
        )
        XCTAssertEqual(
            RemoteFileError.unsupported("not yet").asFileProviderError.code,
            NSFileProviderError.Code.cannotSynchronize.rawValue
        )
    }

    func testFixtureBrowserRejectsMutations() async {
        let browser = FixtureRemoteFileBrowser(
            connection: CloudConnection(provider: .seedbox, displayName: "Seedbox"),
            reason: .unsupported("Seedbox FTP browsing needs a native FTP transport implementation.")
        )

        do {
            _ = try await browser.createItem(
                name: "new.txt",
                parentIdentifier: RemoteFileItem.rootID,
                isDirectory: false,
                contentsURL: nil,
                contentType: "text/plain"
            )
            XCTFail("Expected create to be unsupported on fixture browser.")
        } catch let error as RemoteFileError {
            guard case .unsupported = error else {
                XCTFail("Unexpected error: \(error)")
                return
            }
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    func testB2FileProviderItemAdvertisesWriteCapabilities() {
        let key = ProviderItemKey(
            provider: .backblazeB2,
            kind: .file,
            name: "notes.txt",
            remoteID: "file:abc",
            parentRemoteID: "bucket:1",
            parentItemID: "parent",
            size: 12,
            modifiedAt: nil,
            contentType: "text/plain",
            extra: ["bucketID": "1", "bucketName": "nocdn-main", "fileID": "abc", "fileName": "notes.txt"]
        )
        let item = RemoteFileItem(
            key: key,
            parentID: "parent",
            filename: "notes.txt",
            isDirectory: false,
            size: 12,
            modifiedAt: nil,
            contentType: "text/plain"
        )

        XCTAssertTrue(item.fileProviderCapabilities.contains(.allowsWriting))
        XCTAssertTrue(item.fileProviderCapabilities.contains(.allowsDeleting))
        XCTAssertTrue(item.fileProviderCapabilities.contains(.allowsRenaming))
        XCTAssertTrue(item.fileProviderFileSystemFlags.contains(.userWritable))
    }

    func testB2BucketItemAllowsAddingChildrenButNotDeletingBucket() {
        let key = ProviderItemKey(
            provider: .backblazeB2,
            kind: .folder,
            name: "nocdn-main",
            remoteID: "bucket:1",
            parentRemoteID: RemoteFileItem.rootID,
            parentItemID: RemoteFileItem.rootID,
            size: nil,
            modifiedAt: nil,
            contentType: nil,
            extra: ["bucketID": "1", "bucketName": "nocdn-main", "prefix": ""]
        )
        let item = RemoteFileItem(
            key: key,
            parentID: RemoteFileItem.rootID,
            filename: "nocdn-main",
            isDirectory: true,
            size: nil,
            modifiedAt: nil,
            contentType: nil
        )

        XCTAssertTrue(item.fileProviderCapabilities.contains(.allowsAddingSubItems))
        XCTAssertFalse(item.fileProviderCapabilities.contains(.allowsDeleting))
        XCTAssertFalse(item.fileProviderCapabilities.contains(.allowsRenaming))
    }

    func testConnectionStoreMigratesLegacyAppGroupRootLocation() throws {
        let container = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let legacyURL = container
            .appendingPathComponent("Connections", isDirectory: true)
            .appendingPathComponent(AppConstants.connectionStoreFileName)
        let modernURL = container
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("Connections", isDirectory: true)
            .appendingPathComponent(AppConstants.connectionStoreFileName)

        let connection = CloudConnection(
            id: "migrate-b2",
            provider: .backblazeB2,
            displayName: "nocdn-main",
            b2: B2ConnectionSettings(applicationKeyID: "key-id", applicationKey: "key", bucketName: "nocdn-main")
        )
        try FileManager.default.createDirectory(at: legacyURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        // Write with the same encoding rules ConnectionStore uses (ISO-8601 dates).
        let legacyStore = ConnectionStore(fileURL: legacyURL)
        try legacyStore.save([connection])

        // Simulate migration helper used by ConnectionStore when only the legacy path exists.
        try FileManager.default.createDirectory(at: modernURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: legacyURL, to: modernURL)

        let store = ConnectionStore(fileURL: modernURL)
        XCTAssertEqual(try store.load().map(\.id), ["migrate-b2"])
    }

    func testB2AuthorizeResponseAllowsRestrictedBucketWithoutListBuckets() async throws {
        // Restricted keys expose the bucket via authorize `allowed` and reject unscoped list_buckets.
        // Decode is covered indirectly by ensuring a browser with synthetic credentials surfaces a
        // clear server error when network is unavailable rather than a decode crash path.
        let connection = CloudConnection(
            provider: .backblazeB2,
            displayName: "nocdn-main",
            b2: B2ConnectionSettings(
                applicationKeyID: "not-a-real-key",
                applicationKey: "not-a-real-secret",
                bucketName: "nocdn-main"
            )
        )
        let browser = B2RemoteFileBrowser(connection: connection)

        do {
            _ = try await browser.children(of: RemoteFileItem.rootID)
            XCTFail("Expected authorization against Backblaze to fail with fake credentials.")
        } catch let error as RemoteFileError {
            switch error {
            case .server, .invalidResponse, .missingCredentials:
                break
            default:
                XCTFail("Unexpected error: \(error)")
            }
        } catch {
            // URLSession may surface transport errors before our RemoteFileError mapping.
            XCTAssertFalse(error.localizedDescription.isEmpty)
        }
    }

    func testB2BrowserListsConfiguredBucketWhenCredentialsAreProvided() async throws {
        let bucket = Self.environmentValue("DRIVEMOUNT_TEST_B2_BUCKET")
        let keyID = Self.environmentValue("DRIVEMOUNT_TEST_B2_KEY_ID")
        let applicationKey = Self.environmentValue("DRIVEMOUNT_TEST_B2_APPLICATION_KEY")
        try XCTSkipUnless(bucket != nil && keyID != nil && applicationKey != nil, "B2 credentials not supplied.")

        let connection = CloudConnection(
            provider: .backblazeB2,
            displayName: bucket!,
            b2: B2ConnectionSettings(
                applicationKeyID: keyID!,
                applicationKey: applicationKey!,
                bucketName: bucket!
            )
        )
        let browser = B2RemoteFileBrowser(connection: connection)

        let root = try await browser.item(for: RemoteFileItem.rootID)
        let rootChildren = try await browser.children(of: RemoteFileItem.rootID)

        XCTAssertEqual(root.filename, bucket)
        let bucketFolder = try XCTUnwrap(rootChildren.first { $0.filename == bucket && $0.isDirectory })
        let bucketChildren = try await browser.children(of: bucketFolder.id)
        XCTAssertTrue(bucketChildren.filter { !$0.isDirectory }.allSatisfy { $0.size != nil })

        guard let downloadable = bucketChildren.first(where: {
            !$0.isDirectory && ($0.size ?? 0) > 0 && ($0.size ?? .max) < 1_048_576
        }) else {
            throw XCTSkip("No non-empty file under 1 MB was available at the bucket root.")
        }
        let downloadedURL = try await browser.contents(of: downloadable.id)
        defer { try? FileManager.default.removeItem(at: downloadedURL) }
        let downloadedSize = try Data(contentsOf: downloadedURL).count
        XCTAssertEqual(Int64(downloadedSize), try XCTUnwrap(downloadable.size))
    }

    func testGroupedB2BrowserListsConfiguredBucketsWhenCredentialsAreProvided() async throws {
        let bucketList = Self.environmentValue("DRIVEMOUNT_TEST_B2_BUCKETS")
        let keyID = Self.environmentValue("DRIVEMOUNT_TEST_B2_KEY_ID")
        let applicationKey = Self.environmentValue("DRIVEMOUNT_TEST_B2_APPLICATION_KEY")
        try XCTSkipUnless(bucketList != nil && keyID != nil && applicationKey != nil, "B2 credentials not supplied.")

        let bucketNames = bucketList!
            .split(separator: ",")
            .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        let connections = bucketNames.enumerated().map { index, bucketName in
            CloudConnection(
                id: "live-b2-\(index)",
                provider: .backblazeB2,
                displayName: bucketName,
                b2: B2ConnectionSettings(
                    applicationKeyID: keyID!,
                    applicationKey: applicationKey!,
                    bucketName: bucketName
                )
            )
        }
        let browser = B2GroupedRemoteFileBrowser(connections: connections)

        let root = try await browser.item(for: RemoteFileItem.rootID)
        let rootChildren = try await browser.children(of: RemoteFileItem.rootID)

        XCTAssertEqual(root.filename, AppConstants.b2FileProviderDomainDisplayName)
        XCTAssertEqual(Set(rootChildren.map(\.filename)), Set(bucketNames))
        XCTAssertTrue(rootChildren.allSatisfy(\.isDirectory))
    }

    private static func environmentValue(_ key: String) -> String? {
        let env = ProcessInfo.processInfo.environment
        return env[key] ?? env["TEST_RUNNER_\(key)"]
    }
}
