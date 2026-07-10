import FileProvider
import Foundation
import UniformTypeIdentifiers

struct RemoteFileItem: Equatable, Sendable {
    static let rootID = "root"

    var key: ProviderItemKey?
    var parentID: String
    var filename: String
    var isDirectory: Bool
    var size: Int64?
    var modifiedAt: Date?
    var contentType: String?

    var id: String {
        key?.encodedIdentifier ?? Self.rootID
    }

    var typeIdentifier: String {
        if isDirectory {
            return UTType.folder.identifier
        }

        let mimeType = contentType
            .map { $0.split(separator: ";", maxSplits: 1).first.map(String.init) ?? $0 }
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() }
        let genericMIMETypes = ["application/octet-stream", "binary/octet-stream", "b2/x-auto"]

        if let mimeType, !genericMIMETypes.contains(mimeType), let type = UTType(mimeType: mimeType) {
            return type.identifier
        }
        let pathExtension = (filename as NSString).pathExtension
        if !pathExtension.isEmpty, let type = UTType(filenameExtension: pathExtension) {
            return type.identifier
        }
        if let mimeType, let type = UTType(mimeType: mimeType) {
            return type.identifier
        }
        return UTType.data.identifier
    }

    static func root(displayName: String) -> RemoteFileItem {
        RemoteFileItem(
            key: nil,
            parentID: "",
            filename: displayName,
            isDirectory: true,
            size: nil,
            modifiedAt: nil,
            contentType: nil
        )
    }

    /// Capabilities advertised to Files. Only B2 currently supports mutations.
    var fileProviderCapabilities: NSFileProviderItemCapabilities {
        guard key?.provider == .backblazeB2 else {
            if isDirectory {
                return [.allowsReading, .allowsContentEnumerating]
            }
            return [.allowsReading]
        }

        if isDirectory {
            if key?.remoteID.hasPrefix("bucket:") == true {
                return [.allowsReading, .allowsContentEnumerating, .allowsAddingSubItems]
            }
            return [
                .allowsReading,
                .allowsContentEnumerating,
                .allowsAddingSubItems,
                .allowsRenaming,
                .allowsReparenting,
                .allowsDeleting
            ]
        }

        return [
            .allowsReading,
            .allowsWriting,
            .allowsRenaming,
            .allowsReparenting,
            .allowsDeleting
        ]
    }

    var fileProviderFileSystemFlags: NSFileProviderFileSystemFlags {
        let capabilities = fileProviderCapabilities
        let writable = capabilities.contains(.allowsWriting)
            || capabilities.contains(.allowsAddingSubItems)
            || capabilities.contains(.allowsDeleting)
            || capabilities.contains(.allowsRenaming)

        if isDirectory {
            var flags: NSFileProviderFileSystemFlags = [.userReadable, .userExecutable]
            if writable {
                flags.insert(.userWritable)
            }
            return flags
        }

        var flags: NSFileProviderFileSystemFlags = [.userReadable]
        if writable {
            flags.insert(.userWritable)
        }
        return flags
    }
}
