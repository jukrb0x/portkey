import Foundation

public enum CommandResult: Equatable, Sendable {
    case confirmed(UInt16)
    case inconclusive(String)
    case failed(String)
    public var isConfirmed: Bool { if case .confirmed = self { return true }; return false }
    public var message: UIMessage {
        switch self {
        case .confirmed(let value): return UIMessage("Readback confirmed: %@", String(value))
        case .inconclusive(let detail): return UIMessage("Sent, unverified: %@", detail)
        case .failed(let detail): return UIMessage("Error: %@", detail)
        }
    }
    public var description: String { message.localized(using: Localizer(language: .english)) }

}

public enum CommandProgress: Sendable {
    case switching(Target, Int)
    case completed(CommandResult)
    public var message: UIMessage {
        switch self {
        case .switching(let target, let attempt): return UIMessage("Switching to %@… (%@/3)", target == .mac ? "Mac" : "Windows", String(attempt))
        case .completed(let outcome): return outcome.message
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
    public var onChange: (@MainActor (CommandProgress) -> Void)?
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
                onChange?(.switching(request.target, attempt + 1))
                let outcome = await backend(request.target)
                guard !Task.isCancelled, desired == request else { break }
                result = outcome; onChange?(.completed(outcome))
                if outcome.isConfirmed { break }
            }
            active = nil
            if desired != request || Task.isCancelled { start() }
        }
    }
    public func waitUntilIdle() async { while let task = active { await task.value } }
}
