import Foundation
import FollowCore

@MainActor final class AdapterTests {
    func testBackendTargetsOnlyEnrolledUUIDAndRejectsUnknownReadback() async throws {
        var args: [[String]] = []
        let config = Configuration.qualified
        let backend = DisplayBackend(configuration: config, sleep: { _ in }, execute: { arguments in
            args.append(arguments)
            return ProcessOutput(status: 0, stdout: arguments.contains("get") ? "0\n" : "", stderr: "", timedOut: false, cancelled: false)
        })
        XCTAssertEqual(await backend.select(.windows), .inconclusive("readback 0; expected 15"))
        XCTAssertEqual(args, [["display", "8EBDEBE8-EAF7-4084-8064-0B59A5E48919", "set", "input", "15"], ["display", "8EBDEBE8-EAF7-4084-8064-0B59A5E48919", "get", "input"]])
        XCTAssertNil(DisplayBackend.parseInput("17\nnoise", configuration: config))
        XCTAssertNil(DisplayBackend.parseInput("65535", configuration: config))
        XCTAssertEqual(DisplayBackend.parseInput("17\n", configuration: config), 17)
    }
    func testMonitorSettlesBeforeReadbackWithoutRepeatedWrites() async {
        var elapsed = 0.0; var writes = 0
        let backend = DisplayBackend(configuration: .qualified, sleep: { elapsed += $0 }, execute: { args in
            if args.contains("set") { writes += 1 }
            return ProcessOutput(status: 0, stdout: args.contains("get") ? (elapsed >= 3.8 ? "17" : "0") : "", stderr: "", timedOut: false, cancelled: false)
        })
        XCTAssertEqual(await backend.select(.mac), .confirmed(17)); XCTAssertEqual(writes, 1)
    }
    func testTransportFailureNeverRunsReadback() async {
        var calls = 0
        let backend = DisplayBackend(configuration: .qualified, execute: { _ in calls += 1; return ProcessOutput(status: 1, stdout: "missing monitor", stderr: "", timedOut: false, cancelled: false) })
        let outcome = await backend.select(.mac)
        XCTAssertEqual(outcome.isConfirmed, false); XCTAssertEqual(calls, 1)
    }
    func testConfigurationPersistsAtomicallyAndInvalidSavePreservesPrevious() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = ConfigurationStore(url: directory.appendingPathComponent("config.json"))
        XCTAssertEqual(try store.load(), .qualified)
        try store.save(.qualified)
        var invalid = Configuration.qualified; invalid.monitor = "arbitrary"
        XCTAssertThrowsError(try store.save(invalid))
        XCTAssertEqual(try store.load(), .qualified)
        try Data("{}".utf8).write(to: store.url)
        XCTAssertThrowsError(try store.load())
    }
    func testRealProcessHandlesFullPipesFailureDeadlineAndCancellation() async {
        let out = await ProcessRunner.run(executable: URL(fileURLWithPath: "/bin/sh"), arguments: ["-c", "i=0; while [ $i -lt 10000 ]; do echo abcdefghijklmnopqrstuvwxyz; echo err >&2; i=$((i+1)); done; exit 7"], timeout: 3)
        XCTAssertEqual(out.status, 7); XCTAssertEqual(out.stdout.utf8.count <= 65536, true)
        XCTAssertEqual(out.stderr.contains("err"), true); XCTAssertEqual(out.timedOut, false)
        let began = Date()
        let hung = await ProcessRunner.run(executable: URL(fileURLWithPath: "/bin/sh"), arguments: ["-c", "trap '' TERM; exec /bin/sleep 10"], timeout: 0.1)
        XCTAssertEqual(hung.timedOut, true); XCTAssertEqual(Date().timeIntervalSince(began) < 2, true)
        let missing = await ProcessRunner.run(executable: URL(fileURLWithPath: "/nonexistent/helper"), arguments: [], timeout: 0.1)
        XCTAssertNotEqual(missing.status, 0)
        let task = Task { await ProcessRunner.run(executable: URL(fileURLWithPath: "/bin/sleep"), arguments: ["10"], timeout: 3) }
        task.cancel(); let cancelled = await task.value; XCTAssertEqual(cancelled.cancelled, true)
    }
}
