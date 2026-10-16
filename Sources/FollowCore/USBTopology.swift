import Foundation

public struct USBDevice: Codable, Hashable, Sendable {
    public var vendor: UInt32
    public var product: UInt32
    public var location: UInt32
    public var parentVendor: UInt32?
    public var parentProduct: UInt32?
    public var parentLocation: UInt32?
    public init(vendor: UInt32, product: UInt32, location: UInt32, parentVendor: UInt32? = nil, parentProduct: UInt32? = nil, parentLocation: UInt32? = nil) {
        self.vendor = vendor; self.product = product; self.location = location
        self.parentVendor = parentVendor; self.parentProduct = parentProduct; self.parentLocation = parentLocation
    }
    public var label: String { String(format: "%04x:%04x @ %08x ← %08x", vendor, product, location, parentLocation ?? 0) }
}
public enum Ownership: String, Sendable { case owned, absent, partial, unknown }
