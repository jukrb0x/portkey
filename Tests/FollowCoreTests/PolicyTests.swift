import Foundation
import FollowCore

final class PolicyTests {
    let config = Configuration.qualified
    var pair: [USBDevice] { [config.usb2, config.usb3] }
    func testTopologyRequiresBothBranchesAndExactParent() {
        XCTAssertEqual(config.observe(pair + pair), .owned)
        XCTAssertEqual(config.observe([config.usb2]), .partial)
        var moved = config.usb3; moved.parentLocation = 0x09900000
        XCTAssertEqual(config.observe([config.usb2, moved]), .partial)
        var upstream = config.usb2; upstream.location = 0x02120000
        XCTAssertEqual(config.observe([upstream]), .absent)
        XCTAssertEqual(config.observe(pair + [USBDevice(vendor: 0x04fe, product: 0x21, location: 1)]), .owned)
    }
    func testStartupAbsenceAndInventoryFailureNeverSelectWindows() {
        var p = OwnershipPolicy()
        p.observe(.absent, now: 0); XCTAssertNil(p.tick(now: 10))
        p.observe(.unknown, now: 11); XCTAssertNil(p.tick(now: 20))
    }
    func testSettlingPartialArrivalAndPeripheralChurn() {
        var p = OwnershipPolicy()
        p.observe(.partial, now: 0); XCTAssertNil(p.tick(now: 1))
        p.observe(.owned, now: 1); XCTAssertNil(p.tick(now: 1.29))
        XCTAssertEqual(p.tick(now: 1.31)?.target, .mac)
        let request = p.request
        p.observe(.owned, now: 2); XCTAssertEqual(p.tick(now: 3), request)
        p.observe(.partial, now: 4); XCTAssertNil(p.request)
        p.observe(.absent, now: 4.1); XCTAssertNil(p.tick(now: 4.79))
        XCTAssertEqual(p.tick(now: 4.81)?.target, .windows)
    }
    func testRapidReversalCancelsDeparture() {
        var p = OwnershipPolicy(); p.observe(.owned, now: 0); _ = p.tick(now: 1)
        p.observe(.absent, now: 2); p.observe(.owned, now: 2.2)
        XCTAssertNil(p.tick(now: 2.4)); XCTAssertEqual(p.tick(now: 2.6)?.target, .mac)
    }
    func testFailedEnumerationDisarmsDepartureUntilOwnedAgain() {
        var p = OwnershipPolicy(); p.observe(.owned, now: 0); _ = p.tick(now: 1)
        p.observe(.unknown, now: 2); p.observe(.absent, now: 3)
        XCTAssertNil(p.tick(now: 5))
    }
    func testManualHoldsAcrossPartialAndSameObservationUntilOwnershipChanges() {
        var p = OwnershipPolicy(); p.observe(.owned, now: 0); _ = p.tick(now: 1)
        p.manual(.windows); let manual = p.request
        p.observe(.partial, now: 2); p.observe(.owned, now: 3)
        XCTAssertEqual(p.tick(now: 5), manual)
        p.observe(.absent, now: 6); XCTAssertNil(p.request)
        XCTAssertEqual(p.tick(now: 7)?.target, .windows)
        XCTAssertNotEqual(p.request, manual)
    }
    func testPauseAndReenableApplyStartupRules() {
        var p = OwnershipPolicy(); p.observe(.owned, now: 0); _ = p.tick(now: 1)
        p.setAutomatic(false, now: 2); XCTAssertNil(p.request)
        p.observe(.absent, now: 3); p.setAutomatic(true, now: 4)
        XCTAssertNil(p.tick(now: 6))
        p.observe(.owned, now: 7); XCTAssertEqual(p.tick(now: 8)?.target, .mac)
    }
    func testSleepWakeAndMonitorReconnectDoNotReplayDeparture() {
        var p = OwnershipPolicy(); p.observe(.owned, now: 0); _ = p.tick(now: 1)
        p.sleep(); p.observe(.absent, now: 2); XCTAssertNil(p.tick(now: 4))
        p.wake(now: 5); p.observe(.absent, now: 5); XCTAssertNil(p.tick(now: 7))
        p.observe(.owned, now: 8); XCTAssertEqual(p.tick(now: 9)?.target, .mac)
        let old = p.request; p.monitorReconnected(now: 10)
        XCTAssertEqual(p.tick(now: 11)?.target, .mac); XCTAssertNotEqual(old, p.request)
        p.observe(.absent, now: 12); _ = p.tick(now: 13)
        p.monitorReconnected(now: 14); XCTAssertNil(p.tick(now: 16))
    }
    func testConfigurationRejectsInvalidAndAmbiguousEnrollment() throws {
        try config.validate()
        var bad = config; bad.monitor = ""; XCTAssertThrowsError(try bad.validate())
        bad = config; bad.windowsInput = 17; XCTAssertThrowsError(try bad.validate())
        bad = config; bad.usb3 = bad.usb2; XCTAssertThrowsError(try bad.validate())
        let data = try JSONEncoder().encode(config)
        XCTAssertEqual(try JSONDecoder().decode(Configuration.self, from: data), config)
    }
}
