import AppKit
import FollowCore

@main struct Main {
    @MainActor static func main() {
        if let existing = NSRunningApplication.runningApplications(withBundleIdentifier: AppDataMigration.legacyBundleIdentifier).first {
            existing.activate(options: [])
            fputs("Quit the previous app before launching PortKey to migrate its data.\n", stderr)
            return
        }
        do { try AppDataMigration.run() }
        catch {
            fputs("PortKey could not migrate app data: \(error.localizedDescription)\n", stderr)
            exit(1)
        }
        if CommandLine.arguments.contains("--diagnose") {
            do {
                let inventory = try USBMonitor.snapshot(); let config = try ConfigurationStore.standard.load()
                print("Ownership: \(config.observe(inventory.devices).rawValue)")
                print("Hubs:"); for hub in inventory.hubs { print(hub.label) }
                print("Displays:"); for display in DisplayCatalog.online() { print("\(display.name) \(display.uuid)") }
            } catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
            return
        }
        let app = NSApplication.shared; app.setActivationPolicy(.accessory)
        let delegate = AppDelegate(); app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
