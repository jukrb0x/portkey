import Foundation
import FollowCore

@MainActor final class SchedulerTests {
    func testRetriesBoundedAndInvalidReadbackInconclusive() async {
        var calls: [Target] = []; var delays: [Double] = []
        let scheduler = CommandScheduler(backend: { target in calls.append(target); return .inconclusive("0") }, sleep: { seconds in delays.append(seconds) })
        scheduler.submit(InputRequest(target: .mac, generation: 1))
        await scheduler.waitUntilIdle()
        XCTAssertEqual(calls, [.mac, .mac, .mac]); XCTAssertEqual(delays, [0.75, 1.5])
        XCTAssertEqual(scheduler.result, .inconclusive("0"))
    }
    func testConfirmedDoesNotRetryOrRepeatIdenticalRequest() async {
        var calls = 0
        let scheduler = CommandScheduler(backend: { _ in calls += 1; return .confirmed(17) })
        let request = InputRequest(target: .mac, generation: 1)
        scheduler.submit(request); await scheduler.waitUntilIdle(); scheduler.submit(request)
        await scheduler.waitUntilIdle(); XCTAssertEqual(calls, 1)
    }
    func testOldInflightWriteGetsFinalCorrectionWithoutConcurrency() async {
        var gate: CheckedContinuation<Void, Never>?
        var calls: [Target] = []; var active = 0; var peak = 0
        let scheduler = CommandScheduler(backend: { target in
            active += 1; peak = max(peak, active); calls.append(target)
            if calls.count == 1 { await withCheckedContinuation { gate = $0 } }
            active -= 1; return .confirmed(target == .mac ? 17 : 15)
        })
        scheduler.submit(InputRequest(target: .windows, generation: 1))
        while gate == nil { await Task.yield() }
        scheduler.submit(InputRequest(target: .mac, generation: 2))
        gate?.resume(); await scheduler.waitUntilIdle()
        XCTAssertEqual(calls, [.windows, .mac]); XCTAssertEqual(peak, 1)
        XCTAssertEqual(scheduler.result, .confirmed(17))
    }
    func testPauseCancelsRetryAndIgnoresStaleResult() async {
        var gate: CheckedContinuation<Void, Never>?
        var calls = 0
        let scheduler = CommandScheduler(backend: { _ in
            calls += 1; await withCheckedContinuation { gate = $0 }; return .failed("late")
        })
        scheduler.submit(InputRequest(target: .windows, generation: 1))
        while gate == nil { await Task.yield() }
        scheduler.submit(nil); gate?.resume(); await scheduler.waitUntilIdle()
        XCTAssertEqual(calls, 1); XCTAssertNil(scheduler.result)
    }
}
