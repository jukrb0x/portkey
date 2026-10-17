import AppKit
import SwiftUI

private final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    let model = PortKeyStore()
    private var statusItem: NSStatusItem!
    private var window: NSWindow!
    private var statusMenu: StatusMenu!
    private var menuLanguage = ""

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        makeWindow()
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        statusItem.isVisible = true
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.imageScaling = .scaleProportionallyDown
        statusItem.button?.image = StatusItemAppearance.image()
        statusMenu = StatusMenu(model: model)
        statusItem.menu = statusMenu.menu
        model.openWindow = { [weak self] in self?.openSettings() }
        model.onUpdate = { [weak self] in self?.updateChrome() }
        // The Rust lock protects concurrent native instances. Historical Swift predates that lock.
        let legacy = NSWorkspace.shared.runningApplications.contains {
            $0.processIdentifier != ProcessInfo.processInfo.processIdentifier && $0.bundleIdentifier == "local.portkey.app"
        }
        if legacy {
            model.bridgeFailure = model.text("The historical PortKey app is already running. Quit that instance before starting this controller; your configuration has not been opened.", "旧版 PortKey 已在运行。请先退出旧版再启动此控制核心；尚未打开你的配置。")
            updateChrome(); openSettings(); return
        }
        model.start()
        updateChrome()
        openSettings()
    }
    private func makeWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1020, height: 770), styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.title = "PortKey"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.toolbarStyle = .unified
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 930, height: 720)
        window.contentView = NSHostingView(rootView: ProductRoot(model: model))
        window.center()
    }
    private func updateChrome() {
        let state = model.bridgeFailure ?? model.effectiveTitle
        let detail = model.snapshot?.status ?? ""
        statusItem?.button?.toolTip = "PortKey\n" + state + "\n" + model.usbTitle + "\n" + model.inputSubtitle + (detail.isEmpty ? "" : "\n" + detail)
        statusItem?.button?.setAccessibilityLabel("PortKey, " + state + ", " + model.usbTitle)
        window?.subtitle = model.localName
        if menuLanguage != model.language { menuLanguage = model.language; makeMainMenu() }
    }
    private func makeMainMenu() {
        let main = NSMenu()
        let appItem = NSMenuItem(); main.addItem(appItem)
        let appMenu = NSMenu(title: "PortKey"); appItem.submenu = appMenu
        let settings = NSMenuItem(title: model.text("Open PortKey…", "打开 PortKey…"), action: #selector(openSettings), keyEquivalent: ",")
        settings.target = self; appMenu.addItem(settings); appMenu.addItem(.separator())
        let quit = NSMenuItem(title: model.text("Quit PortKey", "退出 PortKey"), action: #selector(quit), keyEquivalent: "q")
        quit.target = self; appMenu.addItem(quit)
        let editItem = NSMenuItem(title: model.text("Edit", "编辑"), action: nil, keyEquivalent: ""); main.addItem(editItem)
        let editMenu = NSMenu(title: editItem.title); editItem.submenu = editMenu
        for (en, zh, selector, key) in [("Undo", "撤销", "undo:", "z"), ("Redo", "重做", "redo:", "Z"), ("Cut", "剪切", "cut:", "x"), ("Copy", "复制", "copy:", "c"), ("Paste", "粘贴", "paste:", "v"), ("Select All", "全选", "selectAll:", "a")] {
            editMenu.addItem(NSMenuItem(title: model.text(en, zh), action: NSSelectorFromString(selector), keyEquivalent: key))
        }
        NSApp.mainMenu = main
    }
    @objc func openSettings() {
        NSApp.setActivationPolicy(.regular)
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
    @objc private func quit() { NSApp.terminate(nil) }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool { openSettings(); return true }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        sender.orderOut(nil)
        if sender === window { NSApp.setActivationPolicy(.accessory) }
        return false
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationWillTerminate(_ notification: Notification) { model.stop() }
}

// Diagnostics intentionally avoid creating a controller or enabling saved automation.
if CommandLine.arguments.contains("--diagnose") {
    let process = Process()
    process.executableURL = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/portkey-cli")
    process.arguments = ["--diagnose"]
    do { try process.run(); process.waitUntilExit(); exit(process.terminationStatus) }
    catch { FileHandle.standardError.write(Data("Cannot run bundled diagnostics: \(error)\n".utf8)); exit(1) }
}
let application = NSApplication.shared
private let delegate = AppDelegate()
application.delegate = delegate
application.run()
