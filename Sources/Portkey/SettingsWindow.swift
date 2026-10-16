import AppKit
import FollowCore

@MainActor final class SettingsWindow: NSObject {
    private let state: AppState
    private let showMenu: (NSView) -> Void
    private let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 690, height: 560), styleMask: [.titled, .closable], backing: .buffered, defer: false)
    private let monitor = NSTextField()
    private let usb2 = NSPopUpButton(), usb3 = NSPopUpButton()
    private let macInput = NSTextField(), windowsInput = NSTextField()
    private let detected = NSTextField(wrappingLabelWithString: "")
    private let message = NSTextField(wrappingLabelWithString: "")
    private let note = NSTextField(wrappingLabelWithString: "")
    private let refresh = NSButton(), saveButton = NSButton(), menuButton = NSButton()
    private var labels: [(NSTextField, NSControl, String)] = []
    private var choices: [USBDevice] = []
    private enum Notice { case configuration, saved, error(String) }
    private var notice = Notice.configuration
    init(state: AppState, showMenu: @escaping (NSView) -> Void) {
        self.state = state; self.showMenu = showMenu; super.init()
        window.isReleasedWhenClosed = false
        let stack = NSStackView(); stack.orientation = .vertical; stack.alignment = .leading; stack.spacing = 13
        stack.translatesAutoresizingMaskIntoConstraints = false
        window.contentView!.addSubview(stack)
        NSLayoutConstraint.activate([stack.leadingAnchor.constraint(equalTo: window.contentView!.leadingAnchor, constant: 22), stack.trailingAnchor.constraint(equalTo: window.contentView!.trailingAnchor, constant: -22), stack.topAnchor.constraint(equalTo: window.contentView!.topAnchor, constant: 20)])
        func field(_ key: String, _ view: NSControl) {
            let label = NSTextField(labelWithString: ""); labels.append((label, view, key))
            let row = NSStackView(views: [label, view]); row.spacing = 12; row.alignment = .centerY
            label.widthAnchor.constraint(equalToConstant: 150).isActive = true
            stack.addArrangedSubview(row); row.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        }
        detected.font = .systemFont(ofSize: 11); stack.addArrangedSubview(detected)
        field("Monitor system UUID", monitor)
        field("Shared USB2 hub", usb2); field("Shared USB3 hub", usb3)
        field("Mac input (HDMI 1)", macInput); field("Windows input (DP)", windowsInput)
        note.font = .systemFont(ofSize: 12); stack.addArrangedSubview(note); note.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        refresh.target = self; refresh.action = #selector(reload)
        saveButton.target = self; saveButton.action = #selector(save); saveButton.keyEquivalent = "\r"
        menuButton.target = self; menuButton.action = #selector(openMenu(_:))
        window.initialFirstResponder = refresh
        stack.addArrangedSubview(NSStackView(views: [refresh, saveButton, menuButton])); stack.addArrangedSubview(message)
        message.widthAnchor.constraint(equalTo: stack.widthAnchor).isActive = true
        reload(); window.center()
    }
    func show() { reload(); NSApp.activate(ignoringOtherApps: true); window.makeKeyAndOrderFront(nil) }
    func refreshLanguage() {
        let l = state.localizer
        window.title = l.text("Portkey — Settings / Rebind")
        for (label, control, key) in labels { label.stringValue = l.text(key); control.setAccessibilityLabel(l.text(key)) }
        note.stringValue = l.text("Connect USB to Mac before rebinding. Select shared branches by location and parent hub; identical upstream hubs are not substitutes. Rebind after changing the Mac port or dock.\nUnplugging or powering off the entire USB switch also looks like leaving Mac. Both hosts must be awake.")
        refresh.title = l.text("Rescan / Revert Edits"); saveButton.title = l.text("Save Binding"); menuButton.title = l.text("Open Status Menu")
        for popup in [usb2, usb3] {
            for (index, hub) in choices.enumerated() {
                popup.item(at: index)?.title = hub.label + ((state.inventory?.hubs.contains(hub) ?? false) ? "" : l.text(" (disconnected / saved)"))
            }
        }
        detected.stringValue = l.text("Online displays:\n%@\nBackend: Contents/Helpers/m1ddc · pinned 04d9497", DisplayCatalog.online().map { "\($0.name) — \($0.uuid)" }.joined(separator: "\n"))
        switch notice {
        case .configuration: message.stringValue = l.text("Configuration: %@", ConfigurationStore.standard.url.path)
        case .saved: message.stringValue = l.text("Saved. %@", state.status)
        case .error(let detail): message.stringValue = l.text(detail)
        }
    }
    @objc private func openMenu(_ sender: NSButton) { showMenu(sender) }
    @objc private func reload() {
        state.reconcile()
        let config = state.configuration ?? .qualified
        monitor.stringValue = config.monitor; macInput.stringValue = String(config.macInput); windowsInput.stringValue = String(config.windowsInput)
        choices = state.inventory?.hubs ?? []
        for hub in [config.usb2, config.usb3] where !choices.contains(hub) { choices.append(hub) }
        for popup in [usb2, usb3] { popup.removeAllItems(); for hub in choices { popup.addItem(withTitle: hub.label) } }
        usb2.selectItem(at: choices.firstIndex(of: config.usb2) ?? 0); usb3.selectItem(at: choices.firstIndex(of: config.usb3) ?? 0)
        notice = .configuration; refreshLanguage()
    }
    @objc private func save() {
        do {
            guard choices.indices.contains(usb2.indexOfSelectedItem), choices.indices.contains(usb3.indexOfSelectedItem),
                  let mac = UInt16(macInput.stringValue), let windows = UInt16(windowsInput.stringValue) else { throw ConfigError.invalid("Choose two hubs and enter valid input values.") }
            var config = state.configuration ?? .qualified
            config.monitor = monitor.stringValue.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
            config.usb2 = choices[usb2.indexOfSelectedItem]; config.usb3 = choices[usb3.indexOfSelectedItem]
            config.macInput = mac; config.windowsInput = windows
            try state.save(config); notice = .saved
        } catch { notice = .error(error.localizedDescription) }
        refreshLanguage()
    }
}
