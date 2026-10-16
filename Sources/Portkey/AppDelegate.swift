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
    private let ownershipLabels: [Ownership: String] = [.owned: "Mac（双 Hub 齐全）", .absent: "双 Hub 不在 Mac", .partial: "仅部分 Hub 连接", .unknown: "未知 / 读取失败"]
    func applicationDidFinishLaunching(_ notification: Notification) {
        if let id = Bundle.main.bundleIdentifier, let existing = NSRunningApplication.runningApplications(withBundleIdentifier: id).first(where: { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier }) {
            existing.activate(options: []); NSApp.terminate(nil); return
        }
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(systemSymbolName: "display", accessibilityDescription: "Portkey")
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
        usbLine?.title = "USB：" + (ownershipLabels[state.policy.ownership] ?? "未知")
        inputLine?.title = "输入：" + (state.scheduler.result?.description ?? "尚未确认")
        automaticItem?.state = state.automatic ? .on : .off
    }
    func menuWillOpen(_ menu: NSMenu) {
        menu.removeAllItems()
        @discardableResult func label(_ text: String) -> NSMenuItem { let entry = NSMenuItem(title: text, action: nil, keyEquivalent: ""); entry.isEnabled = false; menu.addItem(entry); return entry }
        @discardableResult func action(_ text: String, _ selector: Selector, key: String = "") -> NSMenuItem {
            let entry = NSMenuItem(title: text, action: selector, keyEquivalent: key); entry.target = self; menu.addItem(entry); return entry
        }
        label("Portkey")
        statusLine = label(""); usbLine = label(""); inputLine = label("")
        menu.addItem(.separator())
        automaticItem = action("自动切换", #selector(toggle))
        action("切到 Mac（HDMI 1）", #selector(toMac))
        action("切到 Windows（DisplayPort）", #selector(toWindows))
        menu.addItem(.separator())
        action("设置 / 重新绑定…", #selector(openSettings), key: ",")
        action("查看日志", #selector(showLogs))
        action("退出 Portkey", #selector(quit), key: "q")
        updateTitle()
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
