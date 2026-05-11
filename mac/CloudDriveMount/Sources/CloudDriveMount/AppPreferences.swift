import Foundation

enum AppPreferences {
    private static let startAtLoginKey = "StartAtLogin"
    private static let startMinimizedKey = "StartMinimized"

    static func registerDefaults() {
        UserDefaults.standard.register(defaults: [
            startAtLoginKey: true,
            startMinimizedKey: true
        ])
    }

    static var startAtLogin: Bool {
        get { UserDefaults.standard.bool(forKey: startAtLoginKey) }
        set { UserDefaults.standard.set(newValue, forKey: startAtLoginKey) }
    }

    static var startMinimized: Bool {
        get { UserDefaults.standard.bool(forKey: startMinimizedKey) }
        set { UserDefaults.standard.set(newValue, forKey: startMinimizedKey) }
    }
}
