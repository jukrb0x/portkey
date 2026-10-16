import Foundation

@MainActor public enum PolicyTicker {
    public static func start(_ action: @escaping @MainActor () -> Void) -> Timer {
        let timer = Timer(timeInterval: 0.1, repeats: true) { _ in MainActor.assumeIsolated { action() } }
        RunLoop.main.add(timer, forMode: .common)
        return timer
    }
}
