import AppKit

// Use the symbol's native alignment and aspect ratio; a square size override shrinks its optical bounds.
enum StatusItemAppearance {
    static func image() -> NSImage? {
        let image = NSImage(systemSymbolName: "arrow.left.arrow.right", accessibilityDescription: "PortKey")?
            .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 14, weight: .medium))
        image?.isTemplate = true
        return image
    }
}

final class StatusMenu: NSObject, NSMenuDelegate {
    let menu = NSMenu()
    private let model: PortKeyStore
    private let confirm: (String) -> Bool

    init(model: PortKeyStore, confirm: ((String) -> Bool)? = nil) {
        self.model = model
        self.confirm = confirm ?? { target in
            let alert = NSAlert()
            alert.messageText = model.text("Switch display to ", "将显示器切换到 ") + (target == "local" ? model.localName : model.remoteName)
            alert.informativeText = model.text("USB stays with the physical selector. This request holds locally until a genuine USB change. The other computer’s automatic rules are not paused.", "USB 仍由实体切换器控制。本机手动保持直到 USB 真实变化，不会暂停另一台电脑的自动规则。")
            alert.addButton(withTitle: model.text("Send display request", "发送显示器请求"))
            alert.addButton(withTitle: model.text("Cancel", "取消"))
            NSApp.activate(ignoringOtherApps: true)
            return alert.runModal() == .alertFirstButtonReturn
        }
        super.init()
        menu.autoenablesItems = false
        menu.delegate = self
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard menu === self.menu else { return }
        menu.removeAllItems()
        item(model.snapshot?.activeProfile?.name ?? model.text("No profile in use", "当前未使用配置"), enabled: false)
        item(model.usbTitle, enabled: false)
        menu.addItem(.separator())
        for (target, name) in [("local", model.localName), ("remote", model.remoteName)] {
            let entry = item(model.text("Switch display to ", "切换画面到 ") + name, action: #selector(switchDisplay(_:)), enabled: model.canSwitch)
            entry.representedObject = target
        }
        let automatic = item(model.text("Automatic switching on this computer", "本机自动切换"), action: #selector(toggleAutomatic), enabled: canToggleAutomatic)
        automatic.state = model.snapshot?.config?.automatic == true ? .on : .off
        let profiles = item(model.text("Profiles", "配置"))
        let submenu = NSMenu(); submenu.autoenablesItems = false; profiles.submenu = submenu
        let detect = NSMenuItem(title: model.text("Automatic profile detection", "自动识别配置"), action: #selector(restoreAutomatic), keyEquivalent: "")
        detect.target = self; detect.isEnabled = model.canChooseProfiles; detect.state = model.isManualProfileSelection ? .off : .on
        submenu.addItem(detect)
        if model.snapshot?.profiles.isEmpty == false { submenu.addItem(.separator()) }
        for profile in model.snapshot?.profiles ?? [] {
            let entry = NSMenuItem(title: profile.name, action: #selector(selectProfile(_:)), keyEquivalent: "")
            entry.target = self; entry.representedObject = profile.id; entry.isEnabled = model.canChooseProfiles
            entry.state = model.isActiveProfile(profile) ? .on : (model.snapshot?.profile_selection?.selected_profile_id == profile.id ? .mixed : .off)
            entry.toolTip = model.text("Changes this computer only; the other app is not updated.", "仅在这台电脑切换配置；另一台不会同步更改。")
            submenu.addItem(entry)
        }
        menu.addItem(.separator())
        item(model.text("Open PortKey…", "打开 PortKey…"), action: #selector(openWindow))
        menu.addItem(.separator())
        item(model.text("Quit PortKey", "退出 PortKey"), action: #selector(quit))
    }

    @discardableResult private func item(_ title: String, action: Selector? = nil, enabled: Bool = true) -> NSMenuItem {
        let entry = NSMenuItem(title: title, action: action, keyEquivalent: "")
        entry.target = self; entry.isEnabled = enabled; menu.addItem(entry)
        return entry
    }
    private var canToggleAutomatic: Bool {
        if model.snapshot?.config?.automatic == true { return model.canConfigure }
        return model.canSwitch && model.snapshot?.workspace?.can_enable != false
    }
    @objc private func switchDisplay(_ sender: NSMenuItem) {
        guard model.canSwitch, let profileID = model.snapshot?.active_profile_id, let target = sender.representedObject as? String, ["local", "remote"].contains(target), confirm(target), model.canSwitch, model.snapshot?.active_profile_id == profileID else { return }
        model.requestManual(target); model.sendManual(); showFailure()
    }
    @objc private func toggleAutomatic() {
        guard canToggleAutomatic else { return }
        model.toggleAutomation(model.snapshot?.config?.automatic != true); showFailure()
    }
    @objc private func selectProfile(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? String, let profile = model.snapshot?.profiles.first(where: { $0.id == id }) else { return }
        model.useProfile(profile); showFailure()
    }
    @objc private func restoreAutomatic() { model.restoreAutomaticProfiles(); showFailure() }
    @objc private func openWindow() { model.show(.overview) }
    @objc private func quit() { NSApp.terminate(nil) }
    private func showFailure() { if model.errorMessage != nil || model.bridgeFailure != nil { model.show(.overview) } }
}
