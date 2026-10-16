import AppKit
import FollowCore

@MainActor final class AppState {
    var configuration: Configuration?
    var policy = OwnershipPolicy()
    var inventory: USBInventory?
    private var statusMessage: UIMessage = "Reading USB…"
    private(set) var language = UILanguage.saved()
    var localizer: Localizer { Localizer(language: language) }
    var status: String { statusMessage.localized(using: localizer) }
    var onChange: (() -> Void)?
    let log = DiagnosticLog()
    let helper = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/m1ddc")
    let monitor = USBMonitor()
    private var timer: Timer?
    private var tokens: [NSObjectProtocol] = []
    private var lastSnapshot = 0.0
    private var previousDisplays: Set<String> = []
    private var unavailable: UIMessage?
    private var lastLogged = ""
    lazy var scheduler = CommandScheduler(backend: { [weak self] target in
        guard let self, let configuration = self.configuration, self.unavailable == nil else { return .failed("Open settings to repair configuration/backend") }
        return await DisplayBackend(configuration: configuration, execute: { [helper = self.helper] args in await ProcessRunner.run(executable: helper, arguments: args) }).select(target)
    })
    var now: Double { ProcessInfo.processInfo.systemUptime }
    var automatic: Bool { policy.automatic }
    init() {
        do { configuration = try ConfigurationStore.standard.load() } catch { unavailable = UIMessage("Invalid configuration: %@; open Settings.", error.localizedDescription) }
        if !FileManager.default.isExecutableFile(atPath: helper.path) { unavailable = "Missing m1ddc backend; rebuild the app." }
        scheduler.onChange = { [weak self] progress in self?.setStatus(progress.message) }
        if UserDefaults.standard.object(forKey: "automatic") != nil && !UserDefaults.standard.bool(forKey: "automatic") || CommandLine.arguments.contains("--paused") { policy.setAutomatic(false, now: now) }
        monitor.onChange = { [weak self] in self?.reconcile() }
        do { try monitor.start() } catch { unavailable = UIMessage(error.localizedDescription) }
        previousDisplays = Set(DisplayCatalog.online().map(\.uuid))
        observe(NSWorkspace.willSleepNotification, center: NSWorkspace.shared.notificationCenter) { state in
            state.policy.sleep(); state.scheduler.submit(nil); state.setStatus("Mac sleeping; pending work cancelled")
        }
        observe(NSWorkspace.didWakeNotification, center: NSWorkspace.shared.notificationCenter) { state in
            state.policy.wake(now: state.now); state.setStatus("Mac awake; reading USB again"); state.reconcile()
        }
        observe(NSApplication.didChangeScreenParametersNotification, center: .default) { state in
            let displays = Set(DisplayCatalog.online().map(\.uuid))
            if let uuid = state.configuration?.monitor, displays.contains(uuid), !state.previousDisplays.contains(uuid) {
                state.policy.monitorReconnected(now: state.now); state.scheduler.submit(state.policy.request)
            }
            state.previousDisplays = displays; state.onChange?()
        }
        timer = PolicyTicker.start { [weak self] in self?.tick() }
        log.write("App started; helper=\(helper.path); automatic=\(automatic)")
        reconcile()
    }
    private func observe(_ name: Notification.Name, center: NotificationCenter, action: @escaping @MainActor (AppState) -> Void) {
        tokens.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in MainActor.assumeIsolated { if let self { action(self) } } })
    }
    func reconcile() {
        guard !policy.sleeping else { return }
        lastSnapshot = now
        do {
            let value = try USBMonitor.snapshot(); inventory = value
            let ownership = configuration?.observe(value.devices) ?? .unknown
            let previous = policy.ownership; policy.observe(ownership, now: now)
            if previous != ownership { log.write("USB \(previous.rawValue) → \(ownership.rawValue)") }
            scheduler.submit(policy.request)
            if let unavailable { setStatus(unavailable) }
            else if !automatic { setStatus("Automatic switching paused") }
            else if ownership == .partial { setStatus("USB hub pair incomplete; waiting to settle") }
            else if ownership == .unknown { setStatus("USB state unknown; check enrollment") }
            else if ownership == .absent && policy.request == nil { setStatus("USB absent from Mac; waiting for a valid ownership transition") }
            onChange?()
        } catch {
            inventory = nil; policy.observe(.unknown, now: now); scheduler.submit(nil)
            setStatus(UIMessage("USB read failed: %@", error.localizedDescription))
        }
    }
    private func tick() {
        guard !policy.sleeping else { return }
        if now - lastSnapshot >= 2 { reconcile() }
        let request = policy.tick(now: now)
        scheduler.submit(unavailable == nil ? request : nil)
    }
    func toggleAutomatic() {
        policy.setAutomatic(!automatic, now: now); scheduler.submit(nil)
        UserDefaults.standard.set(automatic, forKey: "automatic")
        setStatus(automatic ? "Automatic switching enabled; waiting for USB to settle" : "Automatic switching paused")
        reconcile()
    }
    func manual(_ target: Target) {
        guard unavailable == nil else { setStatus(unavailable!); return }
        policy.manual(target); log.write("Manual selection: \(target.rawValue)"); scheduler.submit(policy.request); onChange?()
    }
    func save(_ value: Configuration) throws {
        try ConfigurationStore.standard.save(value)
        configuration = value; unavailable = nil
        if !FileManager.default.isExecutableFile(atPath: helper.path) { unavailable = "Missing m1ddc backend; rebuild the app." }
        policy.sleep(); policy.wake(now: now)
        // A new scheduler lifetime must not overlap an old helper; submit nil on the existing worker.
        scheduler.submit(nil); log.write("Enrollment saved: \(value.monitor), \(value.usb2.label), \(value.usb3.label)")
        setStatus("Settings saved; reading USB again"); reconcile()
    }
    func setLanguage(_ value: UILanguage) {
        language = value; value.save(); onChange?()
    }
    private func setStatus(_ message: UIMessage) {
        statusMessage = message
        let text = message.localized(using: Localizer(language: .english))
        if text != lastLogged { log.write(text); lastLogged = text }
        onChange?()
    }
    func stop() async { timer?.invalidate(); scheduler.submit(nil); await scheduler.waitUntilIdle(); log.write("App stopped") }
}
