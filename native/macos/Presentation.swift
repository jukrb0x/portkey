import Foundation

struct Device: Codable, Equatable, Identifiable {
    var id: String
    var label: String
}
struct Configuration: Codable, Equatable {
    var version = 1
    var platform = "macos"
    var display_id = ""
    var local_input = 17
    var remote_input = 15
    var hubs: [Device] = []
    var automatic = false
    var language = "en"
}
struct Inventory: Decodable, Equatable { var displays: [Device]; var hubs: [Device] }
struct DirectionEvidence: Codable, Equatable {
    var result = "unverified"
    var source = "none"
    var note = ""
}
struct Evidence: Codable, Equatable {
    var local = DirectionEvidence()
    var remote = DirectionEvidence()
}
struct Workspace: Codable, Equatable {
    var local_name = "This computer"
    var remote_name = "Other computer"
    var local_os = "macos"
    var remote_os = "unknown"
    var local_role = "a"
    var workspace_id = ""
    var revision = 1
    var mode = "unverified"
    var single_controller = "local"
    var verification = "not_checked"
    var evidence = Evidence()
    var peer_confirmed = false
    var peer_paused_confirmed = false
    var peer_runtime: String?
    var automatic_requested: Bool?
    var effective_state: String?
    var can_enable: Bool?
    var qualification_current: Bool?
    var actionValue: [String: Any] {
        var value = (try? JSONSerialization.jsonObject(with: JSONEncoder().encode(self))) as? [String: Any] ?? [:]
        for key in ["peer_runtime", "automatic_requested", "effective_state", "can_enable", "qualification_current"] { value.removeValue(forKey: key) }
        if workspace_id.isEmpty { value.removeValue(forKey: "workspace_id") }
        return value
    }
}
struct PeerPreview: Decodable, Equatable {
    var version: Int?
    var workspace_id: String?
    var revision: Int?
    var local_role: String?
    var local_name: String?
    var remote_name: String?
    var local_input: Int?
    var remote_input: Int?
}
struct SetupState: Decodable, Equatable {
    var phase: String
    var error: Bool?
    var code: String?
    var message: String?
    var name: String?
    var display_id: String?
    var local_input: Int?
    var remote_input: Int?
    var workspace: Workspace?
    var draft_saved: Bool?
    var input_role: String?
    var peer_preview: PeerPreview?
    var profile_id: String?
    var target_profile_id: String?
    var draft_id: String?
    var editing_saved_profile: Bool?
    var qualification_current: Bool?
}
struct SavedProfile: Decodable, Equatable, Identifiable {
    var name: String
    var display_id: String
    var profile_id: String?
    var local_input: Int?
    var remote_input: Int?
    var hub_count: Int?
    var workspace: Workspace?
    var id: String { profile_id ?? display_id }
}
struct DisplayState: Decodable, Equatable {
    var result: String?
    var target: String?
    var input: Int?
    var visual_confirmed: Bool?
}
struct ProfileSelection: Decodable, Equatable {
    var mode: String
    var selected_profile_id: String?
    var selected_display_id: String?
    var selected_profile_name: String?
    var availability: String
    var reason: String
    var message: String
}
struct Snapshot: Decodable, Equatable {
    var setup: SetupState?
    var profiles: [SavedProfile]
    var active_display_id: String?
    var active_profile_id: String?
    var profile_selection: ProfileSelection?
    var profile_name: String
    var config: Configuration?
    var inventory: Inventory?
    var ownership: String
    var status: String
    var error: Bool
    var lifecycle_ok: Bool
    var sleeping: Bool
    var lifecycle_epoch: UInt64?
    var language: String
    var config_path: String
    var editing: Bool
    var workspace: Workspace?
    var draft_available: Bool?
    var display_state: DisplayState?
    var startup_error: String?
    var controls_available: Bool?
    var warning: String?

    var canSwitch: Bool {
        startup_error == nil && controls_available != false && config != nil && active_display_id == config?.display_id && !editing && lifecycle_ok && !sleeping
    }
    var activeProfile: SavedProfile? {
        if let id = active_profile_id { return profiles.first { $0.id == id } }
        let matching = profiles.filter { $0.display_id == active_display_id }
        return matching.count == 1 ? matching.first : nil
    }
    var localName: String { ComputerNamePresentation.name(local: true, saved: activeProfile?.workspace, runtime: workspace, language: language) }
    var remoteName: String { ComputerNamePresentation.name(local: false, saved: activeProfile?.workspace, runtime: workspace, language: language) }
    // Only structured runtime evidence may establish current input; USB and saved qualification cannot.
    var inputEvidence: String { display_state?.result ?? "unconfirmed" }
    var effectiveState: String {
        if !lifecycle_ok { return "unavailable" }
        if sleeping { return "sleeping" }
        if editing { return "editing" }
        if config == nil || error { return "attention" }
        if let effective = workspace?.effective_state { return effective }
        if config?.automatic != true { return "paused" }
        return ownership == "owned" ? "active" : "waiting_usb"
    }
}
struct WorkspaceDraft: Codable, Equatable {
    var name = ""
    var localName = ""
    var remoteName = ""
    var remoteOS = "windows"
    var localRole = "a"
    var mode = "dual"
    var singleController = "local"
    var displayID = ""
    var peerCode = ""
    var step = 0
    var localObserved = false
    var remoteObserved = false
    var peerPaused = false
    var evidenceNote = ""
    var profileID: String?
    mutating func clearAttestations() { localObserved = false; remoteObserved = false; peerPaused = false }
    var hasContent: Bool { [name, localName, remoteName, peerCode, displayID].contains { !$0.isEmpty } }
    var validationError: String? {
        let values = [name, localName, remoteName].map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        guard values.allSatisfy({ !$0.isEmpty && $0.count <= 80 && !$0.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) }) else { return "names" }
        return values[1].compare(values[2], options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame ? "distinct" : nil
    }
    func matchesNames(in setup: SetupState) -> Bool {
        guard let workspace = setup.workspace else { return false }
        return name == setup.name && localName == workspace.local_name && remoteName == workspace.remote_name && remoteOS == workspace.remote_os && localRole == workspace.local_role && mode == workspace.mode && singleController == workspace.single_controller
    }
    func metadataChanges(from original: WorkspaceDraft) -> [String: Any] {
        var fields: [String: Any] = [:]
        let pairs = [("local_name", localName, original.localName), ("remote_name", remoteName, original.remoteName),
                     ("remote_os", remoteOS, original.remoteOS), ("local_role", localRole, original.localRole),
                     ("mode", mode, original.mode), ("single_controller", singleController, original.singleController)]
        for (key, current, previous) in pairs where current.trimmingCharacters(in: .whitespacesAndNewlines) != previous.trimmingCharacters(in: .whitespacesAndNewlines) {
            fields[key] = current.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if ["remote_os", "local_role", "mode", "single_controller"].contains(where: { fields[$0] != nil }) { fields["verification"] = "not_checked" }
        return fields
    }
    func workspace(preserving existing: Workspace? = nil) -> Workspace {
        var result = existing ?? Workspace()
        result.local_name = localName.trimmingCharacters(in: .whitespacesAndNewlines)
        result.remote_name = remoteName.trimmingCharacters(in: .whitespacesAndNewlines)
        if result.mode != mode || result.single_controller != singleController || result.local_role != localRole || result.remote_os != remoteOS {
            result.verification = "not_checked"
            result.evidence = Evidence()
        }
        result.remote_os = remoteOS
        result.local_role = localRole
        result.mode = mode
        result.single_controller = singleController
        return result
    }
}
enum DraftStorageKey {
    static func make(configPath: String, profileID: String) -> String {
        // Base64 namespaces local drafts; it is not encryption.
        "PortKey.productDraft." + Data((configPath + "\u{0}" + profileID).utf8).base64EncodedString()
    }
}

// Persisted metadata distinguishes an explicit user name from the core's generated legacy labels.
enum ComputerNamePresentation {
    static func name(local: Bool, saved: Workspace?, runtime: Workspace?, language: String) -> String {
        if let saved { return local ? saved.local_name : saved.remote_name }
        let generated = local ? "This computer" : "Other computer"
        let value = local ? runtime?.local_name : runtime?.remote_name
        if let value, value != generated { return value }
        return language == "zh-Hans" ? (local ? "这台电脑" : "另一台电脑") : generated
    }
}
