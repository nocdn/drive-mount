import Foundation

struct ProviderItemKey: Codable, Equatable, Sendable {
    enum Kind: String, Codable, Sendable {
        case folder
        case file
    }

    var provider: CloudProvider
    var kind: Kind
    var name: String
    var remoteID: String
    var parentRemoteID: String
    var parentItemID: String? = nil
    var size: Int64?
    var modifiedAt: Date?
    var contentType: String?
    var extra: [String: String]

    var encodedIdentifier: String {
        let identifier = "dm3_" + Self.shortStableIdentifier(provider: provider, kind: kind, remoteID: remoteID)
        ProviderItemCache.shared.store(self, identifier: identifier)
        return identifier
    }

    static func decode(_ rawValue: String) -> ProviderItemKey? {
        if rawValue.hasPrefix("dm3_") {
            return ProviderItemCache.shared.key(for: rawValue)
        }

        if rawValue.hasPrefix("dm2_") {
            let encoded = String(rawValue.dropFirst(4))
            guard let data = Data(base64URLEncoded: encoded),
                  let compact = try? JSONDecoder.providerItemKeyDecoder.decode(CompactProviderItemKey.self, from: data) else {
                return nil
            }
            return compact.key
        }

        guard rawValue.hasPrefix("dm:") else {
            return nil
        }
        let encoded = String(rawValue.dropFirst(3))
        guard let data = Data(base64URLEncoded: encoded) else { return nil }
        return try? JSONDecoder.providerItemKeyDecoder.decode(ProviderItemKey.self, from: data)
    }

    private static func shortStableIdentifier(provider: CloudProvider, kind: Kind, remoteID: String) -> String {
        let source = "\(provider.rawValue)|\(kind.rawValue)|\(remoteID)"
        return "\(fnv1a64(source))\(fnv1a64("drive-mount|\(source)"))"
    }

    private static func fnv1a64(_ source: String) -> String {
        var hash: UInt64 = 0xcbf29ce484222325
        for byte in source.utf8 {
            hash ^= UInt64(byte)
            hash &*= 0x100000001b3
        }
        return String(format: "%016llx", hash)
    }
}

private struct CompactProviderItemKey: Codable {
    var p: CloudProvider
    var k: ProviderItemKey.Kind
    var n: String
    var r: String
    var pr: String
    var pi: String?
    var s: Int64?
    var m: Date?
    var c: String?
    var e: [String: String]

    init(_ key: ProviderItemKey) {
        p = key.provider
        k = key.kind
        n = key.name
        r = key.remoteID
        pr = key.parentRemoteID
        pi = key.parentItemID
        s = key.size
        m = key.modifiedAt
        c = key.contentType
        e = key.extra
    }

    var key: ProviderItemKey {
        ProviderItemKey(
            provider: p,
            kind: k,
            name: n,
            remoteID: r,
            parentRemoteID: pr,
            parentItemID: pi,
            size: s,
            modifiedAt: m,
            contentType: c,
            extra: e
        )
    }
}

extension JSONEncoder {
    static let providerItemKeyEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()
}

extension JSONDecoder {
    static let providerItemKeyDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}

extension Data {
    func base64URLEncodedString() -> String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }

    init?(base64URLEncoded string: String) {
        var base64 = string
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        let padding = base64.count % 4
        if padding > 0 {
            base64 += String(repeating: "=", count: 4 - padding)
        }
        self.init(base64Encoded: base64)
    }
}
