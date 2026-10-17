import Foundation
import IOKit

// Notifications are hints only. The Rust worker rereads the full topology.
final class USBNotifications {
    private var port: IONotificationPortRef?
    private var iterators: [io_iterator_t] = []
    var onChange: (() -> Void)?

    @discardableResult func start() -> Bool {
        precondition(Thread.isMainThread)
        stop()
        guard let created = IONotificationPortCreate(kIOMainPortDefault) else { return false }
        port = created
        guard let source = IONotificationPortGetRunLoopSource(created)?.takeUnretainedValue() else {
            stop(); return false
        }
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        for event in [kIOFirstMatchNotification, kIOTerminatedNotification] {
            var iterator: io_iterator_t = 0
            let result = IOServiceAddMatchingNotification(
                created, event, IOServiceMatching("IOUSBHostDevice"),
                { context, iterator in
                    USBNotifications.drain(iterator)
                    guard let context else { return }
                    let monitor = Unmanaged<USBNotifications>.fromOpaque(context).takeUnretainedValue()
                    guard monitor.port != nil else { return }
                    monitor.onChange?()
                }, Unmanaged.passUnretained(self).toOpaque(), &iterator
            )
            guard result == KERN_SUCCESS else {
                if iterator != 0 { IOObjectRelease(iterator) }
                stop(); return false
            }
            iterators.append(iterator)
            // Drain the initial objects to arm notifications without inventing an event.
            Self.drain(iterator)
        }
        return true
    }

    func stop() {
        precondition(Thread.isMainThread)
        if let port, let source = IONotificationPortGetRunLoopSource(port)?.takeUnretainedValue() {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes)
        }
        for iterator in iterators { IOObjectRelease(iterator) }
        iterators.removeAll()
        if let port { IONotificationPortDestroy(port) }
        port = nil
    }

    private static func drain(_ iterator: io_iterator_t) {
        while case let service = IOIteratorNext(iterator), service != 0 { IOObjectRelease(service) }
    }
    deinit { stop() }
}
