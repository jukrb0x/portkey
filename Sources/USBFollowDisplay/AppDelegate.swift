import AppKit
import FollowCore

@MainActor final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private var item: NSStatusItem!
    private var state: AppState!
    private var settings: SettingsWindow?
    func applicationDidFinishLaunching(_ notification: Notification) {
        if let id = Bundle.main.bundleIdentifier, let existing = NSRunningApplication.runningApplications(withBundleIdentifier: id).first(where: { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier }) {
            existing.activate(options: []); NSApp.terminate(nil); return
        }
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(systemSymbolName: "display", accessibilityDescription: "USB Follow Display")
        item.button?.imagePosition = .imageLeading
        let menu = NSMenu(); menu.delegate = self; item.menu = menu
        state = AppState(); state.onChange = { [weak self] in self?.updateTitle() }
        updateTitle()
        if CommandLine.arguments.contains("--settings") { openSettings() }
    }
    private func updateTitle() {
        item.button?.title = state.automatic ? (state.policy.ownership == .owned ? " M" : state.policy.ownership == .absent ? " ?" : " ·") : " ⏸"
        item.button?.toolTip = state.status
    }
    func menuWillOpen(_ menu: NSMenu) {
        menu.removeAllItems()
        func label(_ text: String) { let entry = NSMenuItem(title: text, action: nil, keyEquivalent: ""); entry.isEnabled = false; menu.addItem(entry) }
        @discardableResult func action(_ text: String, _ selector: Selector, key: String = "") -> NSMenuItem {
            let entry = NSMenuItem(title: text, action: selector, keyEquivalent: key); entry.target = self; menu.addItem(entry); return entry
        }
        label("USB Follow Display")
        label(String(state.status.prefix(110)))
        let ownership: [Ownership: String] = [.owned: "Mac（双 Hub 齐全）", .absent: "双 Hub 不在 Mac", .partial: "仅部分 Hub 连接", .unknown: "未知 / 读取失败"]
        label("USB：" + (ownership[state.policy.ownership] ?? "未知"))
        if let result = state.scheduler.result { label("输入：" + result.description) } else { label("输入：尚未确认") }
        menu.addItem(.separator())
        action("自动切换", #selector(toggle)).state = state.automatic ? .on : .off
        action("切到 Mac（HDMI 1）", #selector(toMac))
        action("切到 Windows（DisplayPort）", #selector(toWindows))
        menu.addItem(.separator())
        action("设置 / 重新绑定…", #selector(openSettings), key: ",")
        action("查看日志", #selector(showLogs))
        action("退出 USB Follow Display", #selector(quit), key: "q")
    }
    @objc private func toggle() { state.toggleAutomatic() }
    @objc private func toMac() { state.manual(.mac) }
    @objc private func toWindows() { state.manual(.windows) }
    @objc private func openSettings() { if settings == nil { settings = SettingsWindow(state: state) }; settings?.show() }
    @objc private func showLogs() { NSWorkspace.shared.open(state.log.url) }
    @objc private func quit() { NSApp.terminate(nil) }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard state != nil else { return .terminateNow }
        Task { await state.stop(); sender.reply(toApplicationShouldTerminate: true) }
        return .terminateLater
    }
}
