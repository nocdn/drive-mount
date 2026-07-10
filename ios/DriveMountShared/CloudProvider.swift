import Foundation

enum CloudProvider: String, CaseIterable, Codable, Identifiable, Sendable {
    case backblazeB2 = "b2"
    case googleDrive = "googleDrive"
    case oneDrive = "oneDrive"
    case seedbox = "seedbox"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .backblazeB2:
            "Backblaze B2"
        case .googleDrive:
            "Google Drive"
        case .oneDrive:
            "OneDrive"
        case .seedbox:
            "Seedbox"
        }
    }

    var defaultConnectionName: String {
        switch self {
        case .backblazeB2:
            "B2"
        case .googleDrive:
            "Google Drive"
        case .oneDrive:
            "OneDrive"
        case .seedbox:
            "Seedbox"
        }
    }

    var symbolName: String {
        switch self {
        case .backblazeB2:
            "shippingbox"
        case .googleDrive:
            "externaldrive"
        case .oneDrive:
            "cloud"
        case .seedbox:
            "server.rack"
        }
    }

    var supportsIOSFileProvider: Bool {
        switch self {
        case .backblazeB2, .googleDrive, .oneDrive:
            true
        case .seedbox:
            false
        }
    }
}
