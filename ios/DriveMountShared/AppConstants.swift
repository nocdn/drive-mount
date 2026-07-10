import Foundation

enum AppConstants {
    static let appGroupIdentifier = "group.org.bartoszbak.drivemount"
    static let appBundleIdentifier = "org.bartoszbak.drivemount"
    static let fileProviderBundleIdentifier = "org.bartoszbak.drivemount.fileprovider"
    static let b2FileProviderDomainIdentifier = "provider.b2"
    static let b2FileProviderDomainDisplayName = "Backblaze B2"
    static let connectionStoreFileName = "connections.json"
    static let diagnosticsFileName = "runtime.jsonl"
    static let providerItemCacheFileName = "provider-items.json"

    static var isUITesting: Bool {
        ProcessInfo.processInfo.arguments.contains("--uitesting")
    }
}
