import AppKit


private struct Device: Codable, Equatable { var id: String; var label: String }
private struct Configuration: Codable, Equatable {
    var version = 1
    var platform = "macos"
    var display_id = ""
    var local_input = 17
    var remote_input = 15
    var hubs: [Device] = []
    var automatic = false
    var language = "en"
}
private struct Inventory: Decodable, Equatable { var displays: [Device]; var hubs: [Device] }
private struct SavedProfile: Decodable { var name: String; var display_id: String }
private struct Snapshot: Decodable {
    var profiles: [SavedProfile]
    var active_display_id: String?
    var profile_name: String
    var config: Configuration?
    var inventory: Inventory?
    var ownership: String
    var status: String
    var error: Bool
    var lifecycle_ok: Bool
    var sleeping: Bool
    var language: String
    var config_path: String
    var editing: Bool
}
private final class SettingsDocument: NSView { override var isFlipped: Bool { true } }
private enum BridgeError: Error { case unavailable, invalidResponse }

private final class AppDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    var handle: UnsafeMutableRawPointer?
    var timer: Timer?
    var snapshot: Snapshot?
    var draft = Configuration()
    var initialized = false
    var bridgeFailure: String?
    var language = "en"
    var statusItem: NSStatusItem!
    var window: NSWindow!
    let status = NSTextField(wrappingLabelWithString: "")
    let profileStatus = NSTextField(wrappingLabelWithString: "")
    let profileName = NSTextField(string: "")
    let ownership = NSTextField(labelWithString: "")
    let instructions = NSTextField(wrappingLabelWithString: "")
    let inputHelp = NSTextField(wrappingLabelWithString: "")
    let footer = NSTextField(wrappingLabelWithString: "")
    let path = NSTextField(wrappingLabelWithString: "")
    let display = NSPopUpButton()
    let hub1 = NSPopUpButton()
    let hub2 = NSPopUpButton()
    let localInput = NSTextField(string: "17")
    let remoteInput = NSTextField(string: "15")
    let languagePicker = NSPopUpButton()
    let automatic = NSButton(checkboxWithTitle: "", target: nil, action: nil)
    var labels: [(NSTextField, String, String)] = []
    var buttons: [(NSButton, String, String)] = []
    var saveButton: NSButton!
    var cancelButton: NSButton!
    var editButton: NSButton!
    var newButton: NSButton!
    var importButton: NSButton!
    var localButton: NSButton!
    var remoteButton: NSButton!
    var displayChoices: [Device] = []
    var hubChoices: [Device] = []
    var selectedHub1: Device?
    var selectedHub2: Device?
    var menuState = ""
    var mainMenuLanguage = ""
    var operationStatus = ""

    func text(_ en: String, _ zh: String) -> String { language == "zh-Hans" ? zh : en }
    func decode<T: Decodable>(_ pointer: UnsafeMutablePointer<CChar>?, as type: T.Type) throws -> T {
        guard let pointer else { throw BridgeError.unavailable }
        defer { pk_string_free(pointer) }
        guard let data = String(validatingUTF8: pointer)?.data(using: .utf8) else { throw BridgeError.invalidResponse }
        return try JSONDecoder().decode(type, from: data)
    }
    @discardableResult func action(_ value: [String: Any]) -> [String: Any]? {
        guard let handle, bridgeFailure == nil || value["type"] as? String == "quit" else { return nil }
        do {
            let bytes = try JSONSerialization.data(withJSONObject: value)
            let json = String(decoding: bytes, as: UTF8.self)
            guard let pointer = json.withCString({ pk_action(handle, $0) }) else { throw BridgeError.unavailable }
            defer { pk_string_free(pointer) }
            guard let data = String(validatingUTF8: pointer)?.data(using: .utf8),
                  let reply = try JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw BridgeError.invalidResponse }
            guard reply["ok"] as? Bool == true else {
                showError(reply["error"] as? String ?? text("Core action failed.", "核心操作失败。")); return nil
            }
            return reply
        } catch { failBridge(error); return nil }
    }
    func showError(_ message: String) {
        let alert = NSAlert(); alert.messageText = "PortKey"; alert.informativeText = message
        alert.alertStyle = .warning
        if window.isVisible { alert.beginSheetModal(for: window) } else { alert.runModal() }
    }
    func failBridge(_ error: Error) {
        guard bridgeFailure == nil else { return }
        bridgeFailure = text("The Rust core is unavailable. Restart PortKey. ", "Rust 核心不可用，请重启 PortKey。") + String(describing: error)
        timer?.invalidate(); updateUI(); showError(bridgeFailure!)
    }
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        makeWindow()
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.isVisible = true
        statusItem.button?.imagePosition = .imageOnly
        statusItem.button?.title = ""
        handle = pk_create()
        guard handle != nil else { failBridge(BridgeError.unavailable); openSettings(); return }
        tick()
        let clock = Timer(timeInterval: 0.2, target: self, selector: #selector(tick), userInfo: nil, repeats: true)
        timer = clock
        RunLoop.main.add(clock, forMode: .common)
        if snapshot?.config == nil { openSettings() }
    }
    @objc func tick() {
        guard let handle, bridgeFailure == nil else { return }
        do {
            let next = try decode(pk_snapshot(handle), as: Snapshot.self)
            let inventoryChanged = snapshot?.inventory != next.inventory
            let languageChanged = language != next.language
            let reloadDraft = !initialized || (!next.editing &&
                (snapshot?.editing == true || snapshot?.config != next.config ||
                 snapshot?.active_display_id != next.active_display_id || snapshot?.profile_name != next.profile_name))
            snapshot = next; language = next.language
            if reloadDraft {
                loadDraft(next.config ?? Configuration()); profileName.stringValue = next.profile_name
                initialized = true
            }
            if inventoryChanged || languageChanged { refreshChoices() }
            updateUI()
        } catch { failBridge(error) }
    }
    func loadDraft(_ config: Configuration) {
        draft = config
        selectedHub1 = config.hubs.first
        selectedHub2 = config.hubs.count > 1 ? config.hubs[1] : nil
        localInput.stringValue = String(config.local_input)
        remoteInput.stringValue = String(config.remote_input)
        refreshChoices()
    }
    func makeWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 700), styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "PortKey"; window.delegate = self; window.isReleasedWhenClosed = false; window.center()
        let stack = NSStackView(); stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 12
        stack.translatesAutoresizingMaskIntoConstraints = false
        let scroll = NSScrollView(); scroll.hasVerticalScroller = true; scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false
        let document = SettingsDocument(); document.translatesAutoresizingMaskIntoConstraints = false
        scroll.documentView = document; window.contentView!.addSubview(scroll); document.addSubview(stack)
        NSLayoutConstraint.activate([
            scroll.leadingAnchor.constraint(equalTo: window.contentView!.leadingAnchor),
            scroll.trailingAnchor.constraint(equalTo: window.contentView!.trailingAnchor),
            scroll.topAnchor.constraint(equalTo: window.contentView!.topAnchor),
            scroll.bottomAnchor.constraint(equalTo: window.contentView!.bottomAnchor),
            document.widthAnchor.constraint(equalTo: scroll.contentView.widthAnchor),
            stack.leadingAnchor.constraint(equalTo: document.leadingAnchor, constant: 24),
            stack.trailingAnchor.constraint(equalTo: document.trailingAnchor, constant: -24),
            stack.topAnchor.constraint(equalTo: document.topAnchor, constant: 24),
            stack.bottomAnchor.constraint(equalTo: document.bottomAnchor, constant: -24)
        ])
        func add(_ view: NSView) { stack.addArrangedSubview(view); view.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true }
        func row(_ views: [NSView]) -> NSStackView { let r = NSStackView(views: views); r.orientation = .horizontal; r.spacing = 10; return r }
        func label(_ en: String, _ zh: String) -> NSTextField {
            let l = NSTextField(labelWithString: en); l.widthAnchor.constraint(equalToConstant: 115).isActive = true
            labels.append((l, en, zh)); return l
        }
        func button(_ en: String, _ zh: String, _ selector: Selector) -> NSButton {
            let b = NSButton(title: en, target: self, action: selector); buttons.append((b, en, zh)); return b
        }
        func picker(_ control: NSPopUpButton, _ en: String, _ zh: String) {
            control.target = self; control.action = #selector(selectionChanged)
            control.setContentHuggingPriority(.defaultLow, for: .horizontal)
            control.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
            control.cell?.lineBreakMode = .byTruncatingMiddle
            add(row([label(en, zh), control]))
        }
        let title = NSTextField(labelWithString: "PortKey"); title.font = .boldSystemFont(ofSize: 24)
        languagePicker.addItems(withTitles: ["English", "简体中文"]); languagePicker.target = self; languagePicker.action = #selector(changeLanguage)
        add(row([title, NSView(), languagePicker]))
        ownership.font = .boldSystemFont(ofSize: 14); add(ownership); add(profileStatus); add(status)
        automatic.target = self; automatic.action = #selector(toggleAutomation); add(automatic)
        localButton = button("Switch to Mac", "切换到 Mac", #selector(useLocal))
        remoteButton = button("Switch to Windows", "切换到 Windows", #selector(useRemote))
        add(row([localButton, remoteButton]))
        add(NSBox.separator())
        add(instructions)
        add(row([label("Setup name", "配置名称"), profileName]))
        picker(display, "Monitor", "显示器")
        picker(hub1, "USB hub 1", "USB 集线器 1")
        picker(hub2, "USB hub 2", "USB 集线器 2")
        add(inputHelp)
        localInput.widthAnchor.constraint(equalToConstant: 65).isActive = true
        remoteInput.widthAnchor.constraint(equalToConstant: 65).isActive = true
        add(row([label("Mac input", "Mac 输入值"), localInput, label("Windows input", "Windows 输入值"), remoteInput]))
        localInput.setAccessibilityLabel("Mac VCP input / Mac VCP 输入值")
        remoteInput.setAccessibilityLabel("Windows VCP input / Windows VCP 输入值")
        saveButton = button("Save enrollment", "保存设备配置", #selector(save))
        cancelButton = button("Cancel editing", "取消编辑", #selector(cancel))
        editButton = button("Edit setup", "修改配置", #selector(edit))
        let refresh = button("Refresh devices", "刷新设备", #selector(refresh))
        newButton = button("New setup", "新增配置", #selector(newSetup))
        add(row([editButton, newButton, saveButton, cancelButton]))
        add(row([refresh]))
        importButton = button("Import Swift setup…", "导入 Swift 配置…", #selector(importSetup)); add(row([importButton]))
        add(NSBox.separator()); footer.font = .systemFont(ofSize: 11); add(footer)
        path.font = .systemFont(ofSize: 10); path.textColor = .secondaryLabelColor; path.isSelectable = true; add(path)
    }
    func refreshChoices() {
        displayChoices = snapshot?.inventory?.displays ?? []
        if !draft.display_id.isEmpty && !displayChoices.contains(where: { $0.id == draft.display_id }) {
            displayChoices.append(Device(id: draft.display_id, label: text("Saved monitor (offline)", "已保存的显示器（离线）")))
        }
        hubChoices = snapshot?.inventory?.hubs ?? []
        for device in [selectedHub1, selectedHub2].compactMap({ $0 }) where !hubChoices.contains(where: { $0.id == device.id }) {
            hubChoices.append(Device(id: device.id, label: device.label + text(" (offline)", "（离线）")))
        }
        func populate(_ popup: NSPopUpButton, _ choices: [Device], _ selected: String?) {
            popup.removeAllItems(); popup.addItem(withTitle: text("Choose explicitly…", "请明确选择…"))
            for device in choices {
                // Device labels include the USB location; the tooltip retains full topology.
                let title = device.label.count > 72 ? String(device.label.prefix(44)) + "…" + String(device.label.suffix(27)) : device.label
                let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
                item.toolTip = "\(device.label)\n\(device.id)"; popup.menu?.addItem(item)
            }
            popup.selectItem(at: choices.firstIndex(where: { $0.id == selected }).map { $0 + 1 } ?? 0)
            popup.toolTip = selected
        }
        populate(display, displayChoices, draft.display_id)
        populate(hub1, hubChoices, selectedHub1?.id); populate(hub2, hubChoices, selectedHub2?.id)
    }
    func updateUI() {
        let editing = snapshot?.editing ?? true
        let healthy = bridgeFailure == nil && handle != nil
        let canSwitch = healthy && snapshot?.config != nil && !editing && snapshot?.lifecycle_ok == true && snapshot?.sleeping == false
        for (label, en, zh) in labels { label.stringValue = text(en, zh) }
        for (button, en, zh) in buttons { button.title = text(en, zh) }
        instructions.stringValue = text("Enroll the monitor and the two hubs that disappear together when USB switches hosts. Paths distinguish identical hubs. Editing pauses switching until you save or cancel.", "请选择目标显示器和 USB 切换主机时同时消失的两个集线器。设备路径区分同型号集线器。编辑期间暂停切换，直到保存或取消。")
        inputHelp.stringValue = text("VCP input values: DisplayPort 15/16, HDMI 17/18; USB-C varies by model. Enter distinct values from 1–255.", "VCP 输入值：DisplayPort 15/16、HDMI 17/18；USB-C 取决于型号。请输入 1–255 内不同的数值。")
        footer.stringValue = text("Keep both computers awake. Enable automation on only one host and DDC/CI in the monitor menu. Closing this window keeps PortKey in the menu bar.", "两台电脑需保持唤醒，仅在一台主机启用自动切换，并在显示器菜单开启 DDC/CI。关闭窗口后 PortKey 继续在菜单栏运行。")
        path.stringValue = text("Configuration: ", "配置文件：") + (snapshot?.config_path ?? "")
        let current = snapshot?.active_display_id == nil ? text("No matching display", "没有匹配的显示器") : (snapshot?.profile_name ?? "")
        let names = snapshot?.profiles.map { $0.name }.joined(separator: " · ") ?? ""
        profileStatus.stringValue = text("Current setup: ", "当前配置：") + current + "\n" + text("Saved setups: ", "已保存配置：") + (names.isEmpty ? text("None", "无") : names)
        profileName.placeholderString = text("e.g. Home or Work", "例如：家里、公司")
        profileName.setAccessibilityLabel(text("Setup name", "配置名称"))
        languagePicker.selectItem(at: language == "zh-Hans" ? 1 : 0)
        display.setAccessibilityLabel(text("Monitor", "显示器"))
        hub1.setAccessibilityLabel(text("USB hub 1", "USB 集线器 1"))
        hub2.setAccessibilityLabel(text("USB hub 2", "USB 集线器 2"))
        languagePicker.setAccessibilityLabel(text("Language", "语言"))
        automatic.title = text("Automatic switching", "自动切换")
        automatic.state = snapshot?.config?.automatic == true ? .on : .off
        automatic.isEnabled = canSwitch
        for control in [display, hub1, hub2] { control.isEnabled = healthy && editing }
        localInput.isEnabled = healthy && editing; remoteInput.isEnabled = healthy && editing
        profileName.isEnabled = healthy && editing
        saveButton.isEnabled = healthy && editing
        cancelButton.isEnabled = healthy && editing && !(snapshot?.profiles.isEmpty ?? true)
        editButton.isEnabled = healthy && !editing && snapshot?.config != nil
        newButton.isEnabled = healthy && !editing
        importButton.isEnabled = healthy && editing
        localButton.isEnabled = canSwitch; remoteButton.isEnabled = canSwitch
        let states = ["owned": text("USB connected to Mac", "USB 已连接 Mac"), "absent": text("Enrolled USB hubs are absent", "已配置的 USB 集线器均未连接 Mac"), "partial": text("Only one hub detected · waiting", "只检测到一个集线器 · 等待稳定"), "unknown": text("Waiting for reliable USB evidence", "等待可靠的 USB 状态")]
        ownership.stringValue = states[snapshot?.ownership ?? "unknown"] ?? states["unknown"]!
        let failed = bridgeFailure != nil || snapshot?.error == true
        if !healthy || snapshot?.lifecycle_ok == false {
            operationStatus = text("Switching unavailable", "切换不可用")
        } else if snapshot?.sleeping == true {
            operationStatus = text("Sleeping · switching paused", "休眠中 · 切换已暂停")
        } else if editing {
            operationStatus = text("Editing setup · switching paused", "正在编辑配置 · 切换已暂停")
        } else if snapshot?.config == nil {
            operationStatus = text("No matching setup · switching paused", "没有匹配的配置 · 切换已暂停")
        } else {
            operationStatus = automatic.state == .on ? text("Automatic switching enabled", "自动切换已启用") : text("Automatic switching paused", "自动切换已暂停")
        }
        let detail = bridgeFailure ?? snapshot?.status ?? ""
        status.stringValue = detail.isEmpty || detail == operationStatus ? operationStatus : operationStatus + "\n" + detail
        status.textColor = failed ? .systemRed : .secondaryLabelColor
        updateStatusItem(failed: failed, switching: canSwitch && automatic.state == .on)
        updateMainMenu()
        updateMenu(canSwitch: canSwitch)
    }
    func updateStatusItem(failed: Bool, switching: Bool) {
        guard let button = statusItem?.button else { return }
        button.title = ""
        let ownershipSymbols = ["owned": "laptopcomputer", "absent": "desktopcomputer"]
        let symbol = failed ? "exclamationmark.triangle" : (!switching ? "pause.circle" : (ownershipSymbols[snapshot?.ownership ?? "unknown"] ?? "questionmark.circle"))
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "PortKey")
        image?.isTemplate = true; button.image = image
        let evidence = text("USB ownership does not confirm the monitor input or picture. The desktop icon means both enrolled USB hubs are absent from Mac; it does not confirm Windows is awake.", "USB 归属不代表已确认显示器输入或画面。台式机图标表示两个已配置 USB 集线器均未连接 Mac，不代表 Windows 已唤醒。")
        button.toolTip = "PortKey\n" + ownership.stringValue + "\n" + status.stringValue + "\n" + evidence
        button.setAccessibilityLabel("PortKey, " + ownership.stringValue + ", " + status.stringValue)
    }
    func updateMainMenu() {
        guard mainMenuLanguage != language else { return }
        mainMenuLanguage = language
        let main = NSMenu()
        let applicationItem = NSMenuItem(); main.addItem(applicationItem)
        let applicationMenu = NSMenu(title: "PortKey"); applicationItem.submenu = applicationMenu
        let settings = NSMenuItem(title: text("Settings…", "设置…"), action: #selector(openSettings), keyEquivalent: ",")
        settings.target = self; applicationMenu.addItem(settings)
        applicationMenu.addItem(.separator())
        let quit = NSMenuItem(title: text("Quit PortKey", "退出 PortKey"), action: #selector(quit), keyEquivalent: "q")
        quit.target = self; applicationMenu.addItem(quit)
        let editItem = NSMenuItem(title: text("Edit", "编辑"), action: nil, keyEquivalent: ""); main.addItem(editItem)
        let editMenu = NSMenu(title: text("Edit", "编辑")); editItem.submenu = editMenu
        let commands: [(String, String, String, String)] = [
            ("Undo", "撤销", "undo:", "z"), ("Redo", "重做", "redo:", "Z"),
            ("Cut", "剪切", "cut:", "x"), ("Copy", "复制", "copy:", "c"),
            ("Paste", "粘贴", "paste:", "v"), ("Select All", "全选", "selectAll:", "a")
        ]
        for (en, zh, selector, key) in commands {
            let item = NSMenuItem(title: text(en, zh), action: NSSelectorFromString(selector), keyEquivalent: key)
            // A nil target routes native editing commands to the current field editor.
            editMenu.addItem(item)
        }
        NSApp.mainMenu = main
    }
    func updateMenu(canSwitch: Bool) {
        guard statusItem != nil else { return }
        let state = "\(language)|\(ownership.stringValue)|\(canSwitch)|\(automatic.state.rawValue)|\(status.stringValue)"
        guard state != menuState else { return }
        menuState = state
        let menu = NSMenu(); menu.autoenablesItems = false
        func item(_ title: String, _ selector: Selector?, enabled: Bool = true) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: selector, keyEquivalent: ""); i.target = self; i.isEnabled = enabled; menu.addItem(i); return i
        }
        _ = item("PortKey", nil, enabled: false)
        _ = item(ownership.stringValue, nil, enabled: false)
        _ = item(operationStatus, nil, enabled: false)
        let detail = bridgeFailure ?? snapshot?.status ?? ""
        if !detail.isEmpty && detail != operationStatus {
            let line = item(String(detail.prefix(100)) + (detail.count > 100 ? "…" : ""), nil, enabled: false)
            line.toolTip = detail
        }
        _ = item(text("USB state does not confirm monitor input", "USB 状态不代表显示器输入已确认"), nil, enabled: false)
        menu.addItem(.separator())
        _ = item(text("Settings…", "设置…"), #selector(openSettings))
        menu.addItem(.separator())
        _ = item(text("Switch to Mac", "切换到 Mac"), #selector(useLocal), enabled: canSwitch)
        _ = item(text("Switch to Windows", "切换到 Windows"), #selector(useRemote), enabled: canSwitch)
        let toggle = item(text("Automatic switching", "自动切换"), #selector(toggleAutomation), enabled: canSwitch)
        toggle.state = automatic.state
        menu.addItem(.separator())
        let lang = item(text("Language", "语言"), nil)
        let submenu = NSMenu()
        for (index, title) in ["English", "简体中文"].enumerated() {
            let i = NSMenuItem(title: title, action: #selector(menuLanguage(_:)), keyEquivalent: ""); i.target = self; i.tag = index
            i.state = (index == (language == "zh-Hans" ? 1 : 0)) ? .on : .off; submenu.addItem(i)
        }
        lang.submenu = submenu
        _ = item(text("Quit PortKey", "退出 PortKey"), #selector(quit))
        statusItem.menu = menu
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openSettings()
        return true
    }
    @objc func openSettings() { window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true) }
    func windowShouldClose(_ sender: NSWindow) -> Bool { sender.orderOut(nil); return false }
    @objc func selectionChanged() {
        draft.display_id = display.indexOfSelectedItem > 0 ? displayChoices[display.indexOfSelectedItem - 1].id : ""
        selectedHub1 = hub1.indexOfSelectedItem > 0 ? hubChoices[hub1.indexOfSelectedItem - 1] : nil
        selectedHub2 = hub2.indexOfSelectedItem > 0 ? hubChoices[hub2.indexOfSelectedItem - 1] : nil
        display.toolTip = draft.display_id; hub1.toolTip = selectedHub1?.id; hub2.toolTip = selectedHub2?.id
    }
    @objc func edit() { if action(["type": "edit", "enabled": true]) != nil { if let config = snapshot?.config { loadDraft(config) }; tick() } }
    @objc func cancel() { if action(["type": "edit", "enabled": false]) != nil { tick() } }
    @objc func newSetup() {
        if action(["type": "new"]) != nil {
            loadDraft(Configuration()); profileName.stringValue = ""; tick()
        }
    }
    @objc func refresh() { _ = action(["type": "refresh"]); tick() }
    @objc func save() {
        guard let local = Int(localInput.stringValue), let remote = Int(remoteInput.stringValue), (1...255).contains(local), (1...255).contains(remote), local != remote else {
            showError(text("Enter two distinct input numbers from 1 to 255.", "请输入 1 至 255 之间两个不同的输入值。")); return
        }
        draft.local_input = local; draft.remote_input = remote
        draft.hubs = [selectedHub1, selectedHub2].compactMap { $0 }; draft.language = language
        do {
            let value = try JSONSerialization.jsonObject(with: JSONEncoder().encode(draft))
            if action(["type": "save", "config": value, "profile_name": profileName.stringValue]) != nil { tick() }
        } catch { showError(String(describing: error)) }
    }
    @objc func importSetup() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        panel.directoryURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/PortKey")
        panel.beginSheetModal(for: window) { [weak self] result in
            guard let self, result == .OK, let url = panel.url,
                  let reply = self.action(["type": "import", "path": url.path]), let config = reply["config"] else { return }
            do { self.loadDraft(try JSONDecoder().decode(Configuration.self, from: JSONSerialization.data(withJSONObject: config))); self.tick() }
            catch { self.showError(String(describing: error)) }
        }
    }
    @objc func useLocal() { _ = action(["type": "manual", "target": "local"]); tick() }
    @objc func useRemote() { _ = action(["type": "manual", "target": "remote"]); tick() }
    @objc func toggleAutomation() { _ = action(["type": "automatic", "enabled": !(snapshot?.config?.automatic ?? false)]); tick() }
    @objc func changeLanguage() { setLanguage(languagePicker.indexOfSelectedItem) }
    @objc func menuLanguage(_ sender: NSMenuItem) { setLanguage(sender.tag) }
    func setLanguage(_ index: Int) { _ = action(["type": "language", "value": index == 1 ? "zh-Hans" : "en"]); tick() }
    @objc func quit() { NSApp.terminate(nil) }
    func applicationWillTerminate(_ notification: Notification) {
        timer?.invalidate(); _ = action(["type": "quit"])
        if let handle { pk_destroy(handle); self.handle = nil }
    }
}
private extension NSBox {
    static func separator() -> NSBox { let box = NSBox(); box.boxType = .separator; return box }
}
// Diagnostics intentionally avoid creating a controller or enabling saved automation.
if CommandLine.arguments.contains("--diagnose") {
    let cli = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/portkey-cli")
    let process = Process(); process.executableURL = cli; process.arguments = ["--diagnose"]
    do { try process.run(); process.waitUntilExit(); exit(process.terminationStatus) }
    catch { FileHandle.standardError.write(Data("Cannot run bundled diagnostics: \(error)\n".utf8)); exit(1) }
}
let application = NSApplication.shared
private let delegate = AppDelegate()
application.delegate = delegate
application.run()
