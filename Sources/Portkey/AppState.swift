import AppKit
import FollowCore

@MainActor final class AppState {
    var configuration: Configuration?
    var policy = OwnershipPolicy()
    var inventory: USBInventory?
    var status = "正在读取 USB…"
    var onChange: (() -> Void)?
    let log = DiagnosticLog()
    let helper = Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/m1ddc")
    let monitor = USBMonitor()
    private var timer: Timer?
    private var tokens: [NSObjectProtocol] = []
    private var lastSnapshot = 0.0
    private var previousDisplays: Set<String> = []
    private var unavailable: String?
    private var lastLogged = ""
    lazy var scheduler = CommandScheduler(backend: { [weak self] target in
        guard let self, let configuration = self.configuration, self.unavailable == nil else { return .failed("Open settings to repair configuration/backend") }
        return await DisplayBackend(configuration: configuration, execute: { [helper = self.helper] args in await ProcessRunner.run(executable: helper, arguments: args) }).select(target)
    })
    var now: Double { ProcessInfo.processInfo.systemUptime }
    var automatic: Bool { policy.automatic }
    init() {
        do { configuration = try ConfigurationStore.standard.load() } catch { unavailable = "配置无效：\(error.localizedDescription)；请打开设置" }
        if !FileManager.default.isExecutableFile(atPath: helper.path) { unavailable = "缺少后端 m1ddc；请重新构建 app" }
        scheduler.onChange = { [weak self] text in self?.setStatus(text) }
        if UserDefaults.standard.object(forKey: "automatic") != nil && !UserDefaults.standard.bool(forKey: "automatic") || CommandLine.arguments.contains("--paused") { policy.setAutomatic(false, now: now) }
        monitor.onChange = { [weak self] in self?.reconcile() }
        do { try monitor.start() } catch { unavailable = error.localizedDescription }
        previousDisplays = Set(DisplayCatalog.online().map(\.uuid))
        observe(NSWorkspace.willSleepNotification, center: NSWorkspace.shared.notificationCenter) { state in
            state.policy.sleep(); state.scheduler.submit(nil); state.setStatus("Mac 睡眠；已取消待执行操作")
        }
        observe(NSWorkspace.didWakeNotification, center: NSWorkspace.shared.notificationCenter) { state in
            state.policy.wake(now: state.now); state.setStatus("Mac 已唤醒；重新读取 USB"); state.reconcile()
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
            else if !automatic { setStatus("自动切换已暂停") }
            else if ownership == .partial { setStatus("USB 双 Hub 尚未齐全；等待稳定") }
            else if ownership == .unknown { setStatus("USB 状态未知；等待重新绑定") }
            else if ownership == .absent && policy.request == nil { setStatus("USB 不在 Mac；等待有效切换证据") }
            onChange?()
        } catch {
            inventory = nil; policy.observe(.unknown, now: now); scheduler.submit(nil)
            setStatus("USB 读取失败：\(error.localizedDescription)")
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
        setStatus(automatic ? "自动切换已开启；等待 USB 稳定" : "自动切换已暂停")
        reconcile()
    }
    func manual(_ target: Target) {
        guard unavailable == nil else { setStatus(unavailable!); return }
        policy.manual(target); log.write("Manual selection: \(target.rawValue)"); scheduler.submit(policy.request); onChange?()
    }
    func save(_ value: Configuration) throws {
        try ConfigurationStore.standard.save(value)
        configuration = value; unavailable = nil
        if !FileManager.default.isExecutableFile(atPath: helper.path) { unavailable = "缺少后端 m1ddc；请重新构建 app" }
        policy.sleep(); policy.wake(now: now)
        // A new scheduler lifetime must not overlap an old helper; submit nil on the existing worker.
        scheduler.submit(nil); log.write("Enrollment saved: \(value.monitor), \(value.usb2.label), \(value.usb3.label)")
        setStatus("设置已保存；重新读取 USB"); reconcile()
    }
    private func setStatus(_ text: String) {
        status = text
        if text != lastLogged { log.write(text); lastLogged = text }
        onChange?()
    }
    func stop() async { timer?.invalidate(); scheduler.submit(nil); await scheduler.waitUntilIdle(); log.write("App stopped") }
}
