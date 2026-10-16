import Foundation

public enum Target: String, Codable, Sendable { case mac, windows }
public struct Configuration: Codable, Equatable, Sendable {
    public var monitor: String
    public var macInput: UInt16
    public var windowsInput: UInt16
    public var usb2: USBDevice
    public var usb3: USBDevice
    public static let qualified = Configuration(monitor: "8EBDEBE8-EAF7-4084-8064-0B59A5E48919", macInput: 17, windowsInput: 15,
        usb2: USBDevice(vendor: 0x05e3, product: 0x0610, location: 0x02121000, parentVendor: 0x05e3, parentProduct: 0x0610, parentLocation: 0x02120000),
        usb3: USBDevice(vendor: 0x05e3, product: 0x0626, location: 0x02210000, parentVendor: 0x05e3, parentProduct: 0x0626, parentLocation: 0x02200000))
    public func input(for target: Target) -> UInt16 { target == .mac ? macInput : windowsInput }
    public func validate() throws {
        guard UUID(uuidString: monitor) != nil else { throw ConfigError.invalid("Monitor must be a system UUID.") }
        guard macInput > 0, windowsInput > 0, macInput != windowsInput, macInput <= 255, windowsInput <= 255 else { throw ConfigError.invalid("Input values must be distinct numbers from 1 to 255.") }
        guard usb2 != usb3, usb2.location != usb3.location else { throw ConfigError.invalid("Choose two different shared hubs.") }
        for hub in [usb2, usb3] {
            guard hub.vendor > 0, hub.product > 0, hub.location > 0, let parent = hub.parentLocation, parent > 0, parent != hub.location,
                hub.parentVendor != nil, hub.parentProduct != nil else { throw ConfigError.invalid("Each hub needs a complete parent topology.") }
        }
    }
    public func observe(_ devices: [USBDevice]) -> Ownership {
        let found = Set(devices)
        switch (found.contains(usb2), found.contains(usb3)) {
        case (true, true): return .owned
        case (false, false): return .absent
        default: return .partial
        }
    }
}
public enum ConfigError: LocalizedError {
    case invalid(String)
    public var errorDescription: String? { if case let .invalid(message) = self { return message }; return nil }
}
