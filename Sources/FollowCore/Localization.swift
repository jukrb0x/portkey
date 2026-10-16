import Foundation

public enum UILanguage: String, CaseIterable, Sendable {
    case english = "en"
    case simplifiedChinese = "zh-Hans"
    public static func saved(in defaults: UserDefaults = .standard) -> Self {
        defaults.string(forKey: "interfaceLanguage").flatMap(Self.init(rawValue:)) ?? .english
    }
    public func save(in defaults: UserDefaults = .standard) { defaults.set(rawValue, forKey: "interfaceLanguage") }
}

public struct Localizer {
    public let language: UILanguage
    private let bundle: Bundle?
    public init(language: UILanguage) {
        self.language = language
        // A packaged app uses its own resources; CLI/test products use SwiftPM resources.
        let root = Bundle.main.bundleURL.pathExtension == "app" ? Bundle.main : Bundle.module
        let path = root.path(forResource: language.rawValue, ofType: "lproj")
            ?? root.path(forResource: language.rawValue.lowercased(), ofType: "lproj")
        bundle = path.flatMap(Bundle.init(path:))
    }
    public func text(_ key: String, _ arguments: String...) -> String { format(key, arguments: arguments) }
    func format(_ key: String, arguments: [String]) -> String {
        let template = bundle?.localizedString(forKey: key, value: key, table: nil) ?? key
        guard !arguments.isEmpty else { return template }
        return String(format: template, locale: Locale(identifier: language.rawValue), arguments: arguments.map { text($0) as CVarArg })
    }
}

/// Store message keys and values so changing language also updates existing status.
public struct UIMessage: ExpressibleByStringLiteral, Sendable {
    let key: String
    let arguments: [String]
    public init(_ key: String, _ arguments: String...) { self.key = key; self.arguments = arguments }
    public init(stringLiteral value: String) { self.init(value) }
    public func localized(using localizer: Localizer) -> String { localizer.format(key, arguments: arguments) }
}
