import Foundation
import FollowCore

final class LocalizationTests {
    func testEnglishDefaultAndPersistentLanguageDoesNotChangeAutomation() {
        let suite = "Portkey.LocalizationTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set(false, forKey: "automatic")
        XCTAssertEqual(UILanguage.saved(in: defaults), .english)
        UILanguage.simplifiedChinese.save(in: defaults)
        XCTAssertEqual(UILanguage.saved(in: defaults), .simplifiedChinese)
        XCTAssertEqual(defaults.bool(forKey: "automatic"), false)
        defaults.set("unsupported", forKey: "interfaceLanguage")
        XCTAssertEqual(UILanguage.saved(in: defaults), .english)
    }
    func testEnglishAndChineseMessagesRenderFromSameStatus() {
        let english = Localizer(language: .english)
        let chinese = Localizer(language: .simplifiedChinese)
        XCTAssertEqual(english.text("Settings / Rebind…"), "Settings / Rebind…")
        XCTAssertEqual(chinese.text("Settings / Rebind…"), "\u{8bbe}\u{7f6e} / \u{91cd}\u{65b0}\u{7ed1}\u{5b9a}…")
        let message = UIMessage("Readback confirmed: %@", "17")
        XCTAssertEqual(message.localized(using: english), "Readback confirmed: 17")
        XCTAssertEqual(message.localized(using: chinese), "\u{8bfb}\u{56de}\u{5df2}\u{786e}\u{8ba4}\u{ff1a}17")
        XCTAssertEqual(english.text("An unknown system detail"), "An unknown system detail")
    }
    func testEveryTranslationKeepsItsFormatArguments() throws {
        let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Sources/FollowCore/Resources")
        func strings(_ locale: String) throws -> [String: String] {
            let data = try Data(contentsOf: root.appendingPathComponent("\(locale).lproj/Localizable.strings"))
            return try PropertyListSerialization.propertyList(from: data, format: nil) as! [String: String]
        }
        let english = try strings("en"), chinese = try strings("zh-Hans")
        XCTAssertEqual(Set(english.keys), Set(chinese.keys))
        for key in english.keys {
            XCTAssertEqual(english[key]!.components(separatedBy: "%@").count, chinese[key]!.components(separatedBy: "%@").count)
            XCTAssertEqual(Localizer(language: .english).text(key), english[key]!)
            XCTAssertEqual(Localizer(language: .simplifiedChinese).text(key), chinese[key]!)
        }
    }
}
