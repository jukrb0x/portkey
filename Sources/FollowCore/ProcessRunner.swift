import Foundation
import Darwin

public struct ProcessOutput: Sendable {
    public var status: Int32
    public var stdout: String
    public var stderr: String
    public var timedOut: Bool
    public var cancelled: Bool
    public init(status: Int32, stdout: String, stderr: String, timedOut: Bool, cancelled: Bool) {
        self.status = status; self.stdout = stdout; self.stderr = stderr; self.timedOut = timedOut; self.cancelled = cancelled
    }
    public var succeeded: Bool { status == 0 && !timedOut && !cancelled }
}
private final class CancellationFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var value = false
    func cancel() { lock.lock(); value = true; lock.unlock() }
    var cancelled: Bool { lock.lock(); defer { lock.unlock() }; return value }
}
public enum ProcessRunner {
    public static func run(executable: URL, arguments: [String], timeout: Double = 2.5) async -> ProcessOutput {
        let flag = CancellationFlag()
        return await withTaskCancellationHandler(operation: {
            await Task.detached(priority: .userInitiated) { execute(executable: executable, arguments: arguments, timeout: timeout, flag: flag) }.value
        }, onCancel: { flag.cancel() })
    }
    private static func execute(executable: URL, arguments: [String], timeout: Double, flag: CancellationFlag) -> ProcessOutput {
        let process = Process(); process.executableURL = executable; process.arguments = arguments
        let output = Pipe(), errors = Pipe(); process.standardOutput = output; process.standardError = errors
        process.standardInput = FileHandle.nullDevice
        if flag.cancelled { return ProcessOutput(status: -1, stdout: "", stderr: "Cancelled", timedOut: false, cancelled: true) }
        do { try process.run() } catch { return ProcessOutput(status: -1, stdout: "", stderr: error.localizedDescription, timedOut: false, cancelled: flag.cancelled) }
        let outFD = output.fileHandleForReading.fileDescriptor, errFD = errors.fileHandleForReading.fileDescriptor
        _ = fcntl(outFD, F_SETFL, O_NONBLOCK); _ = fcntl(errFD, F_SETFL, O_NONBLOCK)
        var out = Data(), err = Data(); let deadline = ProcessInfo.processInfo.systemUptime + max(0.01, timeout)
        var timedOut = false; var terminatedAt: Double?
        while process.isRunning {
            drain(outFD, into: &out); drain(errFD, into: &err)
            let now = ProcessInfo.processInfo.systemUptime
            if terminatedAt == nil && (now >= deadline || flag.cancelled) {
                timedOut = now >= deadline; process.terminate(); terminatedAt = now
            }
            if let start = terminatedAt, now - start >= 0.2 { kill(process.processIdentifier, SIGKILL) }
            usleep(10_000)
        }
        process.waitUntilExit()
        drain(outFD, into: &out); drain(errFD, into: &err)
        try? output.fileHandleForReading.close(); try? errors.fileHandleForReading.close()
        return ProcessOutput(status: process.terminationStatus, stdout: String(decoding: out, as: UTF8.self), stderr: String(decoding: err, as: UTF8.self), timedOut: timedOut, cancelled: flag.cancelled)
    }
    private static func drain(_ fd: Int32, into data: inout Data) {
        var bytes = [UInt8](repeating: 0, count: 4096)
        // Bound each drain so a chatty process cannot prevent the deadline check.
        for _ in 0..<64 {
            let count = read(fd, &bytes, bytes.count)
            guard count > 0 else { break }
            if data.count < 65536 { data.append(contentsOf: bytes.prefix(min(count, 65536 - data.count))) }
        }
    }
}
