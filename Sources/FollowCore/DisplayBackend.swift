import Foundation

@MainActor public struct DisplayBackend {
    public let configuration: Configuration
    private let sleep: @MainActor (Double) async throws -> Void
    public let execute: @MainActor ([String]) async -> ProcessOutput
    public init(configuration: Configuration, sleep: @escaping @MainActor (Double) async throws -> Void = { try await Task.sleep(nanoseconds: UInt64($0 * 1_000_000_000)) }, execute: @escaping @MainActor ([String]) async -> ProcessOutput) { self.configuration = configuration; self.sleep = sleep; self.execute = execute }
    public func select(_ target: Target) async -> CommandResult {
        do { try configuration.validate() } catch { return .failed(error.localizedDescription) }
        let expected = configuration.input(for: target)
        let set = await execute(["display", configuration.monitor, "set", "input", String(expected)])
        guard set.succeeded else { return .failed(Self.message(set)) }
        guard !Task.isCancelled else { return .failed("Cancelled") }
        // This Philips returned 0 through 2.5s after a write; allow its input transition to finish.
        do { try await sleep(4.0) } catch { return .failed("Cancelled") }
        let get = await execute(["display", configuration.monitor, "get", "input"])
        guard get.succeeded else { return .inconclusive(Self.message(get)) }
        if let value = Self.parseInput(get.stdout, configuration: configuration), value == expected { return .confirmed(value) }
        return .inconclusive("readback \(get.stdout.trimmingCharacters(in: .whitespacesAndNewlines)); expected \(expected)")
    }
    public static func parseInput(_ text: String, configuration: Configuration) -> UInt16? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed.allSatisfy({ $0.isASCII && $0.isNumber }), let input = UInt16(trimmed), [configuration.macInput, configuration.windowsInput].contains(input) else { return nil }
        return input
    }
    private static func message(_ result: ProcessOutput) -> String {
        if result.cancelled { return "Cancelled" }
        if result.timedOut { return "Helper timed out" }
        return "Helper exit \(result.status): \((result.stderr + result.stdout).prefix(500))"
    }
}
