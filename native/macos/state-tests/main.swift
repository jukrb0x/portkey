import Foundation
import AppKit
import CoreFoundation

var checks = 0
func expect(_ value: @autoclosure () -> Bool, _ message: String) {
    checks += 1
    if !value() { fputs("FAIL: \(message)\n", stderr); exit(1) }
}
func baseSnapshot() throws -> Snapshot {
    try JSONDecoder().decode(Snapshot.self, from: Data("""
    {"profiles":[{"profile_id":"profile-A","name":"Home","display_id":"display-A"},{"profile_id":"profile-B","name":"Work","display_id":"display-B"}],"active_profile_id":"profile-A","active_display_id":"display-A","profile_name":"Home","config":{"version":1,"platform":"macos","display_id":"display-A","local_input":5,"remote_input":7,"hubs":[],"automatic":true,"language":"en"},"inventory":{"displays":[],"hubs":[]},"ownership":"owned","status":"","error":false,"lifecycle_ok":true,"sleeping":false,"lifecycle_epoch":0,"language":"en","config_path":"/isolated/config","editing":false}
    """.utf8))
}
final class MemoryCore: CoreSession {
    var current: Snapshot
    var actions: [[String: Any]] = []
    var suspended: SetupState?
    var snapshotCount = 0
    var diagnosticsReply: [String: Any]?
    var responseWarning: String?
    init(_ current: Snapshot) { self.current = current }
    func start() throws { fatalError("State tests must never start hardware observation") }
    func stop() {}
    func snapshot() throws -> Snapshot { snapshotCount += 1; return current }
    func action(_ value: [String: Any]) throws -> [String: Any] {
        actions.append(value)
        switch value["type"] as? String {
        case "diagnostics_preview": return diagnosticsReply ?? ["ok": false, "error": "History unavailable"]
        case "setup_workspace":
            current.setup?.name = value["name"] as? String ?? current.setup?.name
            if let workspace = value["workspace"] { current.setup?.workspace = try JSONDecoder().decode(Workspace.self, from: JSONSerialization.data(withJSONObject: workspace)) }
        case "setup_suspend": suspended = current.setup; current.setup = nil; current.draft_available = true
        case "setup_resume": current.setup = suspended; current.setup?.phase = "display"
        default: break
        }
        if let responseWarning { return ["ok": true, "warning": responseWarning] }
        return ["ok": true]
    }
}
let core = MemoryCore(try baseSnapshot())
let suiteName = "local.portkey.state-tests." + UUID().uuidString
let preferences = UserDefaults(suiteName: suiteName)!
defer { preferences.removePersistentDomain(forName: suiteName) }
let store = PortKeyStore(core: core, preferences: preferences)
store.snapshot = core.current
store.selectedProfileID = "profile-B"
core.current.setup = SetupState(phase: "ready", name: "Work", display_id: "display-B", local_input: 17, remote_input: 15, workspace: Workspace(), profile_id: "profile-B", target_profile_id: "profile-B")
// Supply the post-action snapshot without starting any hardware observer.
store.editProfileSetup()
expect(core.actions.last?["type"] as? String == "setup_edit" && core.actions.last?["profile_id"] as? String == "profile-B", "setup editor targets the viewed profile explicitly")
expect(store.page == .setup && store.draft.step == 3 && store.draft.name == "Work" && store.draft.displayID == "display-B" && store.draft.profileID == "profile-B", "existing profile opens at review with saved bindings instead of fresh enrollment")
core.current.setup = nil; store.snapshot = core.current; core.actions = []
store.selectedProfileID = "profile-B"
store.editProfileDetails()
store.metadataDraft.localName = "Work laptop"
store.metadataDraft.remoteName = "Personal desktop"
store.saveProfileDetails()
let rename = core.actions.last!
let changes = rename["workspace"] as! [String: Any]
expect(rename["target_profile_id"] as? String == "profile-B", "inactive profile rename targets explicitly opened profile")
expect(changes["verification"] == nil && changes["mode"] == nil && changes["evidence"] == nil, "inactive legacy rename cannot replace provenance or enabled eligibility")
expect(Set(changes.keys) == Set(["local_name", "remote_name"]), "legacy rename emits only user naming intent")

core.actions = []
store.selectedProfileID = nil; core.current.active_display_id = "display-A"; core.current.active_profile_id = "profile-A"; store.snapshot = core.current
store.editProfileDetails()
store.metadataDraft.localName = "Laptop A"; store.metadataDraft.remoteName = "Desktop A"
core.current.active_display_id = "display-B"; core.current.active_profile_id = "profile-B"; store.snapshot = core.current
store.saveProfileDetails()
expect(core.actions.last?["target_profile_id"] as? String == "profile-A", "active profile changes cannot retarget an open editor")

core.actions = []
core.current.active_display_id = "display-A"; core.current.active_profile_id = "profile-A"; store.snapshot = core.current
store.editProfileDetails()
core.current.profiles.removeAll { $0.display_id == "display-A" }; store.snapshot = core.current
store.saveProfileDetails()
expect(core.actions.isEmpty && store.errorMessage != nil && store.editingMetadata, "missing edited profile is recoverable and cannot write another profile")

// Enabling records the user's actual observations and commits once, without a separate evidence-save step.
var editWorkspace = Workspace(); editWorkspace.workspace_id = "pk-state-edit"
core.current.setup = SetupState(phase: "ready", name: "Work", display_id: "display-B", local_input: 17, remote_input: 15, workspace: editWorkspace, profile_id: "profile-B", target_profile_id: "profile-B", draft_id: "edit-test", editing_saved_profile: true)
store.snapshot = core.current
store.restoreDraft(core.current.setup!, force: true)
store.draft.mode = "single"; store.draft.remoteOS = "windows"
store.draft.localObserved = false; store.draft.remoteObserved = false
expect(store.setupEnableBlocker != nil, "disabled enable explains missing visual observations")
store.draft.localObserved = true; store.draft.remoteObserved = true; store.draft.peerPaused = true
expect(store.setupEnableBlocker == nil, "all single-controller confirmations allow one-step enable")
core.actions = []
store.finishSetup(enable: true)
expect(core.actions.map { $0["type"] as! String } == ["setup_workspace", "setup_evidence", "setup_finish"], "one enable action records confirmations before committing the same profile")
store.snapshot = core.current
store.startSetupTest("local")
expect(store.setupTestCountdown != nil && core.actions.last?["type"] as? String != "setup_test", "local reclaim test waits for user to show the other input")
_ = store.action(["type": "edit", "enabled": false])
expect(store.setupTestCountdown == nil, "editing setup cancels a delayed switch test")
core.current.setup = nil; store.snapshot = core.current; core.actions = []

var workspace = Workspace(); workspace.workspace_id = "pk-test"; workspace.local_name = "Old laptop"; workspace.remote_name = "Old desktop"; workspace.remote_os = "windows"; workspace.mode = "dual"
let setup = SetupState(phase: "display", name: "Old Home", display_id: "display-A", workspace: workspace, profile_id: "draft-A")
core.current.setup = setup; core.current.editing = true; store.snapshot = core.current
store.restoreDraft(setup)
expect(store.draft.profileID == "draft-A", "local draft binds to stable profile identity independently of the display")
store.draft.name = "New Home"; store.draft.localName = "New laptop"; store.draft.remoteName = "New desktop"
store.draft.remoteOS = "macos"; store.draft.localRole = "b"
core.actions = []
store.suspendSetup()
expect(core.actions.map { $0["type"] as! String } == ["setup_workspace", "setup_suspend"], "Finish later synchronizes valid fields before suspending")
expect(core.suspended?.name == "New Home" && core.suspended?.workspace?.local_name == "New laptop", "workspace and computer names are stored in the core draft")
store.beginSetup()
expect(store.draft.name == "New Home" && store.draft.localName == "New laptop" && store.draft.remoteOS == "macos" && store.draft.localRole == "b", "resume retains edited names, OS and role")

store.draft.localName = ""; store.draft.name = "Still editing Home"
core.actions = []
store.suspendSetup()
expect(core.actions.map { $0["type"] as! String } == ["setup_suspend"], "incomplete local names never replace valid core draft")
let restartedCore = MemoryCore(core.current); restartedCore.suspended = core.suspended
let restartedStore = PortKeyStore(core: restartedCore, preferences: preferences)
restartedStore.tick(); restartedStore.beginSetup()
expect(restartedStore.draft.localName == "" && restartedStore.draft.name == "Still editing Home", "incomplete local names survive process reconstruction and core resume")
var otherDraftWorkspace = workspace; otherDraftWorkspace.local_name = "Other laptop"; otherDraftWorkspace.remote_name = "Other desktop"
let differentDraftCore = MemoryCore(core.current)
differentDraftCore.current.setup = SetupState(phase: "display", name: "Other wiring", display_id: "display-A", workspace: otherDraftWorkspace, profile_id: "draft-B")
let differentDraftStore = PortKeyStore(core: differentDraftCore, preferences: preferences)
differentDraftStore.tick()
expect(differentDraftStore.draft.profileID == "draft-B" && differentDraftStore.draft.name == "Other wiring" && differentDraftStore.draft.localName == "Other laptop", "incomplete local text for one profile cannot leak into a different same-display draft")
store.beginSetup()
expect(store.draft.localName == "" && store.draft.name == "Still editing Home", "resume preserves incomplete local text rather than older core names")
expect(store.draft.step == 0, "incomplete names resume on the Names step")

store.draft.localName = "Final laptop"; store.draft.remoteName = "Final desktop"; store.draft.name = "Final Home"
core.actions = []
store.finishSetup(enable: false)
expect(core.actions.map { $0["type"] as! String } == ["setup_workspace", "setup_finish"], "finish synchronizes current workspace name even when step picker bypassed Continue")
expect(core.current.setup?.name == "Final Home", "final save uses revised workspace name")
core.current.display_state = DisplayState(result: "sent_unverified")
core.current.language = "zh-Hans"; store.snapshot = core.current
expect(store.inputEvidenceTitle == "请求已发送，输入尚未确认", "diagnostic runtime evidence uses selected Chinese language")
core.current.language = "en"; store.snapshot = core.current
expect(store.inputEvidenceTitle == "Request sent, input unconfirmed", "diagnostic runtime evidence uses human English labels")
let trackingMode = CFRunLoopMode(rawValue: "NSEventTrackingRunLoopMode" as CFString)
CFRunLoopAddCommonMode(CFRunLoopGetMain(), trackingMode)
let beforeTicking = core.snapshotCount
store.startControllerClock()
CFRunLoopRunInMode(trackingMode, 0.46, false)
expect(core.snapshotCount >= beforeTicking + 2, "actual 200ms process clock keeps snapshotting without windows during menu tracking")
store.stop()
let stoppedCount = core.snapshotCount
CFRunLoopRunInMode(trackingMode, 0.25, false)
expect(core.snapshotCount == stoppedCount, "process shutdown invalidates the controller clock")
let diagnostic = try JSONSerialization.jsonObject(with: Data(store.diagnosticsPreview.utf8)) as! [String: Any]
expect(diagnostic["application_version"] != nil && (diagnostic["scope"] as? String)?.contains("current state snapshot only") == true, "diagnostic scope and version availability are explicit")
expect(!store.diagnosticsPreview.contains("display-A") && !store.diagnosticsPreview.contains("/isolated/config") && !store.diagnosticsPreview.contains("Final laptop"), "export excludes local identities, paths and computer names")
let previewActionsBefore = core.actions.count
store.tick(); store.tick()
expect(core.actions.count == previewActionsBefore, "ordinary snapshot ticks never read diagnostic history")
expect(store.diagnosticsPackageJSON == nil, "diagnostic export starts unavailable until explicit successful collection")
let temporaryExport = FileManager.default.temporaryDirectory.appendingPathComponent("portkey-diagnostics-" + UUID().uuidString + ".json")
defer { try? FileManager.default.removeItem(at: temporaryExport) }
expect(!store.writeDiagnostics(to: temporaryExport) && !FileManager.default.fileExists(atPath: temporaryExport.path), "uncollected fallback cannot be exported as a full package")
let collected: [String: Any] = ["version": 1, "application": ["name": "PortKey", "version": "test"], "current": ["mode": "dual"], "history": ["availability": "unavailable", "files_missing": 1, "truncated": true, "records": [["event": "command_complete", "duration_us": 50, "outcome": "sent_unverified"]]]]
core.diagnosticsReply = ["ok": true, "diagnostics": collected]
store.collectDiagnostics()
let preview = store.diagnosticsPreview
let received = try JSONSerialization.jsonObject(with: Data(preview.utf8)) as! NSDictionary
expect(received.isEqual(to: collected), "preview retains the exact shared package including unavailable and truncated evidence")
expect(store.diagnosticsHistoryState == "unavailable" && store.diagnosticsHistoryTruncated && store.diagnosticsHistorySummary.contains("collection limits"), "missing history and truncation remain explicit in the native presentation")
expect(store.writeDiagnostics(to: temporaryExport), "collected diagnostic package can be saved to explicit local destination")
let exportedBytes = try String(contentsOf: temporaryExport, encoding: .utf8)
expect(exportedBytes == preview, "export bytes equal the previewed package")
let collectCount = core.actions.filter { $0["type"] as? String == "diagnostics_preview" }.count
store.tick()
expect(core.actions.filter { $0["type"] as? String == "diagnostics_preview" }.count == collectCount && store.diagnosticsPreview == preview, "later ticks neither recollect history nor silently change the preview")
core.diagnosticsReply = ["ok": false, "error": "History unavailable"]
store.collectDiagnostics()
expect(store.diagnosticsPackageJSON == nil && store.diagnosticsCollectionError != nil, "failed collection marks package unavailable rather than exporting stale history")

core.diagnosticsReply = ["ok": true, "diagnostics": ["version": 99, "current": [:], "history": ["availability": "available"]]]
store.collectDiagnostics()
expect(store.diagnosticsPackageJSON == nil && store.diagnosticsCollectionError != nil, "incompatible package version cannot be exported as a valid preview")
store.errorMessage = nil
core.responseWarning = "Workspace saved, but the old draft could not be removed."
let committed = store.action(["type": "setup_finish", "enable": false])
expect(committed != nil && store.errorMessage == nil && store.warningMessage == core.responseWarning, "committed save warning stays a success and is visible outside setup")
core.current.language = "zh-Hans"; core.current.warning = "工作空间已保存，但无法移除旧草稿。"; store.tick()
expect(store.warningMessage == "工作空间已保存，但无法移除旧草稿。", "localized snapshot warning takes precedence over old action detail")
core.current.warning = nil; core.responseWarning = nil; _ = store.action(["type": "refresh"])
expect(store.warningMessage == nil, "resolved warning does not remain as a stale failure")
core.current.startup_error = "Another PortKey controller is running."
core.current.controls_available = false; core.diagnosticsReply = ["ok": true, "diagnostics": collected]
store.snapshot = core.current; core.actions = []
for blockedPage in [Page.overview, .workspaces, .setup, .preferences] {
    store.page = blockedPage
    expect(!store.canShowSelectedPage, "startup failure keeps configuration and hardware pages blocked")
}
store.page = .diagnostics
expect(store.canShowSelectedPage && store.canCollectDiagnostics && !store.canConfigure && !store.canSwitch, "startup failure exposes only read-only diagnostics while normal controls stay blocked")
store.collectDiagnostics()
expect(core.actions.map { $0["type"] as! String } == ["diagnostics_preview"], "startup diagnostic recovery sends no configuration or device refresh action")
expect(store.diagnosticsPackageJSON != nil && store.writeDiagnostics(to: temporaryExport), "startup-blocked controller can collect and export its sanitized evidence package")
core.current.startup_error = nil; core.current.controls_available = true; core.current.editing = false
core.current = try baseSnapshot()
core.current.profiles[1].display_id = "display-A"
core.current.profiles[1].name = "Home · HDMI 2"
store.snapshot = core.current; store.selectedProfileID = "profile-B"
expect(store.selectedProfile?.profile_id == "profile-B" && !store.isActiveProfile(core.current.profiles[1]), "same-display browsing keeps distinct wiring profile identity")
store.editProfileDetails()
store.metadataDraft.localName = "HDMI2 laptop"; store.metadataDraft.remoteName = "HDMI2 desktop"
core.current.active_profile_id = "profile-A"; store.snapshot = core.current; core.actions = []
store.saveProfileDetails()
expect(core.actions.last?["target_profile_id"] as? String == "profile-B", "same-display metadata editor saves its exact pinned profile ID")
store.selectedProfileID = nil; core.current.active_profile_id = "profile-A"; store.snapshot = core.current
store.editProfileDetails()
store.metadataDraft.localName = "Pinned A"; store.metadataDraft.remoteName = "Peer A"
core.current.active_profile_id = "profile-B"; store.snapshot = core.current; core.actions = []
store.saveProfileDetails()
expect(core.actions.last?["target_profile_id"] as? String == "profile-A", "active wiring-profile change on the same display cannot retarget an open editor")
core.actions = []
let selected = core.current.profiles[1]
store.useProfile(selected)
expect(core.actions.last?["type"] as? String == "profile_select" && core.actions.last?["profile_id"] as? String == "profile-B", "manual use sends profile identity rather than its shared display identity")
expect(core.actions.allSatisfy { !["save", "workspace_save", "automatic", "manual"].contains($0["type"] as? String ?? "") }, "manual profile selection never fabricates configuration, input or automation actions")
core.current.profile_selection = ProfileSelection(mode: "manual", selected_profile_id: "profile-B", selected_display_id: "display-A", selected_profile_name: "Home · HDMI 2", availability: "waiting", reason: "selected_display_missing", message: "Waiting for the selected display.")
core.current.config = nil; core.current.active_display_id = nil; core.current.active_profile_id = nil
store.snapshot = core.current
expect(store.profileSelectionTitle == "Manually selected: Home · HDMI 2" && store.profileSelectionMessage.contains("Waiting"), "offline manual target is presented as waiting without pretending it is active")
expect(!store.canSwitch, "offline selected profile never exposes hardware input switching")
store.restoreAutomaticProfiles()
expect(core.actions.last?["type"] as? String == "profile_restore_auto", "restore auto sends only the explicit session selection action")
core.current.profile_selection?.mode = "auto"; core.current.profile_selection?.selected_profile_name = nil; store.tick()
expect(store.profileSelectionTitle == "Automatic profile detection", "automatic mode is displayed only from returned core state")
// Exercise the actual NSMenu with MemoryCore; construction/rendering must never dispatch policy actions.
_ = NSApplication.shared
NSApp.setActivationPolicy(.prohibited)
core.current = try baseSnapshot(); core.current.profiles[1].display_id = "display-A"
store.snapshot = core.current; store.errorMessage = nil; core.actions = []
var confirmed = false
var confirmations = 0
let nativeMenu = StatusMenu(model: store, confirm: { _ in confirmations += 1; return confirmed })
nativeMenu.menuNeedsUpdate(nativeMenu.menu)
expect(core.actions.isEmpty && confirmations == 0, "native menu construction never executes controller actions or opens confirmation")
expect(nativeMenu.menu.items.prefix(2).allSatisfy { !$0.isEnabled } && nativeMenu.menu.items.allSatisfy { $0.submenu != nil || $0.view == nil }, "native menu uses disabled compact status rows and native menu items")
func invoke(_ item: NSMenuItem) { _ = (item.target as? NSObject)?.perform(item.action!, with: item) }
let localSwitch = nativeMenu.menu.items.first { $0.representedObject as? String == "local" }!
invoke(localSwitch)
expect(confirmations == 1 && core.actions.isEmpty, "native switch cancellation sends no request")
confirmed = true; invoke(localSwitch)
expect(confirmations == 2 && core.actions.last?["type"] as? String == "manual" && core.actions.last?["target"] as? String == "local", "confirmed native switch uses the existing local manual action")
let profilesMenu = nativeMenu.menu.items.compactMap { $0.submenu }.first!
expect(profilesMenu.items.first?.state == .on && profilesMenu.items.first { $0.representedObject as? String == "profile-A" }?.state == .on, "native profile submenu distinguishes automatic detection and exact active profile")
invoke(profilesMenu.items.first { $0.representedObject as? String == "profile-B" }!)
expect(core.actions.last?["type"] as? String == "profile_select" && core.actions.last?["profile_id"] as? String == "profile-B", "native profile submenu selects exact identity despite shared display")
invoke(profilesMenu.items.first!)
expect(core.actions.last?["type"] as? String == "profile_restore_auto", "native auto profile entry dispatches only restore auto")
core.actions = []; core.current.sleeping = true; store.snapshot = core.current
nativeMenu.menuNeedsUpdate(nativeMenu.menu)
expect(!nativeMenu.menu.items.first { $0.representedObject as? String == "local" }!.isEnabled, "native switch item is disabled during lifecycle suspension")
invoke(localSwitch)
expect(core.actions.isEmpty && confirmations == 2, "stale native item revalidates switch safety before confirmation")
expect(nativeMenu.menu.items.first { $0.title == "Automatic switching on this computer" }?.state == .on, "native automatic item reflects persisted local request with a checkmark")
core.current.sleeping = false; store.snapshot = core.current; core.actions = []
nativeMenu.menuNeedsUpdate(nativeMenu.menu)
let automaticItem = nativeMenu.menu.items.first { $0.title == "Automatic switching on this computer" }!
invoke(automaticItem)
expect(core.actions.last?["type"] as? String == "automatic" && core.actions.last?["enabled"] as? Bool == false, "native automatic check item toggles only the local automatic request")
core.actions = []; core.current.config?.automatic = false; core.current.workspace = Workspace(); core.current.workspace?.can_enable = false; store.snapshot = core.current
nativeMenu.menuNeedsUpdate(nativeMenu.menu)
expect(nativeMenu.menu.items.first { $0.title == "Automatic switching on this computer" }?.isEnabled == false, "native automatic enable respects qualification gate")
invoke(automaticItem)
expect(core.actions.isEmpty, "stale native automatic item rechecks qualification before enabling")
core.current = try baseSnapshot(); store.snapshot = core.current; core.actions = []
let changingMenu = StatusMenu(model: store, confirm: { _ in
    core.current.active_profile_id = "profile-B"; core.current.active_display_id = "display-B"; core.current.config?.display_id = "display-B"; store.snapshot = core.current
    return true
})
changingMenu.menuNeedsUpdate(changingMenu.menu)
invoke(changingMenu.menu.items.first { $0.representedObject as? String == "local" }!)
expect(store.canSwitch && core.actions.isEmpty, "native confirmation pins active profile identity across modal runloop environment changes")
core.current = try baseSnapshot(); core.current.sleeping = true; core.current.lifecycle_ok = false; store.snapshot = core.current; core.actions = []
nativeMenu.menuNeedsUpdate(nativeMenu.menu)
let pausedRecoveryItem = nativeMenu.menu.items.first { $0.title == "Automatic switching on this computer" }!
expect(pausedRecoveryItem.isEnabled && pausedRecoveryItem.state == .on && !store.canSwitch, "native checked automatic item allows safe local pause while switching is unavailable")
invoke(pausedRecoveryItem)
expect(core.actions.last?["type"] as? String == "automatic" && core.actions.last?["enabled"] as? Bool == false, "native recovery pause does not require a hardware-ready profile")
print("Mac state orchestration checks: \(checks) passed")
