import AppKit
import FollowCore

@MainActor final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var item: NSStatusItem!
    private var state: AppState!
    private var settings: SettingsWindow?
    private var statusLine: NSMenuItem?
    private var usbLine: NSMenuItem?
    private var inputLine: NSMenuItem?
    private var automaticItem: NSMenuItem?
    private let ownershipLabels: [Ownership: String] = [.owned: "Mac (both hubs present)", .absent: "Both hubs absent from Mac", .partial: "Only one hub connected", .unknown: "Unknown / read failed"]
    func applicationDidFinishLaunching(_ notification: Notification) {
        let identities = [Bundle.main.bundleIdentifier, AppDataMigration.legacyBundleIdentifier].compactMap { $0 }
        if let existing = identities.flatMap({ NSRunningApplication.runningApplications(withBundleIdentifier: $0) }).first(where: { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier }) {
            existing.activate(options: []); NSApp.terminate(nil); return
        }
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(systemSymbolName: "display", accessibilityDescription: "PortKey")
        item.button?.imagePosition = .imageLeading
        let menu = NSMenu(); menu.delegate = self; item.menu = menu
        state = AppState(); state.onChange = { [weak self] in self?.updateTitle() }
        updateTitle()
        if CommandLine.arguments.contains("--settings") { openSettings() }
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool { openSettings(); return true }
    private func updateTitle() {
        item.button?.title = state.automatic ? (state.policy.ownership == .owned ? " M" : state.policy.ownership == .absent ? " ?" : " ·") : " ⏸"
        item.button?.toolTip = state.status
        statusLine?.title = String(state.status.prefix(110))
        usbLine?.title = state.localizer.text("USB: %@", ownershipLabels[state.policy.ownership] ?? "Unknown / read failed")
        inputLine?.title = state.localizer.text("Input: %@", state.scheduler.result?.message.localized(using: state.localizer) ?? state.localizer.text("Not confirmed"))
        automaticItem?.state = state.automatic ? .on : .off
    }
    func menuWillOpen(_ menu: NSMenu) {
        menu.removeAllItems()
        let l = state.localizer
        @discardableResult func label(_ text: String) -> NSMenuItem { let entry = NSMenuItem(title: text, action: nil, keyEquivalent: ""); entry.isEnabled = false; menu.addItem(entry); return entry }
        @discardableResult func action(_ text: String, _ selector: Selector, key: String = "") -> NSMenuItem {
            let entry = NSMenuItem(title: text, action: selector, keyEquivalent: key); entry.target = self; menu.addItem(entry); return entry
        }
        label("PortKey")
        statusLine = label(""); usbLine = label(""); inputLine = label("")
        menu.addItem(.separator())
        automaticItem = action(l.text("Automatic switching"), #selector(toggle))
        action(l.text("Switch to Mac (HDMI 1)"), #selector(toMac))
        action(l.text("Switch to Windows (DisplayPort)"), #selector(toWindows))
        menu.addItem(.separator())
        action(l.text("Settings / Rebind…"), #selector(openSettings), key: ",")
        action(l.text("Show Logs"), #selector(showLogs))
        let languageItem = NSMenuItem(title: l.text("Language"), action: nil, keyEquivalent: "")
        let languages = NSMenu()
        for language in UILanguage.allCases {
            let title = language == .english ? "English" : Localizer(language: .simplifiedChinese).text("Chinese (Simplified)")
            let entry = NSMenuItem(title: title, action: #selector(changeLanguage(_:)), keyEquivalent: "")
            entry.representedObject = language.rawValue; entry.target = self; entry.state = state.language == language ? .on : .off
            languages.addItem(entry)
        }
        languageItem.submenu = languages; menu.addItem(languageItem)
        action(l.text("Quit PortKey"), #selector(quit), key: "q")
        updateTitle()
    }
    @objc private func changeLanguage(_ sender: NSMenuItem) {
        guard let code = sender.representedObject as? String, let language = UILanguage(rawValue: code) else { return }
        state.setLanguage(language); settings?.refreshLanguage()
        if let menu = item.menu { menuWillOpen(menu) }
    }
    @objc private func toggle() { state.toggleAutomatic() }
    @objc private func toMac() { state.manual(.mac) }
    @objc private func toWindows() { state.manual(.windows) }
    @objc private func openSettings() { if settings == nil { settings = SettingsWindow(state: state, showMenu: { [weak self] view in _ = self?.item.menu?.popUp(positioning: nil, at: NSPoint(x: 0, y: view.bounds.height), in: view) }) }; settings?.show() }
    @objc private func showLogs() { NSWorkspace.shared.open(state.log.url) }
    @objc private func quit() { NSApp.terminate(nil) }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard state != nil else { return .terminateNow }
        Task { await state.stop(); sender.reply(toApplicationShouldTerminate: true) }
        return .terminateLater
    }
}
