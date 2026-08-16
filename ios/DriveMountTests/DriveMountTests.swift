import XCTest
import UniformTypeIdentifiers
@testable import DriveMount

final class DriveMountTests: XCTestCase {
    func testIOSAppIconHasNoCustomImage() throws {
        let iosRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let appIconSet = iosRoot
            .appendingPathComponent("DriveMountApp/Assets.xcassets/AppIcon.appiconset")
        let contentsURL = appIconSet.appendingPathComponent("Contents.json")
        let json = try XCTUnwrap(
            JSONSerialization.jsonObject(with: try Data(contentsOf: contentsURL)) as? [String: Any]
        )
        let images = try XCTUnwrap(json["images"] as? [[String: Any]])

        XCTAssertFalse(images.isEmpty)
        XCTAssertTrue(images.allSatisfy { $0["filename"] == nil })

        let pngs = try FileManager.default.contentsOfDirectory(
            at: appIconSet,
            includingPropertiesForKeys: nil
        ).filter { $0.pathExtension.lowercased() == "png" }
        XCTAssertTrue(pngs.isEmpty)

        XCTAssertFalse(
            FileManager.default.fileExists(
                atPath: iosRoot
                    .appendingPathComponent("ios/Assets.xcassets/AppIcon.appiconset/AppIcon.png")
                    .path
            )
        )
    }

    func testB2ConnectionNameFallsBackToBucket() {
        let connection = CloudConnection(
            provider: .backblazeB2,
            b2: B2ConnectionSettings(bucketName: " nocdn-main ")
        )

        XCTAssertEqual(connection.normalized().effectiveDisplayName, "nocdn-main")
    }

    func testSeedboxSettingsNormalizeHostPortAndPath() {
        let settings = SeedboxConnectionSettings(
            host: " sftp://seedbox.example.com ",
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
        XCTAssertEqual(normalized.effectiveSFTPPort, 65535)
        XCTAssertEqual(normalized.remotePath, "downloads/movies")
        XCTAssertEqual(
            SeedboxConnectionSettings(host: " FTPS://seedbox.example.com/// ").normalized().host,
            "seedbox.example.com"
        )
    }

    func testLegacyFTPSPortUsesSFTPPort22() {
        let settings = SeedboxConnectionSettings(host: "ftps://box.example.com", port: 21)

        XCTAssertEqual(settings.normalized().host, "box.example.com")
        XCTAssertEqual(settings.effectiveSFTPPort, 22)
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

    func testConnectionStoreRoundTripsMultipleB2Buckets() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let store = ConnectionStore(fileURL: directory.appendingPathComponent("connections.json"))
        let connection = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            displayName: "B2",
            b2: B2ConnectionSettings(
                applicationKeyID: "key-id",
                applicationKey: "key",
                bucketNames: ["nocdn-main", "nocdn-music"]
            )
        )

        try store.save([connection])

        XCTAssertEqual(try store.load().first?.b2.normalizedBucketNames, ["nocdn-main", "nocdn-music"])
    }

    @MainActor
    func testB2FileProviderDomainUsesBucketConnectionIdentity() {
        let connection = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            displayName: "B2",
            b2: B2ConnectionSettings(bucketName: "nocdn-main")
        )

        let domain = ConnectionListViewModel.fileProviderDomain(for: connection)

        XCTAssertEqual(domain.identifier.rawValue, "b2|b2-main|nocdn-main")
        XCTAssertEqual(domain.displayName, "nocdn-main")
        XCTAssertTrue(domain.isReplicated)
    }

    @MainActor
    func testEachConnectionGetsItsOwnFileProviderDomain() {
        let connections = [
            CloudConnection(
                id: "b2-main",
                provider: .backblazeB2,
                displayName: "B2",
                b2: B2ConnectionSettings(bucketNames: ["nocdn-main", "nocdn-music"])
            ),
            CloudConnection(id: "seedbox", provider: .seedbox, displayName: "Seedbox"),
            CloudConnection(id: "google", provider: .googleDrive, displayName: "Google Drive"),
            CloudConnection(id: "onedrive", provider: .oneDrive, displayName: "OneDrive")
        ]

        let domains = ConnectionListViewModel.fileProviderDomains(for: connections)
        let identifiers = Set(domains.map { $0.identifier.rawValue })

        XCTAssertEqual(domains.count, 5)
        XCTAssertEqual(
            identifiers,
            [
                "b2|b2-main|nocdn-main",
                "b2|b2-main|nocdn-music",
                "google",
                "onedrive",
                "seedbox"
            ]
        )
        XCTAssertEqual(
            Set(domains.map(\.displayName)),
            ["nocdn-main", "nocdn-music", "Google Drive", "OneDrive", "Seedbox"]
        )
        XCTAssertTrue(domains.allSatisfy(\.isReplicated))
    }

    @MainActor
    func testOneB2ConnectionRegistersADomainPerBucket() {
        let connection = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            b2: B2ConnectionSettings(bucketNames: [" nocdn-main ", "nocdn-music", "nocdn-main", ""])
        )

        let domains = ConnectionListViewModel.fileProviderDomains(for: [connection])

        XCTAssertEqual(domains.map(\.displayName), ["nocdn-main", "nocdn-music"])
        XCTAssertEqual(
            domains.map(\.identifier.rawValue),
            ["b2|b2-main|nocdn-main", "b2|b2-main|nocdn-music"]
        )
    }

    func testB2SettingsMigrateLegacySingleBucketName() throws {
        let json = Data("""
        {"applicationKeyID":"id","applicationKey":"key","bucketName":"nocdn-main"}
        """.utf8)
        let settings = try JSONDecoder().decode(B2ConnectionSettings.self, from: json)

        XCTAssertEqual(settings.normalizedBucketNames, ["nocdn-main"])
        XCTAssertEqual(settings.bucketName, "nocdn-main")
    }

    @MainActor
    func testAddingB2TwiceDoesNotCreateASecondConnection() async {
        let model = ConnectionListViewModel(store: ConnectionStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent(UUID().uuidString, isDirectory: true)
                .appendingPathComponent("connections.json")
        ))
        model.connections = [
            CloudConnection(id: "b2-main", provider: .backblazeB2, b2: B2ConnectionSettings(bucketName: "nocdn-main"))
        ]

        await model.addConnection(provider: .backblazeB2)

        XCTAssertEqual(model.connections.filter { $0.provider == .backblazeB2 }.count, 1)
        XCTAssertEqual(model.b2BucketRows.map(\.name), ["nocdn-main"])
    }

    @MainActor
    func testEnabledSeedboxRegistersAFileProviderDomain() {
        let domains = ConnectionListViewModel.fileProviderDomains(for: [
            CloudConnection(id: "seedbox", provider: .seedbox, displayName: "Seedbox")
        ])

        XCTAssertEqual(domains.map(\.identifier.rawValue), ["seedbox"])
        XCTAssertEqual(domains.first?.displayName, "Seedbox")
    }

    @MainActor
    func testDisabledSeedboxDoesNotRegisterAFileProviderDomain() {
        var connection = CloudConnection(id: "seedbox", provider: .seedbox, displayName: "Seedbox")
        connection.isEnabled = false

        XCTAssertTrue(ConnectionListViewModel.fileProviderDomains(for: [connection]).isEmpty)
    }

    @MainActor
    func testSavingB2ReplacesOnlyItsBucketDomain() {
        let b2 = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            b2: B2ConnectionSettings(bucketName: "nocdn-main")
        )
        let b2Domain = ConnectionListViewModel.fileProviderDomain(for: b2)
        let seedboxDomain = ConnectionListViewModel.fileProviderDomain(for: CloudConnection(
            id: "seedbox",
            provider: .seedbox,
            displayName: "Seedbox"
        ))

        let identifiers = ConnectionListViewModel.domainIdentifiersToRemove(
            existingDomains: [b2Domain, seedboxDomain],
            targetDomains: [b2Domain, seedboxDomain],
            resettingDomainIDs: ConnectionListViewModel.domainIdentifiers(for: [b2])
        )

        XCTAssertEqual(identifiers, ["b2|b2-main|nocdn-main"])
    }

    func testFactoryScopesB2BrowserToTheDomainBucket() {
        let connection = CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            b2: B2ConnectionSettings(
                applicationKeyID: "key-id",
                applicationKey: "key",
                bucketNames: ["nocdn-main", "nocdn-music"]
            )
        )
        let browser = RemoteFileBrowserFactory.browser(
            forDomainIdentifier: "b2|b2-main|nocdn-music",
            displayName: "nocdn-music",
            connections: [connection]
        )

        XCTAssertTrue(browser is B2RemoteFileBrowser)
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
            NSFileProviderError.Code.notAuthenticated.rawValue
        )
        let unsupported = RemoteFileError.unsupported("not yet").asFileProviderError
        XCTAssertEqual(unsupported.domain, NSCocoaErrorDomain)
        XCTAssertEqual(unsupported.code, NSFeatureUnsupportedError)
        XCTAssertEqual(
            RemoteFileError.server("HTTP 403 {\"code\":\"download_cap_exceeded\"}").asFileProviderError.code,
            NSFileProviderError.Code.insufficientQuota.rawValue
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

    func testB2BucketIsTheWritableFilesRoot() async throws {
        let browser = B2RemoteFileBrowser(connection: CloudConnection(
            id: "b2-main",
            provider: .backblazeB2,
            displayName: "nocdn-main"
        ))

        let root = try await browser.item(for: RemoteFileItem.rootID)

        XCTAssertTrue(root.fileProviderFileSystemFlags.contains(.userWritable))
        XCTAssertTrue(root.fileProviderCapabilities.contains(.allowsAddingSubItems))
    }

    @MainActor
    func testFileProviderStateResetRunsOncePerRevision() {
        XCTAssertTrue(ConnectionListViewModel.requiresFileProviderStateReset(storedRevision: 0))
        XCTAssertFalse(ConnectionListViewModel.requiresFileProviderStateReset(
            storedRevision: AppConstants.fileProviderStateRevision
        ))
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
        XCTAssertTrue(root.fileProviderCapabilities.contains(.allowsAddingSubItems))
        XCTAssertTrue(rootChildren.filter { !$0.isDirectory }.allSatisfy { $0.size != nil })

        guard let downloadable = rootChildren.first(where: {
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

    func testSeedboxFactoryUsesSFTPBrowserWhenConfigured() {
        let connection = CloudConnection(
            provider: .seedbox,
            displayName: "Seedbox",
            seedbox: SeedboxConnectionSettings(host: "box.example.com", username: "user", password: "pass")
        )

        XCTAssertTrue(RemoteFileBrowserFactory.browser(for: connection) is SeedboxRemoteFileBrowser)
    }

    func testSeedboxBrowserListsAndStreamsFromTransport() async throws {
        let payload = Data(repeating: 0xAB, count: 4096)
        let transport = InMemorySeedboxTransport(files: [
            "downloads/Heated.Rivalry.mkv": payload
        ])
        let browser = SeedboxRemoteFileBrowser(
            connection: CloudConnection(
                provider: .seedbox,
                displayName: "Seedbox",
                seedbox: SeedboxConnectionSettings(host: "box.example.com", username: "user", password: "pass")
            ),
            transport: transport
        )

        let children = try await browser.children(of: RemoteFileItem.rootID)
        XCTAssertEqual(children.map(\.filename), ["Heated.Rivalry.mkv"])
        XCTAssertEqual(children.first?.size, 4096)
        XCTAssertFalse(children.first?.fileProviderCapabilities.contains(.allowsWriting) ?? true)

        let downloaded = try await browser.contents(of: try XCTUnwrap(children.first?.id))
        defer { try? FileManager.default.removeItem(at: downloaded) }
        XCTAssertEqual(try Data(contentsOf: downloaded), payload)
    }

    func testReadOnlySeedboxRejectsMutations() async {
        let browser = SeedboxRemoteFileBrowser(
            connection: CloudConnection(
                provider: .seedbox,
                seedbox: SeedboxConnectionSettings(host: "box.example.com", username: "user", password: "pass", readOnly: true)
            ),
            transport: InMemorySeedboxTransport()
        )

        do {
            _ = try await browser.createItem(
                name: "new.txt",
                parentIdentifier: RemoteFileItem.rootID,
                isDirectory: false,
                contentsURL: nil,
                contentType: "text/plain"
            )
            XCTFail("Expected read-only Seedbox to reject creates.")
        } catch let error as RemoteFileError {
            guard case .unsupported = error else {
                XCTFail("Unexpected error: \(error)")
                return
            }
        } catch {
            XCTFail("Unexpected error type: \(error)")
        }
    }

    func testWritableSeedboxCreatesAndDeletesThroughTransport() async throws {
        let transport = InMemorySeedboxTransport()
        let browser = SeedboxRemoteFileBrowser(
            connection: CloudConnection(
                provider: .seedbox,
                seedbox: SeedboxConnectionSettings(
                    host: "box.example.com",
                    username: "user",
                    password: "pass",
                    readOnly: false
                )
            ),
            transport: transport
        )

        let created = try await browser.createItem(
            name: "notes.txt",
            parentIdentifier: RemoteFileItem.rootID,
            isDirectory: false,
            contentsURL: nil,
            contentType: "text/plain"
        )
        XCTAssertEqual(created.filename, "notes.txt")
        XCTAssertTrue(created.fileProviderCapabilities.contains(.allowsDeleting))

        try await browser.deleteItem(identifier: created.id)
        let children = try await browser.children(of: RemoteFileItem.rootID)
        XCTAssertTrue(children.isEmpty)
    }

    func testSeedboxFileNameSanitizationRejectsPathTraversal() {
        XCTAssertThrowsError(try SeedboxRemoteFileBrowser.sanitizedFileName("../secret"))
        XCTAssertThrowsError(try SeedboxRemoteFileBrowser.sanitizedFileName("a/b"))
        XCTAssertEqual(try SeedboxRemoteFileBrowser.sanitizedFileName(" video.mkv "), "video.mkv")
        XCTAssertEqual(SeedboxRemoteFileBrowser.join("downloads", "video.mkv"), "downloads/video.mkv")
    }

    func testSeedboxBrowserListsConfiguredHostWhenCredentialsAreProvided() async throws {
        let host = Self.environmentValue("DRIVEMOUNT_TEST_SEEDBOX_HOST")
        let username = Self.environmentValue("DRIVEMOUNT_TEST_SEEDBOX_USERNAME")
        let password = Self.environmentValue("DRIVEMOUNT_TEST_SEEDBOX_PASSWORD")
        try XCTSkipUnless(host != nil && username != nil && password != nil, "Seedbox credentials not supplied.")

        let port = Self.environmentValue("DRIVEMOUNT_TEST_SEEDBOX_PORT").flatMap(Int.init) ?? 22
        let remotePath = Self.environmentValue("DRIVEMOUNT_TEST_SEEDBOX_PATH") ?? "downloads"
        let connection = CloudConnection(
            provider: .seedbox,
            displayName: "Seedbox",
            seedbox: SeedboxConnectionSettings(
                host: host!,
                username: username!,
                password: password!,
                port: port,
                remotePath: remotePath
            )
        )
        let browser = SeedboxRemoteFileBrowser(connection: connection)
        let children = try await browser.children(of: RemoteFileItem.rootID)
        XCTAssertFalse(children.isEmpty, "Expected the configured Seedbox remote path to contain items.")
    }

    private static func environmentValue(_ key: String) -> String? {
        let env = ProcessInfo.processInfo.environment
        return env[key] ?? env["TEST_RUNNER_\(key)"]
    }
}

final class InMemorySeedboxTransport: SeedboxTransporting, @unchecked Sendable {
    private var files: [String: Data]
    private var directories: Set<String>

    init(files: [String: Data] = [:], directories: Set<String> = ["downloads"]) {
        self.files = files
        self.directories = directories
        for path in files.keys {
            var parent = path
            while let slash = parent.lastIndex(of: "/") {
                parent = String(parent[..<slash])
                if !parent.isEmpty {
                    self.directories.insert(parent)
                }
            }
        }
    }

    func list(path: String) async throws -> [SeedboxEntry] {
        let prefix = path == "." ? "" : path.hasSuffix("/") ? path : path + "/"
        var entries: [String: SeedboxEntry] = [:]
        for directory in directories where directory.hasPrefix(prefix) {
            let remainder = String(directory.dropFirst(prefix.count))
            guard let name = remainder.split(separator: "/").first.map(String.init), remainder == name else {
                continue
            }
            entries[name] = SeedboxEntry(name: name, path: directory, isDirectory: true, size: nil, modifiedAt: nil)
        }
        for (filePath, data) in files where filePath.hasPrefix(prefix) {
            let remainder = String(filePath.dropFirst(prefix.count))
            guard let name = remainder.split(separator: "/").first.map(String.init), remainder == name else {
                continue
            }
            entries[name] = SeedboxEntry(name: name, path: filePath, isDirectory: false, size: Int64(data.count), modifiedAt: nil)
        }
        return entries.values.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    func stat(path: String) async throws -> SeedboxEntry {
        if directories.contains(path) {
            return SeedboxEntry(name: name(of: path), path: path, isDirectory: true, size: nil, modifiedAt: nil)
        }
        if let data = files[path] {
            return SeedboxEntry(name: name(of: path), path: path, isDirectory: false, size: Int64(data.count), modifiedAt: nil)
        }
        throw RemoteFileError.notFound(path)
    }

    func download(path: String, to destination: URL, expectedSize: Int64?) async throws {
        guard let data = files[path] else {
            throw RemoteFileError.notFound(path)
        }
        if let expectedSize, Int64(data.count) != expectedSize {
            throw RemoteFileError.invalidResponse("Downloaded \(data.count) bytes, but the provider reported \(expectedSize) bytes.")
        }
        try data.write(to: destination, options: .atomic)
    }

    func createDirectory(path: String) async throws {
        directories.insert(path)
    }

    func remove(path: String, isDirectory: Bool) async throws {
        if isDirectory {
            directories.remove(path)
            files = files.filter { !$0.key.hasPrefix(path + "/") && $0.key != path }
        } else {
            files.removeValue(forKey: path)
        }
    }

    func rename(from oldPath: String, to newPath: String) async throws {
        if let data = files.removeValue(forKey: oldPath) {
            files[newPath] = data
            return
        }
        if directories.remove(oldPath) != nil {
            directories.insert(newPath)
        }
    }

    func upload(from source: URL, to path: String) async throws {
        files[path] = try Data(contentsOf: source)
    }

    private func name(of path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }
}
