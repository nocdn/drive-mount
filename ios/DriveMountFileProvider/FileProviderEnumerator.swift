import FileProvider
import Foundation

final class FileProviderEnumerator: NSObject, NSFileProviderEnumerator {
    private let itemIdentifier: NSFileProviderItemIdentifier
    private let browser: any RemoteFileBrowsing

    init(itemIdentifier: NSFileProviderItemIdentifier, browser: any RemoteFileBrowsing) {
        self.itemIdentifier = itemIdentifier
        self.browser = browser
    }

    func invalidate() {}

    func enumerateItems(for observer: NSFileProviderEnumerationObserver, startingAt page: NSFileProviderPage) {
        // Working set / trash are not remote listings. Returning empty avoids domain errors
        // when Files probes these containers (which previously contributed to the badge).
        if itemIdentifier == .workingSet || itemIdentifier == .trashContainer {
            observer.finishEnumerating(upTo: nil)
            Diagnostics.shared.info(
                "enumeration.finished",
                area: "fileprovider",
                fields: [
                    "container": itemIdentifier == .workingSet ? "workingSet" : "trash",
                    "count": "0"
                ]
            )
            return
        }

        Task {
            do {
                let identifier = itemIdentifier == .rootContainer ? RemoteFileItem.rootID : itemIdentifier.rawValue
                let children = try await browser.children(of: identifier)
                observer.didEnumerate(children.map(FileProviderItem.init(item:)))
                observer.finishEnumerating(upTo: nil)
                Diagnostics.shared.info(
                    "enumeration.finished",
                    area: "fileprovider",
                    fields: [
                        "container": identifier == RemoteFileItem.rootID ? "root" : "folder",
                        "count": "\(children.count)"
                    ]
                )
            } catch {
                Diagnostics.shared.error("enumeration.failed", area: "fileprovider", error: error)
                observer.finishEnumeratingWithError(error.asFileProviderError)
            }
        }
    }

    func enumerateChanges(for observer: NSFileProviderChangeObserver, from syncAnchor: NSFileProviderSyncAnchor) {
        // No incremental change feed yet — report an empty page so Files does not treat
        // the domain as permanently failed.
        observer.finishEnumeratingChanges(upTo: syncAnchor, moreComing: false)
    }

    func currentSyncAnchor(completionHandler: @escaping (NSFileProviderSyncAnchor?) -> Void) {
        completionHandler(NSFileProviderSyncAnchor(Data("0".utf8)))
    }
}
