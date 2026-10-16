import AppKit
import FollowCore

@MainActor final class SettingsWindow: NSObject {
    private let state: AppState
    private let showMenu: (NSView) -> Void
    private let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 650, height: 490), styleMask: [.titled, .closable], backing: .buffered, defer: false)
    private let monitor = NSTextField()
    private let usb2 = NSPopUpButton(), usb3 = NSPopUpButton()
    private let macInput = NSTextField(), windowsInput = NSTextField()
    private let detected = NSTextField(wrappingLabelWithString: "")
    private let message = NSTextField(wrappingLabelWithString: "")
    private var choices: [USBDevice] = []
    init(state: AppState, showMenu: @escaping (NSView) -> Void) {
        self.state = state; self.showMenu = showMenu; super.init()
        window.title = "Portkey — 设置 / 重新绑定"; window.isReleasedWhenClosed = false
        let stack = NSStackView(); stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 13
        stack.translatesAutoresizingMaskIntoConstraints = false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: window.contentView!.leadingAnchor, constant: 22), stack.trailingAnchor.constraint(equalTo: window.contentView!.trailingAnchor, constant: -22), stack.topAnchor.constraint(equalTo: window.contentView!.topAnchor, constant: 20)])
        func field(_ label: String, _ view: NSView) {
            let row = NSStackView(views: [NSTextField(labelWithString: label), view]); row.spacing = 12; row.alignment = .centerY
            row.arrangedSubviews[0].widthAnchor.constraint(equalToConstant: 140).isActive = true
            stack.addArrangedSubview(row); row.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        detected.font = .systemFont(ofSize: 11); stack.addArrangedSubview(detected)
        field("显示器系统 UUID", monitor); monitor.setAccessibilityLabel("显示器系统 UUID")
        field("共享 USB2 Hub", usb2); usb2.setAccessibilityLabel("共享 USB2 Hub")
        field("共享 USB3 Hub", usb3); usb3.setAccessibilityLabel("共享 USB3 Hub")
        field("Mac 输入（HDMI1）", macInput); macInput.setAccessibilityLabel("Mac 输入")
        field("Windows 输入（DP）", windowsInput); windowsInput.setAccessibilityLabel("Windows 输入")
        let note = NSTextField(wrappingLabelWithString: "重新绑定时先把 USB 切到 Mac。按 location 和父 Hub 选择共享分支；上游同型号 Hub 不能替代。更换 Mac 端口或扩展坞后需重新绑定。\n完整拔掉 / 断电 USB 切换器也会被识别为离开 Mac。首次支持两台主机保持唤醒。")
        note.font = .systemFont(ofSize: 12); stack.addArrangedSubview(note); note.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        let refresh = NSButton(title: "重新扫描 / 撤销编辑", target: self, action: #selector(reload))
        let save = NSButton(title: "保存绑定", target: self, action: #selector(save)); save.keyEquivalent = "\r"
        let menu = NSButton(title: "打开状态菜单", target: self, action: #selector(openMenu(_:)))
        window.initialFirstResponder = refresh
        stack.addArrangedSubview(NSStackView(views: [refresh, save, menu])); stack.addArrangedSubview(message)
        message.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        reload(); window.center()
    }
    func show() { reload(); NSApp.activate(ignoringOtherApps: true); window.makeKeyAndOrderFront(nil) }
    @objc private func openMenu(_ sender: NSButton) { showMenu(sender) }
    @objc private func reload() {
        state.reconcile()
        let config = state.configuration ?? .qualified
        monitor.stringValue = config.monitor; macInput.stringValue = String(config.macInput); windowsInput.stringValue = String(config.windowsInput)
        choices = state.inventory?.hubs ?? []
        for hub in [config.usb2, config.usb3] where !choices.contains(hub) { choices.append(hub) }
        for popup in [usb2, usb3] {
            popup.removeAllItems()
            for hub in choices { popup.addItem(withTitle: hub.label + ((state.inventory?.hubs.contains(hub) ?? false) ? "" : "（未连接 / 已保存）")) }
        }
        usb2.selectItem(at: choices.firstIndex(of: config.usb2) ?? 0); usb3.selectItem(at: choices.firstIndex(of: config.usb3) ?? 0)
        detected.stringValue = "在线显示器：\n" + DisplayCatalog.online().map { "\($0.name) — \($0.uuid)" }.joined(separator: "\n") + "\n后端：Contents/Helpers/m1ddc · 固定版本 04d9497"
        message.stringValue = "配置：\(ConfigurationStore.standard.url.path)"
    }
    @objc private func save() {
        do {
            guard choices.indices.contains(usb2.indexOfSelectedItem), choices.indices.contains(usb3.indexOfSelectedItem),
                  let mac = UInt16(macInput.stringValue), let windows = UInt16(windowsInput.stringValue) else { throw ConfigError.invalid("请选择两个 Hub，并填写有效输入数值。") }
            var config = state.configuration ?? .qualified
            config.monitor = monitor.stringValue.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            config.usb2 = choices[usb2.indexOfSelectedItem]; config.usb3 = choices[usb3.indexOfSelectedItem]
            config.macInput = mac; config.windowsInput = windows
            try state.save(config); message.stringValue = "已保存。\(state.status)"
        } catch { message.stringValue = error.localizedDescription }
    }
}
