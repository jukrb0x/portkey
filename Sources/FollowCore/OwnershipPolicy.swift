import Foundation

public struct InputRequest: Equatable, Sendable {
    public let target: Target
    public let generation: UInt64
    public init(target: Target, generation: UInt64) { self.target = target; self.generation = generation }
}

/// Clock-driven policy. Repeated snapshots never extend a settling deadline.
public struct OwnershipPolicy {
    public private(set) var ownership: Ownership = .unknown
    public private(set) var automatic = true
    public private(set) var request: InputRequest?
    public private(set) var sleeping = false
    private var generation: UInt64 = 0
    private var established = false
    private var deadline: TimeInterval?
    private var manualAnchor: Ownership?
    private var stable: Ownership = .unknown
    public init() {}
    public mutating func observe(_ value: Ownership, now: TimeInterval) {
        guard !sleeping, value != ownership else { return }
        ownership = value
        if value == .unknown { established = false; invalidate(); return }
        if value == .owned || value == .absent {
            if let anchor = manualAnchor, value != anchor { manualAnchor = nil; invalidate() }
            stable = value
        }
        guard manualAnchor == nil else { return }
        invalidate()
        guard automatic else { return }
        if value == .owned { deadline = now + 0.3 }
        if value == .absent && established { deadline = now + 0.7 }
    }
    @discardableResult public mutating func tick(now: TimeInterval) -> InputRequest? {
        if let due = deadline, now >= due, automatic, !sleeping {
            deadline = nil
            if ownership == .owned { established = true; issue(.mac) }
            else if ownership == .absent && established { issue(.windows) }
        }
        return request
    }
    public mutating func manual(_ target: Target) {
        guard !sleeping else { return }
        invalidate(); manualAnchor = stable; issue(target)
    }
    public mutating func setAutomatic(_ enabled: Bool, now: TimeInterval) {
        automatic = enabled; reset()
        if ownership == .owned || ownership == .absent { stable = ownership }
        guard enabled else { return }
        if ownership == .owned { deadline = now + 0.3 }
    }
    public mutating func sleep() { sleeping = true; reset(); ownership = .unknown }
    public mutating func wake(now: TimeInterval) { sleeping = false; reset(); ownership = .unknown }
    public mutating func monitorReconnected(now: TimeInterval) {
        invalidate()
        guard automatic, !sleeping, manualAnchor == nil else { return }
        if ownership == .owned { deadline = now + 0.3 }
    }
    private mutating func reset() { invalidate(); established = false; manualAnchor = nil; stable = .unknown }
    private mutating func invalidate() { generation &+= 1; request = nil; deadline = nil }
    private mutating func issue(_ target: Target) { generation &+= 1; request = InputRequest(target: target, generation: generation) }
}
