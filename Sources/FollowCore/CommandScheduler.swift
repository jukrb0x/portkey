import Foundation

public enum CommandResult: Equatable, Sendable {
    case confirmed(UInt16)
    case inconclusive(String)
    case failed(String)
    public var isConfirmed: Bool { if case .confirmed = self { return true }; return false }
    public var description: String {
        switch self {
        case .confirmed(let value): return "Readback confirmed: \(value)"
        case .inconclusive(let message): return "Sent, unverified: \(message)"
        case .failed(let message): return "Error: \(message)"
        }
    }
}

/// Cancellation stops retries; a replacement starts only after the old backend returns.
@MainActor public final class CommandScheduler {
    public typealias Backend = @MainActor (Target) async -> CommandResult
    private let backend: Backend
    private let sleep: @MainActor (Double) async throws -> Void
    private var desired: InputRequest?
    private var active: Task<Void, Never>?
    public private(set) var result: CommandResult?
    public var onChange: (@MainActor (String) -> Void)?
    public init(backend: @escaping Backend, sleep: @escaping @MainActor (Double) async throws -> Void = { try await Task.sleep(nanoseconds: UInt64($0 * 1_000_000_000)) }) {
        self.backend = backend; self.sleep = sleep
    }
    public func submit(_ request: InputRequest?) {
        guard request != desired else { return }
        desired = request; result = nil; active?.cancel()
        if active == nil { start() }
    }
    private func start() {
        guard let request = desired else { return }
        active = Task {
            for attempt in 0..<3 {
                guard !Task.isCancelled, desired == request else { break }
                if attempt > 0 {
                    do { try await sleep(attempt == 1 ? 0.75 : 1.5) } catch { break }
                    guard !Task.isCancelled, desired == request else { break }
                }
                onChange?("Switching to \(request.target.rawValue)… (\(attempt + 1)/3)")
                let outcome = await backend(request.target)
                guard !Task.isCancelled, desired == request else { break }
                result = outcome; onChange?(outcome.description)
                if outcome.isConfirmed { break }
            }
            active = nil
            if desired != request || Task.isCancelled { start() }
        }
    }
    public func waitUntilIdle() async { while let task = active { await task.value } }
}
