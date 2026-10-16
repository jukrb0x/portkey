import Foundation
import FollowCore

final class AppDataMigrationTests {
    func testLegacyDataAndPreferencesMoveToPortKeyWithoutLosingEnrollment() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: home) }
        let oldConfig = home.appendingPathComponent("Library/Application Support/USBFollowDisplay/config.json")
        let newConfig = home.appendingPathComponent("Library/Application Support/PortKey/config.json")
        try FileManager.default.createDirectory(at: oldConfig.deletingLastPathComponent(), withIntermediateDirectories: true)
        var config = Configuration.qualified; config.macInput = 18
        try ConfigurationStore(url: oldConfig).save(config)
        let oldLog = home.appendingPathComponent("Library/Logs/USBFollowDisplay/app.log")
        try FileManager.default.createDirectory(at: oldLog.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("historic log\n".utf8).write(to: oldLog)
        let suite = "PortKey.Migration.\(UUID().uuidString)"
        let legacySuite = suite + ".legacy"
        let preferences = UserDefaults(suiteName: suite)!, legacy = UserDefaults(suiteName: legacySuite)!
        defer { preferences.removePersistentDomain(forName: suite); legacy.removePersistentDomain(forName: legacySuite) }
        legacy.set(false, forKey: "automatic"); legacy.set("zh-Hans", forKey: "interfaceLanguage")
        try AppDataMigration.run(home: home, preferences: preferences, legacyPreferences: legacy)
        XCTAssertEqual(try ConfigurationStore(url: newConfig).load(), config)
        XCTAssertEqual(try? String(contentsOf: home.appendingPathComponent("Library/Logs/PortKey/app.log"), encoding: .utf8), "historic log\n")
        XCTAssertEqual(FileManager.default.fileExists(atPath: oldConfig.path), false)
        XCTAssertEqual(preferences.object(forKey: "automatic") as? Bool, false)
        XCTAssertEqual(preferences.string(forKey: "interfaceLanguage"), "zh-Hans")
        // A repeated launch must preserve newer enrollment and language choices.
        config.windowsInput = 16; try ConfigurationStore(url: newConfig).save(config)
        preferences.set("en", forKey: "interfaceLanguage")
        try AppDataMigration.run(home: home, preferences: preferences, legacyPreferences: legacy)
        XCTAssertEqual(try ConfigurationStore(url: newConfig).load(), config)
        XCTAssertEqual(preferences.string(forKey: "interfaceLanguage"), "en")
    }
    func testExistingPortKeyDataWinsAndFailedMigrationIsNotSilentlyIgnored() throws {
        let home = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: home) }
        let support = home.appendingPathComponent("Library/Application Support")
        let old = ConfigurationStore(url: support.appendingPathComponent("USBFollowDisplay/config.json"))
        let new = ConfigurationStore(url: support.appendingPathComponent("PortKey/config.json"))
        try old.save(.qualified)
        var config = Configuration.qualified; config.macInput = 18; try new.save(config)
        let suite = "PortKey.Migration.\(UUID().uuidString)"
        let preferences = UserDefaults(suiteName: suite)!
        defer { preferences.removePersistentDomain(forName: suite) }
        try AppDataMigration.run(home: home, preferences: preferences, legacyPreferences: nil)
        XCTAssertEqual(try new.load(), config)
        XCTAssertEqual(try old.load(), .qualified)
        // A destination blocked by a file must fail without discarding the old data.
        try FileManager.default.removeItem(at: support.appendingPathComponent("PortKey"))
        try Data("blocked".utf8).write(to: support.appendingPathComponent("PortKey"))
        XCTAssertThrowsError(try AppDataMigration.run(home: home, preferences: preferences, legacyPreferences: nil))
        XCTAssertEqual(try old.load(), .qualified)
    }
}
