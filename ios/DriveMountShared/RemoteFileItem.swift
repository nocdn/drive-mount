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
}
