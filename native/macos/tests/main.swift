import Foundation

var checks = 0
func expect(_ condition: @autoclosure () -> Bool, _ message: String) {
    checks += 1
    if !condition() { fputs("FAIL: \(message)\n", stderr); exit(1) }
}
func snapshot(_ extra: String = "") throws -> Snapshot {
    let json = """
    {"profiles":[],"active_display_id":"exact-display","profile_name":"Home","config":{"version":1,"platform":"macos","display_id":"exact-display","local_input":5,"remote_input":7,"hubs":[],"automatic":true,"language":"en"},"inventory":{"displays":[],"hubs":[]},"ownership":"owned","status":"","error":false,"lifecycle_ok":true,"sleeping":false,"language":"en","config_path":"/private/config","editing":false\(extra)}
    """
    return try JSONDecoder().decode(Snapshot.self, from: Data(json.utf8))
}
var normal = try snapshot()
expect(normal.canSwitch, "valid enrolled awake profile allows a local manual request")
expect(normal.inputEvidence == "unconfirmed", "USB ownership must never imply display input confirmation")
normal.ownership = "absent"
expect(normal.inputEvidence == "unconfirmed", "absence must not imply remote input")
normal.editing = true
expect(!normal.canSwitch, "editing must block manual actions")
normal.editing = false; normal.sleeping = true
expect(!normal.canSwitch, "sleep must block manual actions")
normal.sleeping = false; normal.lifecycle_ok = false
expect(!normal.canSwitch, "lifecycle failure must block manual actions")
normal.lifecycle_ok = true; normal.active_display_id = nil
expect(!normal.canSwitch, "unmatched display must block manual actions")
let legacy = try snapshot()
expect(legacy.localName == "This computer", "missing product metadata has neutral local name")
expect(legacy.remoteName == "Other computer", "missing product metadata does not guess Windows")
var draft = WorkspaceDraft()
draft.localName = "  Work  "; draft.remoteName = "work"
expect(draft.validationError != nil, "indistinguishable computer names rejected")
draft.remoteName = "Personal"; draft.name = "Home"
expect(draft.validationError == nil, "distinct trimmed names accepted")
draft.displayID = "display-A"; draft.peerCode = "offline-record"; draft.step = 2
let roundtrip = try JSONDecoder().decode(WorkspaceDraft.self, from: JSONEncoder().encode(draft))
expect(roundtrip == draft, "draft roundtrip retains role, mapping and progress")
let locationA = DraftStorageKey.make(configPath: "/a", profileID: "profile-1")
expect(locationA != DraftStorageKey.make(configPath: "/b", profileID: "profile-1"), "isolated config drafts cannot leak into live setup")
expect(locationA != DraftStorageKey.make(configPath: "/a", profileID: "profile-2"), "distinct profile drafts remain separate even when hardware is shared")
let confirmed = try snapshot(",\"display_state\":{\"result\":\"readback_confirmed\",\"target\":\"remote\",\"input\":7,\"visual_confirmed\":false}")
expect(confirmed.inputEvidence == "readback_confirmed", "runtime readback contract decodes independently of USB ownership")
let pending = try snapshot(",\"display_state\":{\"result\":\"pending\",\"target\":\"local\",\"input\":5,\"visual_confirmed\":false}")
expect(pending.inputEvidence == "pending", "pending target stays pending rather than confirmed")
var transientWorkspace = Workspace()
transientWorkspace.peer_runtime = "unknown"; transientWorkspace.can_enable = true; transientWorkspace.effective_state = "active"
let payload = transientWorkspace.actionValue
expect(payload["peer_runtime"] == nil && payload["can_enable"] == nil && payload["effective_state"] == nil, "read-only snapshot fields never leak into strict core action payload")
expect(payload["workspace_id"] == nil, "new workspace lets core generate opaque identity")

var unfinished = WorkspaceDraft()
unfinished.localName = "Unsaved local name"
expect(unfinished.hasContent, "incomplete name draft remains resumable before core creation")
expect(!WorkspaceDraft().hasContent, "blank defaults do not create a phantom draft")
var oldQualified = Workspace()
oldQualified.mode = "dual"; oldQualified.remote_os = "windows"; oldQualified.verification = "dual_observed"
oldQualified.local_name = "Work"; oldQualified.remote_name = "Personal"
oldQualified.evidence.local.result = "observed"; oldQualified.evidence.local.source = "user_observed"
oldQualified.evidence.remote.result = "observed"; oldQualified.evidence.remote.source = "user_observed"
var edit = WorkspaceDraft(); edit.name = "Home"; edit.localName = "Renamed"; edit.remoteName = "Personal"
expect(edit.workspace(preserving: oldQualified).verification == "dual_observed", "renaming preserves explicit evidence")
edit.mode = "single"
expect(edit.workspace(preserving: oldQualified).verification == "not_checked", "changing controller plan cannot carry old qualification")
var oldObservationDraft = WorkspaceDraft()
oldObservationDraft.localObserved = true; oldObservationDraft.remoteObserved = true; oldObservationDraft.peerPaused = true
oldObservationDraft.clearAttestations()
expect(!oldObservationDraft.localObserved && !oldObservationDraft.remoteObserved && !oldObservationDraft.peerPaused, "new binding observations require fresh explicit attestation")
var chineseLegacy = try snapshot()
chineseLegacy.language = "zh-Hans"; chineseLegacy.workspace = Workspace()
chineseLegacy.profiles = [SavedProfile(name: "Home", display_id: "exact-display")]
expect(chineseLegacy.localName == "这台电脑" && chineseLegacy.remoteName == "另一台电脑", "generated active legacy names follow Chinese presentation language")
chineseLegacy.profiles[0].workspace = Workspace()
expect(chineseLegacy.localName == "This computer" && chineseLegacy.remoteName == "Other computer", "explicitly saved literal names are never translated")
chineseLegacy.profiles[0].workspace?.local_name = "My Mac"
expect(chineseLegacy.localName == "My Mac", "saved user names remain verbatim across presentation language")
let warningSnapshot = try snapshot(",\"warning\":\"工作空间已保存，但无法移除旧草稿。\"")
expect(warningSnapshot.warning != nil && !warningSnapshot.error && warningSnapshot.canSwitch, "cleanup warning decodes without converting a committed save into failure")
print("Mac presentation checks: \(checks) passed")
