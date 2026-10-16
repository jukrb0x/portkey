import Foundation
@main struct Runner {
    @MainActor static func main() async throws {
        let p = PolicyTests()
        p.testTopologyRequiresBothBranchesAndExactParent()
        p.testStartupAbsenceAndInventoryFailureNeverSelectWindows()
        p.testSettlingPartialArrivalAndPeripheralChurn()
        p.testRapidReversalCancelsDeparture()
        p.testFailedEnumerationDisarmsDepartureUntilOwnedAgain()
        p.testManualHoldsAcrossPartialAndSameObservationUntilOwnershipChanges()
        p.testPauseAndReenableApplyStartupRules()
        p.testSleepWakeAndMonitorReconnectDoNotReplayDeparture()
        try p.testConfigurationRejectsInvalidAndAmbiguousEnrollment()
        let s = SchedulerTests()
        await s.testRetriesBoundedAndInvalidReadbackInconclusive()
        await s.testConfirmedDoesNotRetryOrRepeatIdenticalRequest()
        await s.testOldInflightWriteGetsFinalCorrectionWithoutConcurrency()
        await s.testPauseCancelsRetryAndIgnoresStaleResult()
        let a = AdapterTests()
        try await a.testBackendTargetsOnlyEnrolledUUIDAndRejectsUnknownReadback()
        await a.testMonitorSettlesBeforeReadbackWithoutRepeatedWrites()
        await a.testTransportFailureNeverRunsReadback()
        try a.testConfigurationPersistsAtomicallyAndInvalidSavePreservesPrevious()
        await a.testRealProcessHandlesFullPipesFailureDeadlineAndCancellation()
        print("\(checks) checks, \(failures) failures")
        if failures > 0 { exit(1) }
    }
}
