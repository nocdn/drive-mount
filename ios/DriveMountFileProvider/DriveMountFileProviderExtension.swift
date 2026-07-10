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
                let browser = makeBrowser()
                let remoteIdentifier = identifier == .rootContainer ? RemoteFileItem.rootID : identifier.rawValue
                let item = try await browser.item(for: remoteIdentifier)
                completionHandler(FileProviderItem(item: item), nil)
            } catch {
                completionHandler(nil, error)
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
                completionHandler(nil, nil, error)
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
        completionHandler(nil, fields, false, RemoteFileError.unsupported("Creating files is not implemented yet."))
        progress.completedUnitCount = 1
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
        completionHandler(nil, changedFields, false, RemoteFileError.unsupported("Modifying files is not implemented yet."))
        progress.completedUnitCount = 1
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
        completionHandler(RemoteFileError.unsupported("Deleting files is not implemented yet."))
        progress.completedUnitCount = 1
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
}
