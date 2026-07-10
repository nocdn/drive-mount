import FileProvider
import Foundation
import UniformTypeIdentifiers

final class DriveMountFileProviderExtension: NSObject, NSFileProviderReplicatedExtension {
    private let domain: NSFileProviderDomain

    required init(domain: NSFileProviderDomain) {
        self.domain = domain
        Diagnostics.shared.info("extension.initialized", area: "fileprovider", fields: ["domain": domain.identifier.rawValue])
    }

    func invalidate() {}

    func item(for identifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest, completionHandler: @escaping (NSFileProviderItem?, Error?) -> Void) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        Task {
            do {
                if identifier == .workingSet || identifier == .trashContainer {
                    completionHandler(
                        nil,
                        NSError(
                            domain: NSFileProviderErrorDomain,
                            code: NSFileProviderError.Code.noSuchItem.rawValue,
                            userInfo: [NSLocalizedDescriptionKey: "Container item is not materializable."]
                        )
                    )
                    progress.completedUnitCount = 1
                    return
                }
                let browser = makeBrowser()
                let remoteIdentifier = identifier == .rootContainer ? RemoteFileItem.rootID : identifier.rawValue
                let item = try await browser.item(for: remoteIdentifier)
                completionHandler(FileProviderItem(item: item), nil)
            } catch {
                completionHandler(nil, error.asFileProviderError)
            }
            progress.completedUnitCount = 1
        }
        return progress
    }

    func enumerator(for containerItemIdentifier: NSFileProviderItemIdentifier, request: NSFileProviderRequest) throws -> NSFileProviderEnumerator {
        FileProviderEnumerator(itemIdentifier: containerItemIdentifier, browser: makeBrowser())
    }

    func fetchContents(
        for itemIdentifier: NSFileProviderItemIdentifier,
        version requestedVersion: NSFileProviderItemVersion?,
        request: NSFileProviderRequest,
        completionHandler: @escaping (URL?, NSFileProviderItem?, Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let task = Task {
            do {
                let browser = makeBrowser()
                let item = try await browser.item(for: itemIdentifier.rawValue)
                Diagnostics.shared.info(
                    "fetch.started",
                    area: "fileprovider",
                    fields: [
                        "provider": item.key?.provider.rawValue ?? "unknown",
                        "expectedBytes": item.size.map(String.init) ?? "unknown"
                    ]
                )
                let url = try await browser.contents(of: itemIdentifier.rawValue)
                let actualSize = try downloadedFileSize(at: url)
                if let expectedSize = item.size, actualSize != expectedSize {
                    throw RemoteFileError.invalidResponse(
                        "Downloaded \(actualSize) bytes, but the provider reported \(expectedSize) bytes."
                    )
                }
                completionHandler(url, FileProviderItem(item: item), nil)
                Diagnostics.shared.info(
                    "fetch.finished",
                    area: "fileprovider",
                    fields: ["bytes": "\(actualSize)"]
                )
            } catch {
                Diagnostics.shared.error("fetch.failed", area: "fileprovider", error: error)
                completionHandler(nil, nil, error.asFileProviderError)
            }
            progress.completedUnitCount = 1
        }
        progress.cancellationHandler = {
            task.cancel()
        }
        return progress
    }

    func createItem(
        basedOn itemTemplate: NSFileProviderItem,
        fields: NSFileProviderItemFields,
        contents url: URL?,
        options: NSFileProviderCreateItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let task = Task {
            do {
                let browser = makeBrowser()
                let parentID = itemTemplate.parentItemIdentifier == .rootContainer
                    ? RemoteFileItem.rootID
                    : itemTemplate.parentItemIdentifier.rawValue
                let isDirectory = itemTemplate.contentType?.conforms(to: .folder) == true
                let created = try await browser.createItem(
                    name: itemTemplate.filename,
                    parentIdentifier: parentID,
                    isDirectory: isDirectory,
                    contentsURL: url,
                    contentType: itemTemplate.contentType?.preferredMIMEType
                )
                Diagnostics.shared.info(
                    "create.finished",
                    area: "fileprovider",
                    fields: [
                        "name": created.filename,
                        "directory": isDirectory ? "1" : "0",
                        "id": created.id,
                        "parent": parentID
                    ]
                )
                await signalDomainRefresh(around: parentID)
                completionHandler(FileProviderItem(item: created), [], false, nil)
            } catch {
                Diagnostics.shared.error(
                    "create.failed",
                    area: "fileprovider",
                    error: error,
                    fields: ["name": itemTemplate.filename]
                )
                completionHandler(nil, fields, false, error.asFileProviderError)
            }
            progress.completedUnitCount = 1
        }
        progress.cancellationHandler = { task.cancel() }
        return progress
    }

    func modifyItem(
        _ item: NSFileProviderItem,
        baseVersion version: NSFileProviderItemVersion,
        changedFields: NSFileProviderItemFields,
        contents newContents: URL?,
        options: NSFileProviderModifyItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (NSFileProviderItem?, NSFileProviderItemFields, Bool, Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let task = Task {
            do {
                let browser = makeBrowser()
                let identifier = item.itemIdentifier == .rootContainer
                    ? RemoteFileItem.rootID
                    : item.itemIdentifier.rawValue

                let newName = changedFields.contains(.filename) ? item.filename : nil
                let newParent: String?
                if changedFields.contains(.parentItemIdentifier) {
                    newParent = item.parentItemIdentifier == .rootContainer
                        ? RemoteFileItem.rootID
                        : item.parentItemIdentifier.rawValue
                } else {
                    newParent = nil
                }
                let contentsURL = changedFields.contains(.contents) ? newContents : nil

                Diagnostics.shared.info(
                    "modify.started",
                    area: "fileprovider",
                    fields: [
                        "id": identifier,
                        "filenameField": changedFields.contains(.filename) ? "1" : "0",
                        "parentField": changedFields.contains(.parentItemIdentifier) ? "1" : "0",
                        "contentsField": changedFields.contains(.contents) ? "1" : "0",
                        "newName": newName ?? "",
                        "newParent": newParent ?? ""
                    ]
                )

                let modified = try await browser.modifyItem(
                    identifier: identifier,
                    newName: newName,
                    newParentIdentifier: newParent,
                    contentsURL: contentsURL,
                    contentType: item.contentType?.preferredMIMEType
                )
                Diagnostics.shared.info(
                    "modify.finished",
                    area: "fileprovider",
                    fields: [
                        "name": modified.filename,
                        "id": modified.id,
                        "remote": modified.key?.extra["fileName"] ?? modified.key?.extra["prefix"] ?? ""
                    ]
                )
                await signalDomainRefresh(around: modified.parentID)
                if modified.parentID != identifier {
                    await signalDomainRefresh(around: identifier)
                }
                completionHandler(FileProviderItem(item: modified), [], false, nil)
            } catch {
                Diagnostics.shared.error(
                    "modify.failed",
                    area: "fileprovider",
                    error: error,
                    fields: ["id": item.itemIdentifier.rawValue, "name": item.filename]
                )
                completionHandler(nil, changedFields, false, error.asFileProviderError)
            }
            progress.completedUnitCount = 1
        }
        progress.cancellationHandler = { task.cancel() }
        return progress
    }

    func deleteItem(
        identifier: NSFileProviderItemIdentifier,
        baseVersion version: NSFileProviderItemVersion,
        options: NSFileProviderDeleteItemOptions = [],
        request: NSFileProviderRequest,
        completionHandler: @escaping (Error?) -> Void
    ) -> Progress {
        let progress = Progress(totalUnitCount: 1)
        let task = Task {
            do {
                let browser = makeBrowser()
                let remoteID = identifier == .rootContainer ? RemoteFileItem.rootID : identifier.rawValue
                Diagnostics.shared.info("delete.started", area: "fileprovider", fields: ["id": remoteID])
                try await browser.deleteItem(identifier: remoteID)
                Diagnostics.shared.info("delete.finished", area: "fileprovider", fields: ["id": remoteID, "hard": "1"])
                await signalDomainRefresh(around: remoteID)
                completionHandler(nil)
            } catch {
                Diagnostics.shared.error(
                    "delete.failed",
                    area: "fileprovider",
                    error: error,
                    fields: ["id": identifier.rawValue]
                )
                completionHandler(error.asFileProviderError)
            }
            progress.completedUnitCount = 1
        }
        progress.cancellationHandler = { task.cancel() }
        return progress
    }

    func importDidFinish(completionHandler: @escaping () -> Void) {
        completionHandler()
    }

    func materializedItemsDidChange(completionHandler: @escaping () -> Void) {
        completionHandler()
    }

    private func makeBrowser() -> any RemoteFileBrowsing {
        let connections = (try? ConnectionStore().load()) ?? []
        return RemoteFileBrowserFactory.browser(
            forDomainIdentifier: domain.identifier.rawValue,
            displayName: domain.displayName,
            connections: connections
        )
    }

    private func downloadedFileSize(at url: URL) throws -> Int64 {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        return (attributes[.size] as? NSNumber)?.int64Value ?? 0
    }

    /// Tell Files to re-enumerate and clear sticky domain error badges after mutations.
    private func signalDomainRefresh(around identifier: String) async {
        guard let manager = NSFileProviderManager(for: domain) else {
            return
        }
        do {
            try await manager.signalEnumerator(for: .workingSet)
            try await manager.signalEnumerator(for: .rootContainer)
            if identifier != RemoteFileItem.rootID {
                try await manager.signalEnumerator(for: NSFileProviderItemIdentifier(identifier))
            }
            // Resolve common sticky error codes that produce the circular ↻! badge.
            for code in [
                NSFileProviderError.Code.notAuthenticated,
                .serverUnreachable,
                .cannotSynchronize
            ] {
                let error = NSError(domain: NSFileProviderErrorDomain, code: code.rawValue)
                try? await manager.signalErrorResolved(error)
            }
        } catch {
            Diagnostics.shared.error(
                "domain.refresh.failed",
                area: "fileprovider",
                error: error,
                fields: ["around": identifier]
            )
        }
    }
}
