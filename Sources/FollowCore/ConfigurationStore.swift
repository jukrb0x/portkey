import Foundation
public struct ConfigurationStore {
    public let url: URL
    public init(url: URL) { self.url = url }
    public static var standard: Self {
        Self(url: FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("PortKey/config.json"))
    }
    public func load() throws -> Configuration {
        guard FileManager.default.fileExists(atPath: url.path) else { return .qualified }
        let value = try JSONDecoder().decode(Configuration.self, from: Data(contentsOf: url))
        try value.validate(); return value
    }
    public func save(_ value: Configuration) throws {
        try value.validate()
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(value).write(to: url, options: .atomic)
    }
}
