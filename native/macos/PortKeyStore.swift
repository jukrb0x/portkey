import AppKit
import SwiftUI
import ColorSync

enum Page: String, CaseIterable { case overview, workspaces, setup, diagnostics, preferences }
enum BridgeError: Error { case unavailable, invalidResponse }

final class PortKeyStore: NSObject, ObservableObject {
    @Published var snapshot: Snapshot?
    @Published var page: Page = .overview
    @Published var draft = WorkspaceDraft()
    @Published var selectedProfileID: String?
    @Published var errorMessage: String?
    @Published var bridgeFailure: String?
    @Published var pendingTarget: String?
    @Published var importedConfig: Configuration?
    @Published var metadataDraft = WorkspaceDraft()
    @Published var editingMetadata = false
    @Published var exportResult: String?
    @Published var diagnosticsPackageJSON: String?
    @Published var diagnosticsCollectionError: String?
    @Published var diagnosticsHistoryState = "uncollected"
    @Published var diagnosticsHistoryTruncated = false
    @Published var lastActionWarning: String?
    var onUpdate: (() -> Void)?
    var openWindow: (() -> Void)?
    private let core: CoreSession
    private var coreStarted = false
    private let preferences: UserDefaults
    private var localDraftPending = false
    init(core: CoreSession = NativeCore(), preferences: UserDefaults = .standard) { self.core = core; self.preferences = preferences; super.init() }
    private var timer: Timer?
    private var metadataProfileID: String?
    private var metadataBaseline = WorkspaceDraft()
    private var draftLoaded = false
    private var storedDraftKey = ""
    private let usbNotifications = USBNotifications()

    var language: String { snapshot?.language ?? "en" }
    func text(_ en: String, _ zh: String) -> String { language == "zh-Hans" ? zh : en }
    var localName: String { snapshot?.localName ?? text("This computer", "这台电脑") }
    var remoteName: String { snapshot?.remoteName ?? text("Other computer", "另一台电脑") }
    func computerName(in profile: SavedProfile, local: Bool) -> String {
        ComputerNamePresentation.name(local: local, saved: profile.workspace, runtime: isActiveProfile(profile) ? snapshot?.workspace : nil, language: language)
    }
    var canShowSelectedPage: Bool { snapshot?.startup_error == nil || page == .diagnostics }
    var canCollectDiagnostics: Bool { bridgeFailure == nil && snapshot != nil }
    var canConfigure: Bool { bridgeFailure == nil && snapshot != nil && snapshot?.startup_error == nil && snapshot?.controls_available != false }
    var canSwitch: Bool { bridgeFailure == nil && snapshot?.canSwitch == true }
    var warningMessage: String? { snapshot?.warning ?? lastActionWarning }
    var hasUnfinishedSetup: Bool { snapshot?.draft_available == true || snapshot?.setup != nil || draft.hasContent }
    var selectedProfile: SavedProfile? {
        snapshot?.profiles.first { $0.id == selectedProfileID } ?? snapshot?.activeProfile ?? snapshot?.profiles.first
    }
    func isActiveProfile(_ profile: SavedProfile) -> Bool { snapshot?.activeProfile?.id == profile.id }
    var canChooseProfiles: Bool { canConfigure && snapshot?.editing == false && snapshot?.sleeping == false && snapshot?.lifecycle_ok == true }
    var isManualProfileSelection: Bool { snapshot?.profile_selection?.mode == "manual" }
    var profileSelectionTitle: String {
        isManualProfileSelection ? text("Manually selected: ", "已手动选择：") + (snapshot?.profile_selection?.selected_profile_name ?? text("Saved profile", "已保存的配置")) : text("Automatic profile detection", "自动识别配置")
    }
    var profileSelectionMessage: String {
        if let message = snapshot?.profile_selection?.message, !message.isEmpty { return message }
        return text("Recognizes the connected display and your remembered wiring profile. This does not enable automatic input switching.", "识别已连接的显示器及记住的接线配置。这不会启用自动输入切换。")
    }
    func useProfile(_ profile: SavedProfile) {
        guard canChooseProfiles, snapshot?.profiles.contains(where: { $0.id == profile.id }) == true else { return }
        if action(["type": "profile_select", "profile_id": profile.id]) != nil { selectedProfileID = profile.id }
    }
    func restoreAutomaticProfiles() { guard canChooseProfiles else { return }; _ = action(["type": "profile_restore_auto"]) }
    var displays: [Device] {
        let choices = snapshot?.inventory?.displays ?? []
        var names: [String: String] = [:]
        for screen in NSScreen.screens {
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber,
                  let uuid = CGDisplayCreateUUIDFromDisplayID(number.uint32Value)?.takeRetainedValue(),
                  let identity = CFUUIDCreateString(nil, uuid) else { continue }
            names[(identity as String).uppercased()] = screen.localizedName
        }
        return choices.enumerated().map { index, device in
            let name = names[device.id.uppercased()] ?? text("External display \(index + 1)", "外接显示器 \(index + 1)")
            let duplicates = choices.filter { names[$0.id.uppercased()] == name }.count
            return Device(id: device.id, label: name + (duplicates > 1 ? " · \(index + 1)" : ""))
        }
    }
    var displayName: String {
        displays.first { $0.id == snapshot?.active_display_id }?.label ?? text("Enrolled display", "已绑定的显示器")
    }
    var effectiveTitle: String {
        switch snapshot?.effectiveState {
        case "active": return text("Following the USB switch", "跟随 USB 切换器")
        case "paused": return text("Paused on \(localName)", "已在\(localName)暂停")
        case "editing": return text("Setup in progress · locally paused", "设置中 · 本机已暂停")
        case "sleeping": return text("Sleeping · switching paused", "休眠中 · 切换已暂停")
        case "unavailable": return text("Switching unavailable", "切换不可用")
        case "attention": return text("Profile needs attention", "配置需要检查")
        default: return text("Waiting for reliable USB evidence", "等待可靠的 USB 状态")
        }
    }
    var usbTitle: String {
        switch snapshot?.ownership {
        case "owned": return text("Connected to \(localName)", "已连接\(localName)")
        case "absent": return text("Not connected here", "未连接本机")
        case "partial": return text("Partial connection · waiting", "连接不完整 · 等待稳定")
        default: return text("Not yet established", "尚未确认")
        }
    }
    var inputTitle: String {
        guard let state = snapshot?.display_state else { return text("Display input unconfirmed", "显示器输入尚未确认") }
        if state.result == "readback_confirmed" { return state.target == "local" ? localName : remoteName }
        if state.result == "sent_unverified" { return text("Request sent · input unconfirmed", "已发送请求 · 输入尚未确认") }
        if state.result == "failed" { return text("Display request needs attention", "显示器请求需要检查") }
        if state.result == "pending" { return text("Requesting ", "正在请求：") + (state.target == "local" ? localName : remoteName) }
        return text("Display input unconfirmed", "显示器输入尚未确认")
    }
    var inputEvidenceTitle: String {
        switch snapshot?.inputEvidence {
        case "pending": return text("Request pending", "请求处理中")
        case "readback_confirmed": return text("Input readback confirmed", "输入回报已确认")
        case "sent_unverified": return text("Request sent, input unconfirmed", "请求已发送，输入尚未确认")
        case "failed": return text("Request failed", "请求失败")
        default: return text("Input unconfirmed", "输入尚未确认")
        }
    }
    var inputSubtitle: String {
        snapshot?.inputEvidence == "readback_confirmed" ? text("Current readback confirmed by this computer", "本机已确认当前输入回报") : text("USB state and saved observations do not confirm the current picture.", "USB 状态与历史观察不代表已确认当前画面。")
    }
    var modeTitle: String {
        modeTitle(snapshot?.workspace)
    }
    func modeTitle(_ workspace: Workspace?) -> String {
        switch workspace?.mode {
        case "single": return text("Run on ", "仅运行于 ") + (workspace?.single_controller == "remote" ? workspace?.remote_name ?? remoteName : workspace?.local_name ?? localName)
        case "dual": return text("Run on both computers", "两台电脑均运行")
        default: return text("Installation plan not checked", "尚未检查运行方案")
        }
    }
    func verificationTitle(_ value: String?) -> String {
        switch value {
        case "single_observed": return text("Single controller · user-observed directions", "单机控制 · 双向画面已由用户观察")
        case "dual_observed": return text("Both controllers · user-observed directions", "双机控制 · 双向画面已由用户观察")
        case "historical_unknown": return text("Existing setup · evidence provenance unknown", "现有配置 · 验证来源未知")
        case "single_inconclusive": return text("Single controller · inconclusive", "单机控制 · 结果不明确")
        case "needs_attention": return text("Qualification needs review", "需要复查验证记录")
        default: return text("Not checked", "尚未检查")
        }
    }
    func start() {
        precondition(Thread.isMainThread)
        do { try core.start(); coreStarted = true } catch { failBridge(error); return }
        tick()
        guard snapshot?.startup_error == nil else { return }
        usbNotifications.onChange = { [weak self] in _ = self?.action(["type": "usb_refresh"]) }
        let ready = usbNotifications.start()
        _ = action(["type": "usb_notifications", "ready": ready])
        tick()
        startControllerClock()
    }
    func startControllerClock() {
        precondition(Thread.isMainThread)
        timer?.invalidate()
        let clock = Timer(timeInterval: 0.2, target: self, selector: #selector(tick), userInfo: nil, repeats: true)
        timer = clock
        // The policy clock belongs to the process, never to SwiftUI visibility or composition.
        RunLoop.main.add(clock, forMode: .common)
    }
    @objc func tick() {
        precondition(Thread.isMainThread)
        guard bridgeFailure == nil else { return }
        do {
            let next = try core.snapshot()
            if snapshot != next {
                if let phase = next.setup?.phase, ["display", "usb_retry"].contains(phase), draft.step > 1 { draft.step = 1; draft.clearAttestations() }
                snapshot = next
            }
            if !draftLoaded {
                let pointer = DraftStorageKey.make(configPath: next.config_path, profileID: "current") + ".profile"
                let profileID = next.setup?.profile_id ?? preferences.string(forKey: pointer) ?? "new"
                storedDraftKey = DraftStorageKey.make(configPath: next.config_path, profileID: profileID)
                var savedKey = storedDraftKey
                if preferences.data(forKey: savedKey) == nil { savedKey = DraftStorageKey.make(configPath: next.config_path, profileID: "setup") }
                if let data = preferences.data(forKey: savedKey), let saved = try? JSONDecoder().decode(WorkspaceDraft.self, from: data) { draft = saved }
                localDraftPending = preferences.bool(forKey: savedKey + ".localEdits")
                if let setup = next.setup { restoreDraft(setup) }
                draftLoaded = true
            }
            onUpdate?()
        } catch { failBridge(error) }
    }
    @discardableResult func action(_ value: [String: Any]) -> [String: Any]? {
        precondition(Thread.isMainThread)
        guard bridgeFailure == nil || value["type"] as? String == "quit" else { return nil }
        do {
            let reply = try core.action(value)
            guard reply["ok"] as? Bool == true else { errorMessage = reply["error"] as? String ?? text("Action failed.", "操作失败。"); tick(); return nil }
            lastActionWarning = reply["warning"] as? String
            if let type = value["type"] as? String, ["setup_read_local", "setup_read_remote", "setup_usb_start", "setup_select_display", "setup_join", "setup_resume"].contains(type) { draft.clearAttestations() }
            tick()
            return reply
        } catch { failBridge(error); return nil }
    }
    private func failBridge(_ error: Error) {
        bridgeFailure = text("The Rust core is unavailable. Restart PortKey. ", "Rust 核心不可用，请重启 PortKey。") + String(describing: error)
        timer?.invalidate()
        onUpdate?()
    }
    func stop() {
        precondition(Thread.isMainThread)
        usbNotifications.stop(); usbNotifications.onChange = nil
        timer?.invalidate(); timer = nil
        if coreStarted { _ = action(["type": "quit"]); core.stop(); coreStarted = false }
    }
    func json<T: Encodable>(_ value: T) -> Any { (try? JSONSerialization.jsonObject(with: JSONEncoder().encode(value))) ?? [:] }
    func persistDraft() {
        if let setup = snapshot?.setup { localDraftPending = !draft.matchesNames(in: setup) }
        else if !draft.hasContent { localDraftPending = false }
        guard !storedDraftKey.isEmpty, let configPath = snapshot?.config_path, let data = try? JSONEncoder().encode(draft) else { return }
        let profileID = draft.profileID ?? "new"
        storedDraftKey = DraftStorageKey.make(configPath: configPath, profileID: profileID)
        let pointer = DraftStorageKey.make(configPath: configPath, profileID: "current") + ".profile"
        preferences.set(profileID, forKey: pointer)
        preferences.set(data, forKey: storedDraftKey)
        preferences.set(localDraftPending, forKey: storedDraftKey + ".localEdits")
    }
    func restoreDraft(_ setup: SetupState, force: Bool = false) {
        if let profileID = setup.profile_id {
            if let localID = draft.profileID, localID != profileID { localDraftPending = false }
            draft.profileID = profileID
        }
        if let id = setup.display_id, !id.isEmpty { draft.displayID = id }
        guard force || !localDraftPending else { return }
        draft.name = setup.name ?? draft.name
        draft.displayID = setup.display_id ?? draft.displayID
        if let workspace = setup.workspace {
            draft.localName = workspace.local_name; draft.remoteName = workspace.remote_name
            draft.remoteOS = workspace.remote_os; draft.localRole = workspace.local_role
            draft.mode = workspace.mode; draft.singleController = workspace.single_controller
        }
    }
    func show(_ destination: Page) { page = destination; openWindow?() }
    func beginSetup() {
        if snapshot?.setup != nil { page = .setup; return }
        if snapshot?.draft_available == true { if action(["type": "setup_resume"]) != nil { if let setup = snapshot?.setup { restoreDraft(setup) }; draft.step = localDraftPending ? 0 : 1; page = .setup }; return }
        page = .setup; persistDraft()
    }
    func continueNames() {
        guard draft.validationError == nil else { errorMessage = text("Enter a profile name and two different computer names (1–80 characters).", "请输入配置名称和两个不同的电脑名称（1–80 字）。"); return }
        let type = snapshot?.setup == nil ? "setup_draft_begin" : "setup_workspace"
        if action(["type": type, "name": draft.name, "workspace": draft.workspace(preserving: snapshot?.setup?.workspace).actionValue]) != nil { draft.profileID = snapshot?.setup?.profile_id ?? draft.profileID; draft.step = 1; persistDraft() }
    }
    func joinSetup() {
        var value: [String: Any] = ["type": "setup_join", "record": draft.peerCode]
        if !draft.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { value["name"] = draft.name }
        if action(value) != nil { localDraftPending = false; if let setup = snapshot?.setup { restoreDraft(setup, force: true) }; draft.step = 1; persistDraft() }
    }
    func selectDisplay() { _ = action(["type": "setup_select_display", "display_id": draft.displayID]) }
    func updateSetupWorkspace() -> Bool {
        guard draft.validationError == nil else {
            errorMessage = text("Complete the profile and two distinct computer names before continuing. Finish later keeps incomplete text locally.", "继续前请填写配置名称及两个不同的电脑名称。稍后继续会在本机保留未完成的文字。")
            return false
        }
        let saved = action(["type": "setup_workspace", "name": draft.name, "workspace": draft.workspace(preserving: snapshot?.setup?.workspace).actionValue]) != nil
        if saved { draft.profileID = snapshot?.setup?.profile_id ?? draft.profileID; localDraftPending = false; persistDraft() }
        return saved
    }
    func suspendSetup() {
        persistDraft()
        if snapshot?.setup != nil {
            if draft.validationError == nil { guard updateSetupWorkspace() else { return } }
            guard action(["type": "setup_suspend"]) != nil else { return }
        }
        page = .overview
    }
    func cancelSetup() {
        guard action(["type": "setup_cancel"]) != nil else { return }
        draft = WorkspaceDraft(); persistDraft(); page = .overview
    }
    func finishSetup(enable: Bool) {
        guard updateSetupWorkspace() else { return }
        guard action(["type": "setup_finish", "enable": enable]) != nil else { return }
        draft = WorkspaceDraft(); persistDraft(); page = .overview
    }
    func saveEvidence() {
        guard updateSetupWorkspace() else { return }
        var evidence = Evidence()
        evidence.local = DirectionEvidence(result: draft.localObserved ? "observed" : "unverified", source: draft.localObserved ? "user_observed" : "none", note: draft.evidenceNote)
        evidence.remote = DirectionEvidence(result: draft.remoteObserved ? "observed" : "unverified", source: draft.remoteObserved ? "user_observed" : "none", note: draft.evidenceNote)
        let verification = draft.localObserved && draft.remoteObserved ? (draft.mode == "single" ? "single_observed" : "dual_observed") : "not_checked"
        _ = action(["type": "setup_evidence", "evidence": json(evidence), "verification": verification, "peer_paused_confirmed": draft.peerPaused])
    }
    func previewPeer() { _ = action(["type": "setup_peer_preview", "record": draft.peerCode]) }
    func copyRecord() {
        guard let reply = action(["type": "setup_export"]), let record = reply["record"] as? String else { return }
        NSPasteboard.general.clearContents(); NSPasteboard.general.setString(record, forType: .string)
        exportResult = text("Setup record copied", "已复制设置记录")
    }
    func requestManual(_ target: String) { pendingTarget = target }
    func sendManual() {
        guard let target = pendingTarget else { return }
        pendingTarget = nil; _ = action(["type": "manual", "target": target])
    }
    func toggleAutomation(_ enabled: Bool) { _ = action(["type": "automatic", "enabled": enabled]) }
    func setLanguage(_ language: String) { _ = action(["type": "language", "value": language]) }
    func editProfileDetails() {
        guard let profile = selectedProfile else { return }
        let workspace = profile.workspace ?? (isActiveProfile(profile) ? snapshot?.workspace : nil) ?? Workspace()
        metadataDraft = WorkspaceDraft()
        metadataDraft.name = profile.name; metadataDraft.localName = workspace.local_name; metadataDraft.remoteName = workspace.remote_name
        metadataDraft.remoteOS = workspace.remote_os; metadataDraft.mode = workspace.mode; metadataDraft.localRole = workspace.local_role
        metadataDraft.singleController = workspace.single_controller
        metadataProfileID = profile.id
        metadataBaseline = metadataDraft
        editingMetadata = true
    }
    func saveProfileDetails() {
        guard metadataDraft.validationError == nil else { errorMessage = text("Use a profile name and two distinct computer names.", "请填写配置名称和两个不同的电脑名称。"); return }
        guard let id = metadataProfileID, snapshot?.profiles.contains(where: { $0.id == id }) == true else {
            errorMessage = text("The profile being edited is no longer saved. Close this editor and choose the profile again.", "正在编辑的配置已不存在。请关闭编辑器并重新选择配置。")
            return
        }
        if action(["type": "workspace_save", "target_profile_id": id, "profile_name": metadataDraft.name, "workspace": metadataDraft.metadataChanges(from: metadataBaseline)]) != nil { editingMetadata = false; metadataProfileID = nil }
    }
    func importLegacy() {
        let panel = NSOpenPanel(); panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        panel.message = text("Choose a historical PortKey configuration. Review the imported binding before saving; automation remains off.", "选择旧版 PortKey 配置。保存前请检查设备绑定；自动切换保持关闭。")
        guard panel.runModal() == .OK, let url = panel.url, let reply = action(["type": "import", "path": url.path]), let value = reply["config"] else { return }
        do { importedConfig = try JSONDecoder().decode(Configuration.self, from: JSONSerialization.data(withJSONObject: value)); page = .preferences }
        catch { errorMessage = String(describing: error) }
    }
    func saveImported() {
        guard var config = importedConfig else { return }
        config.automatic = false; config.language = language
        if action(["type": "save", "config": json(config), "profile_name": text("Imported profile", "导入的配置")]) != nil { importedConfig = nil; page = .workspaces }
    }
    var diagnosticsPreview: String { diagnosticsPackageJSON ?? diagnosticsSnapshotFallback }
    var diagnosticsSnapshotFallback: String {
        let value: [String: Any] = ["application": "PortKey", "frontend": "SwiftUI + AppKit", "format": 1,
            "application_version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unavailable",
            "scope": "Sanitized current state snapshot only. Historical transition and transport timing records are not included.",
            "language": language, "ownership": snapshot?.ownership ?? "unknown", "effective_state": snapshot?.effectiveState ?? "unavailable",
            "automatic_requested": snapshot?.config?.automatic ?? false, "lifecycle_ok": snapshot?.lifecycle_ok ?? false,
            "sleeping": snapshot?.sleeping ?? false, "mode": snapshot?.workspace?.mode ?? "unverified",
            "qualification": snapshot?.workspace?.verification ?? "historical_unknown", "display_evidence": snapshot?.inputEvidence ?? "unconfirmed",
            "peer_runtime": "unknown", "setup_phase": snapshot?.setup?.phase ?? "none",
            "timing_retention": "Active file up to 1 MiB plus five backups; earlier events may be unavailable. Raw logs are not included."]
        return String(decoding: (try? JSONSerialization.data(withJSONObject: value, options: [.prettyPrinted, .sortedKeys])) ?? Data(), as: UTF8.self)
    }
    var diagnosticsHistorySummary: String {
        let summary: String
        switch diagnosticsHistoryState {
        case "available": summary = text("Bounded history collected from the configured timing files.", "已从配置的计时文件收集有限范围的历史记录。")
        case "empty": summary = text("The timing files contain no accepted history records.", "计时文件中没有可用的历史记录。")
        case "not_configured": summary = text("No timing log is configured. The current summary is available; recorded history is unavailable.", "未配置计时日志。当前摘要可用，但没有已记录的历史。")
        case "unavailable": summary = text("Timing history is unavailable. See the package for missing or unreadable-file counts.", "计时历史不可用。缺失或无法读取的文件数量见数据包。")
        default: summary = text("History has not been collected. The fallback below contains current state only.", "尚未收集历史。下方备用预览仅包含当前状态。")
        }
        return summary + (diagnosticsHistoryTruncated ? text(" Some records fall outside the collection limits; see the package counts.", " 部分记录超出收集范围；请查看数据包中的数量。") : "")
    }
    func collectDiagnostics() {
        diagnosticsPackageJSON = nil; diagnosticsCollectionError = nil; exportResult = nil
        diagnosticsHistoryState = "uncollected"; diagnosticsHistoryTruncated = false
        guard let reply = action(["type": "diagnostics_preview"]) else {
            diagnosticsCollectionError = errorMessage ?? bridgeFailure ?? text("Diagnostic collection is unavailable.", "诊断收集不可用。")
            return
        }
        do {
            guard let package = reply["diagnostics"] as? [String: Any], package["version"] as? Int == 1,
                  package["current"] is [String: Any], let history = package["history"] as? [String: Any],
                  let availability = history["availability"] as? String,
                  ["available", "empty", "not_configured", "unavailable"].contains(availability) else { throw BridgeError.invalidResponse }
            let bytes = try JSONSerialization.data(withJSONObject: package, options: [.prettyPrinted, .sortedKeys])
            diagnosticsPackageJSON = String(decoding: bytes, as: UTF8.self)
            diagnosticsHistoryState = availability
            diagnosticsHistoryTruncated = history["truncated"] as? Bool ?? false
        } catch {
            diagnosticsCollectionError = text("The core returned an invalid diagnostic package. Collect again after restarting PortKey.", "核心返回的诊断数据包无效。请重启 PortKey 后重新收集。")
        }
    }
    @discardableResult func writeDiagnostics(to url: URL) -> Bool {
        guard let package = diagnosticsPackageJSON else { return false }
        do {
            try Data(package.utf8).write(to: url, options: .atomic)
            exportResult = text("Diagnostics saved locally", "诊断已保存到本机")
            return true
        } catch { errorMessage = String(describing: error); return false }
    }
    func exportDiagnostics() {
        guard diagnosticsPackageJSON != nil else { return }
        let panel = NSSavePanel(); panel.nameFieldStringValue = "PortKey-diagnostics.json"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        _ = writeDiagnostics(to: url)
    }
}
