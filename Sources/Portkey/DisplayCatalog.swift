import AppKit
import CoreGraphics

struct DetectedDisplay {
    let uuid: String
    let name: String
}
@MainActor enum DisplayCatalog {
    static func online() -> [DetectedDisplay] {
        NSScreen.screens.compactMap { screen in
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber,
                  let uuid = CGDisplayCreateUUIDFromDisplayID(number.uint32Value)?.takeRetainedValue() else { return nil }
            return DetectedDisplay(uuid: CFUUIDCreateString(nil, uuid) as String, name: screen.localizedName)
        }
    }
}
