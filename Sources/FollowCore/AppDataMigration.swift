import Foundation

public enum AppDataMigration {
    public static let legacyBundleIdentifier = "local.usbfollowdisplay.app"
    public static func run(home: URL = FileManager.default.homeDirectoryForCurrentUser,
                           preferences: UserDefaults = .standard,
                           legacyPreferences: UserDefaults? = UserDefaults(suiteName: legacyBundleIdentifier)) throws {
        let files = FileManager.default
        for parent in ["Library/Application Support", "Library/Logs"] {
            let root = home.appendingPathComponent(parent)
            let old = root.appendingPathComponent("USBFollowDisplay")
            let new = root.appendingPathComponent("PortKey")
            var isDirectory: ObjCBool = false
            if files.fileExists(atPath: new.path, isDirectory: &isDirectory) {
                guard isDirectory.boolValue else { throw CocoaError(.fileWriteFileExists, userInfo: [NSFilePathErrorKey: new.path]) }
                continue
            }
            if files.fileExists(atPath: old.path) { try files.moveItem(at: old, to: new) }
        }
        for key in ["automatic", "interfaceLanguage"] where preferences.object(forKey: key) == nil {
            if let value = legacyPreferences?.object(forKey: key) { preferences.set(value, forKey: key) }
        }
    }
}
