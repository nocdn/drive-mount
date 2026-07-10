import FileProvider
import Foundation
import UniformTypeIdentifiers

final class FileProviderItem: NSObject, NSFileProviderItem {
    private let item: RemoteFileItem

    init(item: RemoteFileItem) {
        self.item = item
    }

    var itemIdentifier: NSFileProviderItemIdentifier {
        if item.id == RemoteFileItem.rootID {
            return .rootContainer
        }
        return NSFileProviderItemIdentifier(item.id)
    }

    var parentItemIdentifier: NSFileProviderItemIdentifier {
        if item.parentID.isEmpty {
            .rootContainer
        } else if item.parentID == RemoteFileItem.rootID {
            .rootContainer
        } else {
            NSFileProviderItemIdentifier(item.parentID)
        }
    }

    var filename: String {
        item.filename
    }

    var contentType: UTType {
        UTType(item.typeIdentifier) ?? (item.isDirectory ? .folder : .data)
    }

    var documentSize: NSNumber? {
        item.size.map { NSNumber(value: $0) }
    }

    var contentModificationDate: Date? {
        item.modifiedAt
    }

    var itemVersion: NSFileProviderItemVersion {
        NSFileProviderItemVersion(
            contentVersion: versionComponent(seed: "\(item.id)|content|\(item.size ?? -1)|\(item.modifiedAt?.timeIntervalSince1970 ?? 0)"),
            metadataVersion: versionComponent(seed: "\(item.id)|metadata|\(item.parentID)|\(item.filename)|\(item.typeIdentifier)")
        )
    }

    var capabilities: NSFileProviderItemCapabilities {
        if item.isDirectory {
            return [.allowsReading, .allowsWriting, .allowsContentEnumerating]
        }
        return [.allowsReading, .allowsWriting]
    }

    var fileSystemFlags: NSFileProviderFileSystemFlags {
        if item.isDirectory {
            return [.userReadable, .userWritable, .userExecutable]
        }
        return [.userReadable, .userWritable]
    }

    private func versionComponent(seed: String) -> Data {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in seed.utf8 {
            hash ^= UInt64(byte)
            hash &*= 0x100000001b3
        }
        return Data(String(hash, radix: 16).utf8)
    }
}
