import Foundation

// This narrow session boundary lets UI orchestration be verified without starting USB or display hardware.
protocol CoreSession: AnyObject {
    func start() throws
    func snapshot() throws -> Snapshot
    func action(_ value: [String: Any]) throws -> [String: Any]
    func stop()
}
final class NativeCore: CoreSession {
    private var handle: UnsafeMutableRawPointer?
    func start() throws {
        precondition(Thread.isMainThread)
        handle = pk_create()
        guard handle != nil else { throw BridgeError.unavailable }
    }
    private func data(_ pointer: UnsafeMutablePointer<CChar>?) throws -> Data {
        guard let pointer else { throw BridgeError.unavailable }
        defer { pk_string_free(pointer) }
        guard let result = String(validatingUTF8: pointer)?.data(using: .utf8) else { throw BridgeError.invalidResponse }
        return result
    }
    func snapshot() throws -> Snapshot {
        precondition(Thread.isMainThread)
        guard let handle else { throw BridgeError.unavailable }
        return try JSONDecoder().decode(Snapshot.self, from: data(pk_snapshot(handle)))
    }
    func action(_ value: [String: Any]) throws -> [String: Any] {
        precondition(Thread.isMainThread)
        guard let handle else { throw BridgeError.unavailable }
        let bytes = try JSONSerialization.data(withJSONObject: value)
        let pointer = String(decoding: bytes, as: UTF8.self).withCString { pk_action(handle, $0) }
        guard let result = try JSONSerialization.jsonObject(with: data(pointer)) as? [String: Any] else { throw BridgeError.invalidResponse }
        return result
    }
    func stop() {
        precondition(Thread.isMainThread)
        if let handle { pk_destroy(handle); self.handle = nil }
    }
}
