import Foundation
import IOKit
import FollowCore

struct USBInventory {
    var devices: [USBDevice]
    var hubs: [USBDevice]
}
enum USBReadError: LocalizedError {
    case failed(String)
    var errorDescription: String? { if case let .failed(message) = self { return message }; return nil }
}
final class USBMonitor {
    private var port: IONotificationPortRef?
    private var iterators: [io_iterator_t] = []
    var onChange: (() -> Void)?
    func start() throws {
        guard let port = IONotificationPortCreate(kIOMainPortDefault) else { throw USBReadError.failed("Cannot create USB notification port") }
        self.port = port
        CFRunLoopAddSource(CFRunLoopGetMain(), IONotificationPortGetRunLoopSource(port).takeUnretainedValue(), .commonModes)
        for event in [kIOFirstMatchNotification, kIOTerminatedNotification] {
            var iterator: io_iterator_t = 0
            let result = IOServiceAddMatchingNotification(port, event, IOServiceMatching("IOUSBHostDevice"), { context, iterator in
                while case let service = IOIteratorNext(iterator), service != 0 { IOObjectRelease(service) }
                guard let context else { return }
                Unmanaged<USBMonitor>.fromOpaque(context).takeUnretainedValue().onChange?()
            }, Unmanaged.passUnretained(self).toOpaque(), &iterator)
            guard result == KERN_SUCCESS else { throw USBReadError.failed("Cannot subscribe to USB events: \(result)") }
            iterators.append(iterator)
            while case let service = IOIteratorNext(iterator), service != 0 { IOObjectRelease(service) }
        }
    }
    deinit { for iterator in iterators { IOObjectRelease(iterator) }; if let port { IONotificationPortDestroy(port) } }
    static func snapshot() throws -> USBInventory {
        var iterator: io_iterator_t = 0
        guard IOServiceGetMatchingServices(kIOMainPortDefault, IOServiceMatching("IOUSBHostDevice"), &iterator) == KERN_SUCCESS else { throw USBReadError.failed("USB enumeration failed") }
        defer { IOObjectRelease(iterator) }
        var devices: [USBDevice] = [], hubs: [USBDevice] = []
        while case let entry = IOIteratorNext(iterator), entry != 0 {
            defer { IOObjectRelease(entry) }
            let props = try properties(entry)
            guard let vendor = number(props, "idVendor"), let product = number(props, "idProduct"), let location = number(props, "locationID") else { throw USBReadError.failed("Incomplete USB inventory; writes paused") }
            let parent = try parentIdentity(entry)
            let device = USBDevice(vendor: vendor, product: product, location: location, parentVendor: parent?.0, parentProduct: parent?.1, parentLocation: parent?.2)
            devices.append(device)
            if number(props, "bDeviceClass") == 9 { hubs.append(device) }
        }
        guard IOIteratorIsValid(iterator) != 0 else { throw USBReadError.failed("USB inventory changed during enumeration; retrying") }
        return USBInventory(devices: Array(Set(devices)), hubs: Array(Set(hubs)).sorted { $0.location < $1.location })
    }
    private static func properties(_ entry: io_registry_entry_t) throws -> NSDictionary {
        var result: Unmanaged<CFMutableDictionary>?
        guard IORegistryEntryCreateCFProperties(entry, &result, kCFAllocatorDefault, 0) == KERN_SUCCESS, let result else { throw USBReadError.failed("USB property read failed") }
        return result.takeRetainedValue() as NSDictionary
    }
    private static func number(_ props: NSDictionary, _ key: String) -> UInt32? { (props[key] as? NSNumber)?.uint32Value }
    private static func parentIdentity(_ entry: io_registry_entry_t) throws -> (UInt32, UInt32, UInt32)? {
        var current: io_registry_entry_t = 0
        guard IORegistryEntryGetParentEntry(entry, kIOServicePlane, &current) == KERN_SUCCESS else { return nil }
        while current != 0 {
            let props: NSDictionary
            do { props = try properties(current) } catch { IOObjectRelease(current); throw error }
            if let v = number(props, "idVendor"), let p = number(props, "idProduct"), let l = number(props, "locationID") { IOObjectRelease(current); return (v, p, l) }
            var parent: io_registry_entry_t = 0
            let result = IORegistryEntryGetParentEntry(current, kIOServicePlane, &parent)
            IOObjectRelease(current)
            if result != KERN_SUCCESS { return nil }; current = parent
        }
        return nil
    }
}
