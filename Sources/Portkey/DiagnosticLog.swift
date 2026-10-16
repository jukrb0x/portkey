import Foundation
final class DiagnosticLog {
    let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/USBFollowDisplay/app.log")
    func write(_ text: String) {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize, size > 512_000 {
                let backup = url.appendingPathExtension("1"); try? FileManager.default.removeItem(at: backup); try FileManager.default.moveItem(at: url, to: backup)
            }
            if !FileManager.default.fileExists(atPath: url.path) { FileManager.default.createFile(atPath: url.path, contents: nil) }
            let handle = try FileHandle(forWritingTo: url); defer { try? handle.close() }
            try handle.seekToEnd(); try handle.write(contentsOf: Data("\(ISO8601DateFormatter().string(from: Date())) \(text)\n".utf8))
        } catch { NSLog("Portkey log: %@", error.localizedDescription) }
    }
}
