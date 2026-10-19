import AppKit
import SwiftUI

private let surface = Color(nsColor: .controlBackgroundColor)
private let canvas = Color(nsColor: .windowBackgroundColor)
private let hairline = Color.primary.opacity(0.09)
private let productAccent = Color.accentColor

struct Surface<Content: View>: View {
    let content: Content
    init(@ViewBuilder content: () -> Content) { self.content = content() }
    var body: some View { Section { content } }
}
struct Muted: View {
    let value: String
    init(_ value: String) { self.value = value }
    var body: some View { Text(value).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
}
struct SectionHeading: View {
    let title: String
    let subtitle: String
    var body: some View { VStack(alignment: .leading, spacing: 6) { Text(title).font(.largeTitle).fontWeight(.bold); Muted(subtitle) }.padding(.bottom, 6) }
}
struct ExpandableDetails<Content: View>: View {
    let title: String
    let language: String
    let content: Content
    @State private var expanded = false
    init(_ title: String, language: String, @ViewBuilder content: () -> Content) { self.title = title; self.language = language; self.content = content() }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Button { expanded.toggle() } label: {
                HStack(spacing: 7) {
                    Image(systemName: expanded ? "chevron.down" : "chevron.right").font(.caption).foregroundStyle(.secondary).frame(width: 12)
                    Text(title)
                    Spacer()
                }.padding(.vertical, 6).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
            }.buttonStyle(.plain).accessibilityLabel(title)
                .accessibilityValue(language == "zh-Hans" ? (expanded ? "已展开" : "已收起") : (expanded ? "Expanded" : "Collapsed"))
            if expanded { content }
        }.frame(maxWidth: .infinity, alignment: .leading)
    }
}
struct DetailRow: View {
    var title: String
    var subtitle: String? = nil
    var value: String
    var body: some View {
        HStack(alignment: .center, spacing: 20) {
            VStack(alignment: .leading, spacing: 4) { Text(title).fontWeight(.medium); if let subtitle { Muted(subtitle) } }
            Spacer(minLength: 15)
            Text(value).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.trailing).frame(maxWidth: 290, alignment: .trailing).textSelection(.enabled)
        }.padding(.vertical, 4)
    }
}
struct Notice: View {
    var title: String
    var detail: String
    var symbol = "info.circle"
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: symbol).foregroundStyle(productAccent).font(.title3)
            VStack(alignment: .leading, spacing: 5) { Text(title).fontWeight(.semibold); Muted(detail) }
        }.padding(.vertical, 5).frame(maxWidth: .infinity, alignment: .leading)
    }
}
// Keep one fixed-size title instead of morphing the native window title on page changes.
private struct PageToolbarTitle: ToolbarContent {
    let title: String
    @ToolbarContentBuilder var body: some ToolbarContent {
        if #available(macOS 26, *) {
            ToolbarItem(id: "page-title", placement: .navigation) { label }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(id: "page-title", placement: .navigation) { label }
        }
    }
    private var label: some View {
        Text(title)
            .font(.system(size: 13, weight: .semibold))
            .fixedSize()
            .transaction { transaction in
                transaction.animation = nil
                transaction.disablesAnimations = true
            }
            .accessibilityAddTraits(.isHeader)
    }
}
struct ProductRoot: View {
    @ObservedObject var model: PortKeyStore
    @State private var visibility: NavigationSplitViewVisibility = .all
    var body: some View {
        NavigationSplitView(columnVisibility: $visibility) {
            List(selection: Binding<Page?>(get: { model.page }, set: { if let page = $0 { model.page = page } })) {
                Label(model.text("Overview", "概览"), systemImage: "display").tag(Page.overview)
                Label(model.text("Profiles", "配置"), systemImage: "square.stack.3d.up").tag(Page.workspaces)
                Label(model.text("Setup guide", "设置引导"), systemImage: "cable.connector").tag(Page.setup)
                Label(model.text("Diagnostics", "诊断"), systemImage: "waveform.path").tag(Page.diagnostics)
                Label(model.text("Settings", "设置"), systemImage: "gearshape").tag(Page.preferences)
            }
            .listStyle(.sidebar)
            .navigationTitle("PortKey")
            .navigationSplitViewColumnWidth(min: 170, ideal: 200, max: 250)
            .safeAreaInset(edge: .bottom, alignment: .leading) {
                Label {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(model.localName).font(.callout).fontWeight(.medium)
                        Text(model.text("Two computers. One workspace.", "两台电脑，一个工作空间。")).font(.caption).foregroundStyle(.secondary)
                    }
                } icon: { Image(systemName: "arrow.left.arrow.right.square.fill").font(.title2).foregroundStyle(.tint) }
                .padding(16)
            }
        } detail: {
            Form {
                if let failure = model.bridgeFailure {
                    Section { Notice(title: model.text("PortKey needs to restart", "需要重启 PortKey"), detail: failure, symbol: "exclamationmark.triangle") }
                }
                if let startup = model.snapshot?.startup_error {
                    Section {
                        Label(model.text("Another controller or startup problem needs attention", "需要解决另一个控制程序或启动问题"), systemImage: "exclamationmark.triangle").font(.headline)
                        Text(startup).foregroundStyle(.secondary).textSelection(.enabled)
                        HStack {
                            Button(model.text("Read-only diagnostics…", "只读诊断…")) { model.page = .diagnostics }.disabled(!model.canCollectDiagnostics)
                            Button(model.text("Quit this instance", "退出此程序")) { NSApp.terminate(nil) }
                        }
                    }
                }
                if model.canShowSelectedPage {
                switch model.page {
                case .overview: OverviewView(model: model)
                case .workspaces: WorkspacesView(model: model)
                case .setup: SetupView(model: model).disabled(!model.canConfigure)
                case .diagnostics: DiagnosticsView(model: model)
                case .preferences: PreferencesView(model: model)
                }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("PortKey")
            .toolbar {
                PageToolbarTitle(title: pageTitle)
            }
        }
        .frame(minWidth: 900, minHeight: 680)
        .alert("PortKey", isPresented: Binding(get: { model.errorMessage != nil }, set: { if !$0 { model.errorMessage = nil } })) {
            Button(model.text("OK", "好"), role: .cancel) { model.errorMessage = nil }
        } message: { Text(model.errorMessage ?? "") }
        .confirmationDialog(model.text("Switch display to ", "将显示器切换到 ") + (model.pendingTarget == "local" ? model.localName : model.remoteName), isPresented: Binding(get: { model.pendingTarget != nil }, set: { if !$0 { model.pendingTarget = nil } }), titleVisibility: .visible) {
            Button(model.text("Send display request", "发送显示器请求")) { model.sendManual() }
            Button(model.text("Cancel", "取消"), role: .cancel) { model.pendingTarget = nil }
        } message: { Text(model.text("USB stays with the physical selector. This request holds locally until a genuine USB change. The other computer’s automatic rules are not paused.", "USB 仍由实体切换器控制。本机手动保持直到 USB 真实变化，不会暂停另一台电脑的自动规则。")) }
        .sheet(isPresented: $model.editingMetadata) { MetadataEditor(model: model) }
        .onChange(of: model.draft) { _ in model.persistDraft() }
    }
    private var pageTitle: String {
        switch model.page {
        case .overview: return model.text("Overview", "概览")
        case .workspaces: return model.text("Profiles", "配置")
        case .setup: return model.text("Setup guide", "设置引导")
        case .diagnostics: return model.text("Diagnostics", "诊断")
        case .preferences: return model.text("Settings", "设置")
        }
    }
}

struct ProfileSelectionSummary: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        Surface {
            VStack(alignment: .leading, spacing: 12) {
                Label(model.profileSelectionTitle, systemImage: model.isManualProfileSelection ? "hand.point.up.left" : "arrow.triangle.2.circlepath").font(.headline)
                Muted(model.profileSelectionMessage)
                DetailRow(title: model.text("Currently in use", "当前正在使用"), value: model.snapshot?.activeProfile?.name ?? model.text("No profile currently in use", "当前未使用配置"))
                if model.isManualProfileSelection {
                    Button(model.text("Restore automatic profile detection", "恢复自动识别配置")) { model.restoreAutomaticProfiles() }.disabled(!model.canChooseProfiles)
                }
            }
        }
    }
}

struct OverviewView: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        SectionHeading(title: model.snapshot?.activeProfile?.name ?? model.text("Your profiles", "你的配置"), subtitle: model.snapshot?.activeProfile == nil ? model.text("Connect an enrolled display, or choose the profile for this connection.", "连接已绑定的显示器，或选择此连接的配置。") : model.displayName)
        ProfileSelectionSummary(model: model)
        if let warning = model.warningMessage {
            Notice(title: model.text("Saved · draft cleanup needs attention", "已保存 · 需要处理草稿清理"), detail: warning, symbol: "exclamationmark.triangle")
        }
        if model.snapshot?.config == nil && model.snapshot?.profiles.isEmpty == false {
            Surface {
                VStack(alignment: .leading, spacing: 15) {
                    Label(model.isManualProfileSelection ? model.text("Waiting for the selected display", "等待所选配置的显示器") : model.text("Choose or connect a saved profile", "选择或连接已保存的配置"), systemImage: "display").font(.title3).fontWeight(.semibold)
                    Muted(model.profileSelectionMessage)
                    Button(model.text("View profiles", "查看配置")) { model.page = .workspaces }
                }
            }
        } else if model.snapshot?.config == nil {
            Surface {
                VStack(alignment: .leading, spacing: 18) {
                    Image(systemName: "laptopcomputer.and.arrow.down").font(.system(size: 40, weight: .light)).foregroundStyle(productAccent)
                    Text(model.text("Let your display follow you.", "让显示器跟随你的操作。")).font(.title2).fontWeight(.semibold)
                    Muted(model.text("PortKey follows one physical USB switch between two named computers. Start by choosing your monitor and learning the switch.", "PortKey 跟随实体 USB 切换器，在两台已命名的电脑间切换画面。首先选择显示器并识别切换器。"))
                    Button(model.snapshot?.draft_available == true ? model.text("Continue saved setup", "继续已保存的设置") : model.text("Create a profile", "新建配置")) { model.beginSetup() }.buttonStyle(.borderedProminent)
                }
            }
        } else {
            Surface {
                VStack(alignment: .leading, spacing: 21) {
                    HStack(spacing: 17) {
                        Image(systemName: "display").font(.system(size: 29, weight: .light)).foregroundStyle(productAccent)
                            .frame(width: 54, height: 54)
                        VStack(alignment: .leading, spacing: 6) { Text(model.text("Display input", "显示器输入")).font(.caption).foregroundStyle(.secondary); Text(model.inputTitle).font(.title2).fontWeight(.semibold); Muted(model.inputSubtitle) }
                    }
                    HStack(spacing: 14) {
                        ComputerCard(model: model, local: true)
                        ComputerCard(model: model, local: false)
                    }
                }
            }
            Surface {
                VStack(spacing: 13) {
                    DetailRow(title: model.text("Local USB", "本机 USB"), subtitle: model.text("The enrolled physical switch", "已绑定的实体切换器"), value: model.usbTitle)
                    Divider()
                    Toggle(isOn: Binding(get: { model.snapshot?.config?.automatic == true }, set: model.toggleAutomation)) {
                        VStack(alignment: .leading, spacing: 4) { Text(model.text("Automatic on \(model.localName)", "\(model.localName)上的自动切换")).fontWeight(.medium); Muted(model.effectiveTitle) }
                    }.toggleStyle(.switch).disabled(!model.canSwitch || (model.snapshot?.config?.automatic != true && model.snapshot?.workspace?.can_enable == false))
                }
            }
            if model.snapshot?.effectiveState != "active" && model.snapshot?.effectiveState != "paused" {
                Notice(title: model.effectiveTitle, detail: model.snapshot?.status.isEmpty == false ? model.snapshot!.status : model.text("Waiting for a fresh, complete device observation. No remote ownership is inferred.", "等待完整且最新的设备状态，不推断另一台的归属。"))
            } else if model.snapshot?.status.isEmpty == false {
                Muted(model.snapshot!.status).textSelection(.enabled)
            }
            Notice(title: model.modeTitle, detail: model.snapshot?.workspace?.mode == "single" ? model.text("The designated controller must stay awake and running. Both computers still need an available video signal.", "指定的控制电脑必须保持唤醒并运行。两台电脑仍需提供可用的视频信号。") : model.text("The other PortKey instance’s runtime is unknown. Pause and manual selection apply only to this computer.", "另一台的 PortKey 运行状态未知。暂停和手动选择仅在本机生效。"))
        }
        if model.hasUnfinishedSetup {
            HStack { Label(model.text("An unfinished setup is saved", "已保存尚未完成的设置"), systemImage: "bookmark"); Spacer(); Button(model.text("Continue", "继续")) { model.beginSetup() } }.font(.callout)
        }
        HStack { Button(model.text("Manage profiles", "管理配置")) { model.page = .workspaces }; Spacer(); Text(model.text("Runs in the menu bar when this window closes.", "关闭窗口后继续在菜单栏运行。")).font(.caption).foregroundStyle(.secondary) }
    }
}
struct ComputerCard: View {
    @ObservedObject var model: PortKeyStore
    let local: Bool
    var body: some View {
        GroupBox {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Image(systemName: local ? "laptopcomputer" : "desktopcomputer").font(.title2).foregroundStyle(productAccent)
                Spacer()
                Text(model.text(local ? "This computer" : "Other computer", local ? "本机" : "另一台"))
                    .font(.caption2).foregroundStyle(.secondary).padding(.horizontal, 7).padding(.vertical, 3).background(canvas, in: Capsule())
            }
            VStack(alignment: .leading, spacing: 5) {
                Text(local ? model.localName : model.remoteName).font(.headline).lineLimit(2)
                Muted(local ? "macOS" : (model.snapshot?.workspace?.remote_os == "macos" ? "macOS" : model.snapshot?.workspace?.remote_os == "windows" ? "Windows" : model.text("OS not recorded", "未记录系统")))
            }
            Button(model.text("Switch display here", "切换画面到这里")) { model.requestManual(local ? "local" : "remote") }.frame(maxWidth: .infinity).disabled(!model.canSwitch)
        }.padding(8).frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct WorkspacesView: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        SectionHeading(title: model.text("Profiles", "配置"), subtitle: model.text("Save a profile for each display connection, including different cables on the same monitor.", "为每种显示器连接保存配置，也可区分同一显示器的不同线缆。"))
        ProfileSelectionSummary(model: model)
        if model.snapshot?.profiles.isEmpty != false {
            Surface { VStack(alignment: .leading, spacing: 15) { Text(model.text("Make yourself at home.", "从你的第一处环境开始。")).font(.title2).fontWeight(.semibold); Muted(model.text("No saved profile yet. Your hardware is never selected by guesswork.", "尚无已保存的配置。设备始终需要明确绑定。")); Button(model.text("Add profile", "添加配置")) { model.beginSetup() }.buttonStyle(.borderedProminent) } }
        } else {
            Picker(model.text("Viewing profile", "正在查看配置"), selection: Binding(get: { model.selectedProfile?.id ?? "" }, set: { model.selectedProfileID = $0 })) {
                ForEach(model.snapshot?.profiles ?? []) { profile in Text(profile.name).tag(profile.id) }
            }
            if let profile = model.selectedProfile {
                let workspace = profile.workspace ?? (model.isActiveProfile(profile) ? model.snapshot?.workspace : nil)
                Surface {
                    VStack(alignment: .leading, spacing: 18) {
                        HStack { VStack(alignment: .leading, spacing: 5) { Text(profile.name).font(.title2).fontWeight(.semibold); Muted(model.isActiveProfile(profile) ? model.text("Currently in use", "当前正在使用") : (model.isManualProfileSelection && model.snapshot?.profile_selection?.selected_profile_id == profile.id ? model.profileSelectionMessage : model.text("Saved profile · not currently in use", "已保存的配置 · 当前未使用"))) }; Spacer(); Image(systemName: "display").font(.title).foregroundStyle(.secondary) }
                        HStack(alignment: .top, spacing: 24) {
                            profileComputer(model.computerName(in: profile, local: true), "macOS", model.text("Local identity stays on this computer", "本机设备身份仅保存在本机"))
                            profileComputer(model.computerName(in: profile, local: false), workspace?.remote_os == "macos" ? "macOS" : workspace?.remote_os == "windows" ? "Windows" : model.text("OS not recorded", "未记录系统"), model.text("Saved role · runtime unknown", "已保存角色 · 运行状态未知"))
                        }
                        Divider()
                        DetailRow(title: model.text("Installation plan", "运行方案"), value: model.modeTitle(workspace))
                        DetailRow(title: model.text("Qualification", "验证记录"), value: model.verificationTitle(workspace?.verification ?? "historical_unknown"))
                        DetailRow(title: model.text("Device binding", "设备绑定"), value: model.text("Exact display + local USB topology", "明确的显示器 + 本机 USB 拓扑"))
                        ExpandableDetails(model.text("Saved input mapping details", "已保存的输入映射详情"), language: model.language) {
                            VStack(spacing: 10) {
                                DetailRow(title: model.computerName(in: profile, local: true), value: profile.local_input.map { model.text("Input ", "输入 ") + String($0) } ?? model.text("Not recorded", "未记录"))
                                DetailRow(title: model.computerName(in: profile, local: false), value: profile.remote_input.map { model.text("Input ", "输入 ") + String($0) } ?? model.text("Not recorded", "未记录"))
                                DetailRow(title: model.text("Local switch topology", "本机切换器拓扑"), value: profile.hub_count.map { String($0) + model.text(" enrolled USB branches", " 个已绑定 USB 分支") } ?? model.text("Saved locally", "已保存在本机"))
                                Muted(model.text("Saved mappings are configuration records, not current input readback or peer presence.", "已保存映射是配置记录，不代表当前输入回报或另一台在线。"))
                            }.padding(.top, 12)
                        }
                        Divider()
                        HStack {
                            if !model.isActiveProfile(profile) {
                                Button(model.text("Use this profile", "使用此配置")) { model.useProfile(profile) }.buttonStyle(.borderedProminent).disabled(!model.canChooseProfiles)
                            }
                            Button(model.text("Edit setup…", "编辑设置…")) { model.editProfileSetup() }.disabled(!model.canConfigure || model.snapshot?.editing == true)
                            Button(model.text("Edit profile details…", "编辑配置详情…")) { model.editProfileDetails() }.disabled(!model.canConfigure || model.snapshot?.editing == true)
                        }
                        Muted(model.text("Changes this computer only; the other app is not updated.", "仅在这台电脑切换配置；另一台不会同步更改。"))
                    }
                }
            }
        }
        Notice(title: model.text("Viewing a profile does not apply it", "查看配置不会启用它"), detail: model.text("PortKey applies a profile only to its exact enrolled display. When several profiles share a display, choose the wiring profile you are using.", "PortKey 仅将配置用于其确切绑定的显示器。同一显示器有多个配置时，请选择当前使用的接线配置。"))
        Button { model.beginSetup() } label: { Label(model.snapshot?.draft_available == true ? model.text("Continue setup", "继续配置") : model.text("Add profile", "添加配置"), systemImage: "plus") }
    }
    private func profileComputer(_ name: String, _ os: String, _ foot: String) -> some View {
        VStack(alignment: .leading, spacing: 8) { Text(name).font(.headline); Muted(os); Muted(foot) }.frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct SetupView: View {
    @ObservedObject var model: PortKeyStore
    private var phase: String { model.snapshot?.setup?.phase ?? "names" }
    private var mappingReady: Bool { model.snapshot?.setup?.local_input != nil && model.snapshot?.setup?.remote_input != nil }
    private var usbReady: Bool { ["usb_ready", "reading_input", "exchange", "ready"].contains(phase) }
    @State private var editDetails = false
    var body: some View {
        Group {
        if model.snapshot?.setup?.editing_saved_profile == true && !editDetails {
            SectionHeading(title: model.text("Enable automatic switching", "启用自动切换"), subtitle: model.draft.name + model.text(" · Saved connections are retained. Editing pauses automatic switching on this Mac.", " · 已保留原有连接。编辑期间，本机自动切换暂停。"))
            Surface { reviewStep }
            HStack {
                Button(model.text("Change connection or installation plan", "修改连接或运行方案")) { editDetails = true; model.draft.step = 2 }
                Spacer()
                Button(model.text("Finish later", "稍后继续")) { model.suspendSetup() }
                Button(model.text("Cancel editing", "取消编辑")) { discard = true }
            }
        } else {
        SectionHeading(title: model.text("Connect your computers", "连接两台电脑"), subtitle: model.text("One shared monitor, one physical USB switch, two computers kept awake.", "一个共享显示器，一个实体 USB 切换器，两台保持唤醒的电脑。"))
        if model.snapshot?.draft_available == true && model.snapshot?.setup == nil {
            Notice(title: model.text("A saved setup is waiting", "有一份已保存的设置"), detail: model.text("Resume with fresh device observations. Pending hardware operations are never replayed.", "继续时重新观察设备，不会重放待处理的硬件操作。"))
            Button(model.text("Resume saved setup", "继续已保存的设置")) { model.beginSetup() }.buttonStyle(.borderedProminent)
        } else {
            HStack {
                Text(model.text("Step \(model.draft.step + 1) of 4", "第 \(model.draft.step + 1) 步，共 4 步")).foregroundStyle(.secondary)
                Spacer()
                Text(stepTitle).fontWeight(.medium)
            }.font(.callout).accessibilityElement(children: .combine)
            Surface {
                VStack(alignment: .leading, spacing: 20) {
                    switch model.draft.step {
                    case 1: bindingStep
                    case 2: installationStep
                    case 3: reviewStep
                    default: namesStep
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            if model.snapshot?.setup?.error == true {
                Notice(title: model.text("This step needs attention", "这一步需要检查"), detail: model.snapshot?.setup?.message ?? model.snapshot?.status ?? "", symbol: "exclamationmark.triangle")
            }
            if model.snapshot?.setup != nil {
                Notice(title: model.text("Recovery is always physical", "始终可以使用实体控制恢复"), detail: model.text("If control moves away, return USB with its physical selector and choose the known working computer in the monitor’s input menu. PortKey cannot guarantee a software return from an inactive source.", "控制转移后，请用 USB 实体切换器返回，再用显示器输入菜单选择已知可用的电脑。PortKey 无法保证从非活动输入通过软件切回。"))
            }
            HStack(spacing: 12) {
                if model.draft.step > 0 { Button(model.text("Back", "上一步")) { model.draft.step -= 1 } }
                Spacer()
                Button(model.text("Finish later", "稍后继续")) {
                    if model.snapshot?.setup == nil && model.draft.validationError == nil { model.continueNames() }
                    model.suspendSetup()
                }
                if model.snapshot?.setup != nil { Button(model.text("Discard draft…", "放弃草稿…"), role: .destructive) { discard = true } }
            }
            Muted(model.text("Setup pauses this instance only. The other computer is not paused or activated remotely.", "设置仅暂停本机。另一台不会被远程暂停或启用。"))
        }
        }
        }
        .confirmationDialog(model.text("Discard this unfinished setup?", "放弃这份未完成的设置？"), isPresented: $discard) {
            Button(model.text("Discard draft", "放弃草稿"), role: .destructive) { model.cancelSetup() }
        } message: { Text(model.text("The previous saved workspace stays intact. Only this draft is discarded.", "原先保存的工作空间保持完整。只放弃当前草稿。")) }
    }
    @State private var discard = false
    private var stepTitle: String {
        switch model.draft.step {
        case 1: return model.text("Local devices", "本机设备")
        case 2: return model.text("Installation", "运行方案")
        case 3: return model.text("Review", "核对")
        default: return model.text("Names", "命名")
        }
    }
    private var namesStep: some View {
        VStack(alignment: .leading, spacing: 19) {
            Text(model.text("Give each computer a name.", "给每台电脑起个名字。")).font(.title2).fontWeight(.semibold)
            Muted(model.text("Names belong to the computer, independent of its operating system. Start on either host, including two Macs or two Windows PCs.", "名称属于电脑，与操作系统无关。可从任意一台开始，也支持两台 Mac 或两台 Windows。"))
            LabeledField(title: model.text("Profile name", "配置名称"), placeholder: model.text("Home or Work", "家里或公司"), value: $model.draft.name)
            HStack(alignment: .top, spacing: 20) {
                LabeledField(title: model.text("This computer · macOS", "本机 · macOS"), placeholder: model.text("Work laptop", "工作电脑"), value: $model.draft.localName)
                LabeledField(title: model.text("Other computer", "另一台电脑"), placeholder: model.text("Personal desktop", "个人电脑"), value: $model.draft.remoteName)
            }
            Picker(model.text("Other computer’s operating system", "另一台电脑的操作系统"), selection: $model.draft.remoteOS) { Text("macOS").tag("macos"); Text("Windows").tag("windows") }.pickerStyle(.segmented)
            Picker(model.text("This computer’s workspace role", "本机在此工作空间的角色"), selection: $model.draft.localRole) {
                Text(model.text("First computer (A)", "第一台电脑（A）")).tag("a")
                Text(model.text("Second computer (B)", "第二台电脑（B）")).tag("b")
            }
            Muted(model.text("When continuing an offline setup, choose the other role and use the same computer names. Roles do not imply a server or client.", "继续离线设置时，请选择另一角色并使用相同电脑名称。角色不代表服务端或客户端。"))
            ExpandableDetails(model.text("Continue from the other computer’s record", "使用另一台电脑的记录继续"), language: model.language) {
                VStack(alignment: .leading, spacing: 12) {
                    Muted(model.text("Paste the complete version 2 record. PortKey will adopt its workspace identity and opposite computer role, then ask you to enroll this computer’s devices.", "粘贴完整的版本 2 记录。PortKey 将采用其工作空间身份及相反的电脑角色，再引导绑定本机设备。"))
                    TextEditor(text: $model.draft.peerCode).font(.system(.caption, design: .monospaced)).frame(height: 85)
                    Button(model.text("Continue this offline workspace", "继续此离线工作空间")) { model.joinSetup() }.disabled(model.draft.peerCode.isEmpty)
                }.padding(.top, 10)
            }
            Button(model.text("Continue to local devices", "继续设置本机设备")) { model.continueNames() }.buttonStyle(.borderedProminent).disabled(model.draft.validationError != nil)
        }
    }
    private var bindingStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(model.text("Choose this workspace’s monitor.", "选择此工作空间的显示器。")).font(.title2).fontWeight(.semibold)
            Muted(model.text("Only this exact display can receive commands. If identical displays are unclear, reconnect the intended display by itself before choosing.", "只有明确绑定的这台显示器会收到指令。相同型号无法区分时，请仅连接目标显示器后再选择。"))
            Picker(model.text("Monitor", "显示器"), selection: $model.draft.displayID) {
                Text(model.text("Choose explicitly…", "请明确选择…")).tag("")
                ForEach(model.displays) { display in Text(display.label).tag(display.id) }
            }.disabled(!["names", "select", "draft"].contains(phase) && !(phase == "display" && model.snapshot?.setup?.display_id?.isEmpty == true))
            if ["names", "select", "draft"].contains(phase) || (phase == "display" && model.snapshot?.setup?.display_id?.isEmpty == true) {
                HStack { Button(model.text("Use this monitor", "使用此显示器")) { model.selectDisplay() }.buttonStyle(.borderedProminent).disabled(model.draft.displayID.isEmpty); Button(model.text("Refresh devices", "刷新设备")) { _ = model.action(["type": "refresh"]) } }
            } else {
                Divider()
                Text(model.text("Recognize the physical USB switch", "识别实体 USB 切换器")).font(.headline)
                Muted(usbInstructions)
                if ["display", "usb_retry"].contains(phase) {
                    Button(model.text(phase == "usb_retry" ? "USB is here — retry detection" : "USB is here — begin detection", phase == "usb_retry" ? "USB 已连接本机 — 重新检测" : "USB 已连接本机 — 开始检测")) { _ = model.action(["type": "setup_usb_start"]) }.buttonStyle(.borderedProminent)
                } else if usbReady {
                    Label(model.snapshot?.setup?.target_profile_id != nil ? model.text("Saved USB switch binding", "已保存的 USB 切换器绑定") : model.text("Switch learned from departure and return", "已通过离开和返回识别切换器"), systemImage: "checkmark.circle").foregroundStyle(.secondary)
                    Button(model.text("Continue to input mapping", "继续设置输入映射")) { model.draft.step = 2 }.buttonStyle(.borderedProminent)
                } else { HStack { ProgressView().controlSize(.small); Text(model.text("Observing device changes…", "正在观察设备变化…")).font(.callout) } }
            }
        }
    }
    private var usbInstructions: String {
        switch phase {
        case "usb_capturing": return model.text("Keep USB on this computer while the baseline is captured. Wait for the next instruction before pressing the switch.", "记录初始状态时请保持 USB 连接本机，等待下一步提示后再按切换器。")
        case "usb_wait_departure": return model.text("Press the physical USB selector to the other computer now. Keep this monitor input unchanged so you can read this instruction. Wait for the return cue; no click is needed on the computer that lost USB.", "现在按实体 USB 切换器切到另一台。保持显示器输入不变以阅读提示。等待切回提示；失去 USB 的电脑不需要点击。")
        case "usb_wait_return": return model.text("USB departure detected. Press the same selector to return USB here, then leave it connected while the branches settle.", "已检测到 USB 离开。现在用同一个切换器将 USB 切回本机，并保持连接等待分支稳定。")
        case "usb_retry": return model.text("Detection was incomplete or interrupted. Return USB to this computer, leave unrelated USB devices connected, and try again. A keyboard unplug alone is not a switch.", "检测未完成或被中断。请将 USB 切回本机，保持其他 USB 设备连接后重试。只拔下键盘不代表切换器。")
        default: return model.text("Start with USB on this computer and keep other devices connected. The guide observes the complete USB topology when the physical switch leaves and returns; it never picks hubs by their model alone.", "从 USB 连接本机开始，并保持其他设备连接。引导观察切换器离开和返回时的完整拓扑，不会仅按型号选择集线器。")
        }
    }
    private var installationStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(model.text("Match pictures to computer names.", "将画面与电脑名称对应。")).font(.title2).fontWeight(.semibold)
            Muted(model.text("Use the monitor’s physical input menu to show the named computer, then read the current input. These actions read only; they never scan or guess input numbers.", "使用显示器实体输入菜单显示指定电脑画面，再读取当前输入。以下操作只读取，不会遍历或猜测输入编号。"))
            DetailRow(title: model.draft.localName, subtitle: model.text("This computer", "本机"), value: model.snapshot?.setup?.local_input.map { model.text("Input observed · ", "已观察输入 · ") + String($0) } ?? model.text("Not observed", "尚未观察"))
            Button(model.text("\(model.draft.localName) is visible — read input", "已显示\(model.draft.localName) — 读取输入")) { _ = model.action(["type": "setup_read_local"]) }.disabled(phase == "reading_input")
            Divider()
            Picker(model.text("Where should PortKey run?", "在哪些电脑运行 PortKey？"), selection: $model.draft.mode) {
                Text(model.text("On one computer", "在一台电脑上")).tag("single")
                Text(model.text("On both computers", "在两台电脑上")).tag("dual")
            }.pickerStyle(.segmented).onChange(of: model.draft.mode) { _ in model.draft.clearAttestations(); _ = model.updateSetupWorkspace() }
            if model.draft.mode == "single" { singlePath } else { dualPath }
            if phase == "reading_input" { HStack { ProgressView().controlSize(.small); Text(model.text("Reading the selected display…", "正在读取所选显示器…")) } }
            if mappingReady { Button(model.text("Review this setup", "核对此设置")) { if model.updateSetupWorkspace() { model.draft.step = 3 } }.buttonStyle(.borderedProminent) }
        }
    }
    private var singlePath: some View {
        VStack(alignment: .leading, spacing: 15) {
            Picker(model.text("Designated controller", "指定控制电脑"), selection: $model.draft.singleController) {
                Text(model.draft.localName).tag("local"); Text(model.draft.remoteName).tag("remote")
            }.onChange(of: model.draft.singleController) { _ in model.draft.clearAttestations(); _ = model.updateSetupWorkspace() }
            Notice(title: model.text("One controller must be able to switch both ways", "单台控制电脑必须能够双向切换"), detail: model.text("It must stay awake and running, including when its own video source is inactive. An inconclusive result can be saved; it does not qualify automation. If the other computer is designated, finish its enrollment there.", "控制电脑必须保持唤醒并运行，且在自身视频输入未活动时仍能切换。可以保存不明确的结果，但不能据此启用自动切换。若指定另一台，请在那台电脑完成绑定。"))
            Button(model.text("\(model.draft.remoteName) is visible — read its input", "已显示\(model.draft.remoteName) — 读取其输入")) { if model.updateSetupWorkspace() { _ = model.action(["type": "setup_read_remote"]) } }.disabled(model.snapshot?.setup?.local_input == nil || phase == "reading_input")
            Muted(model.text("You can return using the monitor’s menu. If the inactive source cannot read the input, use both computers and exchange their observed choices instead.", "可通过显示器菜单切回。如非活动输入无法读取，请改用双机方案，交换各自观察到的输入。"))
        }
    }
    private var dualPath: some View {
        VStack(alignment: .leading, spacing: 15) {
            Muted(model.text("On the other computer, continue this workspace with the opposite role, the same names, and its own local device binding. No account or network is involved.", "在另一台使用相反角色、相同名称和那台自己的设备绑定继续此工作空间，无需账号或网络。"))
            if let code = model.snapshot?.setup?.code, !code.isEmpty {
                ExpandableDetails(model.text("This computer’s offline record", "本机的离线记录"), language: model.language) { Text(code).font(.system(.caption, design: .monospaced)).textSelection(.enabled).padding(.top, 8) }
                Button(model.text("Copy setup record", "复制设置记录")) { model.copyRecord() }
            }
            VStack(alignment: .leading, spacing: 7) { Text(model.text("Other computer’s setup record", "另一台电脑的设置记录")).font(.callout).fontWeight(.medium); TextEditor(text: $model.draft.peerCode).font(.system(.caption, design: .monospaced)).frame(height: 84) }
            Button(model.text("Review incoming record", "检查传入的记录")) { if model.updateSetupWorkspace() { model.previewPeer() } }.disabled(model.draft.peerCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            if let preview = model.snapshot?.setup?.peer_preview {
                VStack(alignment: .leading, spacing: 8) {
                    Text(model.text("Incoming computer: ", "传入的电脑：") + (preview.local_name ?? model.draft.remoteName)).font(.headline)
                    DetailRow(title: model.text("Workspace revision", "工作空间版本"), value: String(preview.revision ?? 0))
                    DetailRow(title: model.text("Other computer’s observed input", "另一台已观察到的输入"), value: preview.local_input.map(String.init) ?? "—")
                    DetailRow(title: model.text("This computer’s observed input", "本机已观察到的输入"), value: model.snapshot?.setup?.local_input.map(String.init) ?? "—")
                }
                Notice(title: model.text("Incoming record passed local consistency checks", "传入记录已通过本地一致性检查"), detail: model.text("Review the two computer names, workspace role and input choices above. Confirm only if both local enrollments belong to the same physical workspace. This does not show the peer online.", "请核对两台名称、工作空间角色及输入选项。仅在两台本机绑定属于同一实体环境时确认。这不代表另一台在线。"))
                Button(model.text("Confirm this is the same workspace", "确认这是同一工作空间")) { _ = model.action(["type": "setup_peer_confirm"]) }.buttonStyle(.borderedProminent)
            }
            if model.snapshot?.setup?.workspace?.peer_confirmed == true { Label(model.text("Reciprocal mapping saved · peer runtime unknown", "已保存双向映射 · 另一台运行状态未知"), systemImage: "doc.badge.checkmark").font(.callout) }
        }
    }
    private var reviewStep: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(model.text("Check that PortKey can switch the picture", "检查 PortKey 能否切换画面")).font(.title2).fontWeight(.semibold)
            Muted(model.text("These buttons send real input-switch commands. Input readings and manual changes in the monitor menu do not prove PortKey can switch. Confirm only pictures you actually saw, including earlier checks with these same connections.", "下面的测试会实际发送切换指令。读取输入、或用显示器菜单切换，都不能证明 PortKey 能切换。仅确认你实际看到的结果；相同连接下此前的成功测试也可以确认。"))
            if model.draft.mode == "single" {
                Toggle(model.text("PortKey on the other computer is quit, paused, or not installed", "另一台的 PortKey 已退出、暂停或未安装"), isOn: $model.draft.peerPaused).toggleStyle(.checkbox)
                Muted(model.text("Keep this Mac awake and USB connected here during testing. It must be able to switch from either picture.", "测试时保持 Mac 唤醒、USB 连接本机。单机控制必须能从另一台画面切回。"))
            }
            VStack(alignment: .leading, spacing: 9) {
                Text(model.text("1. Switch to ", "1. 切到 ") + model.setupRemoteLabel).font(.headline)
                Button(model.text("Test switching to ", "测试切到 ") + model.setupRemoteLabel) { model.startSetupTest("remote") }.disabled(!model.canTestSetup || (model.draft.mode == "single" && !model.draft.peerPaused))
                Muted(model.text("Watch the picture. Then use the monitor's input menu to return to this Mac and record the result. Do not move USB.", "观察画面是否切换。然后用显示器输入菜单返回 Mac，再确认结果；不需要切换 USB。"))
                Toggle(model.text("I saw PortKey switch the picture to ", "我看到 PortKey 将画面切到了 ") + model.setupRemoteLabel, isOn: $model.draft.remoteObserved).toggleStyle(.checkbox)
            }
            Divider()
            VStack(alignment: .leading, spacing: 9) {
                Text(model.text("2. Switch back to ", "2. 切回 ") + model.setupLocalLabel).font(.headline)
                if let seconds = model.setupTestCountdown {
                    Text(model.text("Switching back in ", "将在 ") + String(seconds) + model.text(" seconds. Show the other computer using the monitor menu now.", " 秒后尝试切回。现在请用显示器菜单显示另一台画面。"))
                    Button(model.text("Cancel test", "取消测试")) { model.cancelSetupTest() }
                } else {
                    Button(model.text("Test switching back in 10 seconds", "测试 10 秒后切回本机")) { model.startSetupTest("local") }.disabled(!model.canTestSetup || (model.draft.mode == "single" && !model.draft.peerPaused))
                }
                Muted(model.text("After starting, use the monitor menu to show the other computer within 10 seconds. PortKey will then try to bring this Mac back. If it does not, return with the monitor menu; leave this result unconfirmed.", "点击后，在 10 秒内用显示器菜单显示另一台电脑。PortKey 随后尝试切回 Mac。若未切回，用显示器菜单返回，并保留此项未确认。Mac 画面原本就在显示时的同输入请求不算通过。"))
                Toggle(model.text("I saw PortKey bring this Mac back from the other computer's picture", "我看到 PortKey 从另一台画面切回了 Mac"), isOn: $model.draft.localObserved).toggleStyle(.checkbox)
            }
            if let result = model.snapshot?.display_state?.result, !["unconfirmed", "idle"].contains(result) {
                Muted(result == "pending" ? model.text("Sending the test command…", "正在发送测试指令…") : model.inputSubtitle)
            }
            if let reason = model.setupEnableBlocker {
                Notice(title: model.text("Not enabled yet", "尚未启用"), detail: reason)
            } else {
                Label(model.text("Ready to enable on this Mac", "可以在本机启用"), systemImage: "checkmark.circle").foregroundStyle(productAccent)
            }
            HStack(spacing: 12) {
                Button(model.text("Enable automatic switching", "启用自动切换")) { model.finishSetup(enable: true) }.buttonStyle(.borderedProminent).disabled(model.setupEnableBlocker != nil)
                Button(model.text("Save without enabling", "仅保存，暂不启用")) { model.finishSetup(enable: false) }.disabled(!mappingReady)
            }
            Muted(model.text("Enabling saves your confirmations and this profile together. In two-controller mode, enable the other computer separately.", "启用会一起保存这些确认和当前配置，无需另存观察记录。双机控制需在另一台分别启用。"))
            ExpandableDetails(model.text("Saved connections and notes", "已保存的连接与备注"), language: model.language) {
                VStack(alignment: .leading, spacing: 10) {
                    DetailRow(title: model.setupLocalLabel, value: model.snapshot?.setup?.local_input.map { model.text("Saved input ", "已保存输入 ") + String($0) } ?? model.text("Incomplete", "未完成"))
                    DetailRow(title: model.setupRemoteLabel, value: model.snapshot?.setup?.remote_input.map { model.text("Saved input ", "已保存输入 ") + String($0) } ?? model.text("Incomplete", "未完成"))
                    LabeledField(title: model.text("Note (optional)", "备注（可选）"), placeholder: model.text("What you observed", "实际观察结果"), value: $model.draft.evidenceNote)
                }.padding(.top, 8)
            }
        }
    }
}

struct LabeledField: View {
    let title: String
    let placeholder: String
    @Binding var value: String
    var body: some View { VStack(alignment: .leading, spacing: 7) { Text(title).font(.callout).fontWeight(.medium); TextField("", text: $value, prompt: Text(placeholder)).labelsHidden().textFieldStyle(.roundedBorder).frame(maxWidth: .infinity).accessibilityLabel(title) }.frame(maxWidth: .infinity, alignment: .leading) }
}
struct MetadataEditor: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        VStack(spacing: 0) {
            Text(model.text("Profile details", "配置详情")).font(.title2).fontWeight(.semibold).frame(maxWidth: .infinity, alignment: .leading).padding(24)
            Form {
                Section(model.text("Names", "名称")) {
                    TextField(model.text("Profile name", "配置名称"), text: $model.metadataDraft.name)
                    TextField(model.text("This computer", "本机"), text: $model.metadataDraft.localName)
                    TextField(model.text("Other computer", "另一台电脑"), text: $model.metadataDraft.remoteName)
                    Picker(model.text("Other operating system", "另一台的操作系统"), selection: $model.metadataDraft.remoteOS) { Text(model.text("Not recorded", "未记录")).tag("unknown"); Text("macOS").tag("macos"); Text("Windows").tag("windows") }
                }
                Section(model.text("Installation", "运行方案")) {
                    Picker(model.text("Run PortKey on", "运行 PortKey 的电脑"), selection: $model.metadataDraft.mode) { Text(model.text("Not checked", "尚未检查")).tag("unverified"); Text(model.text("One computer", "一台电脑")).tag("single"); Text(model.text("Both computers", "两台电脑")).tag("dual") }
                    if model.metadataDraft.mode == "single" { Picker(model.text("Controller", "控制电脑"), selection: $model.metadataDraft.singleController) { Text(model.metadataDraft.localName).tag("local"); Text(model.metadataDraft.remoteName).tag("remote") } }
                }
                Section {
                    Muted(model.text("Renaming keeps device bindings and evidence. Changing the installation plan can pause local automation until that arrangement is qualified. Changes are saved on this computer only.", "重命名会保留设备绑定和证据。更改运行方案可能会暂停本机自动切换，直到新方案完成验证。更改仅保存在本机。"))
                }
            }.formStyle(.grouped)
            Divider()
            HStack { Button(model.text("Cancel", "取消")) { model.editingMetadata = false }.keyboardShortcut(.cancelAction); Spacer(); Button(model.text("Save details", "保存详情")) { model.saveProfileDetails() }.keyboardShortcut(.defaultAction).buttonStyle(.borderedProminent).disabled(model.metadataDraft.validationError != nil) }.padding(20)
        }.frame(width: 590, height: 570)
    }
}

struct DiagnosticsView: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        SectionHeading(title: model.text("Diagnostics", "诊断"), subtitle: model.text("Local evidence, with its limits visible.", "本机证据，清楚展示其边界。"))
        Surface {
            VStack(alignment: .leading, spacing: 14) {
                DetailRow(title: model.text("Controller", "控制核心"), value: "Rust · SwiftUI / AppKit")
                DetailRow(title: model.text("Local state", "本机状态"), value: model.effectiveTitle)
                DetailRow(title: model.text("USB observation", "USB 观察"), value: model.usbTitle)
                DetailRow(title: model.text("Current input evidence", "当前输入证据"), value: model.inputEvidenceTitle)
                DetailRow(title: model.text("Other instance", "另一台程序"), value: model.text("Runtime unknown · no network coordination", "运行状态未知 · 无网络协调"))
                Divider()
                Text(model.text("Latest controller detail", "最近的核心详情")).font(.headline)
                Text(model.bridgeFailure ?? (model.snapshot?.status.isEmpty == false ? model.snapshot!.status : model.text("No additional detail.", "暂无更多详情。"))).font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                Button(model.text("Refresh device observations", "刷新设备观察")) { _ = model.action(["type": "refresh"]) }.disabled(!model.canConfigure)
            }
        }
        Surface {
            VStack(alignment: .leading, spacing: 15) {
                Text(model.text("Diagnostic package preview", "诊断数据包预览")).font(.headline)
                Muted(model.text("Explicitly collect a sanitized current summary and bounded recorded transitions, timings and outcomes. Missing files and collection limits remain visible. No names, device identities, paths or user input are included; nothing is uploaded.", "明确收集已脱敏的当前摘要，以及有限范围内记录的状态变化、计时和结果。缺失文件及收集范围会明确保留。不含名称、设备身份、路径或用户输入，不会上传。"))
                HStack {
                    Button(model.diagnosticsPackageJSON == nil ? model.text("Collect diagnostic package", "收集诊断数据包") : model.text("Refresh collected package", "重新收集数据包")) { model.collectDiagnostics() }.disabled(!model.canCollectDiagnostics)
                    if model.diagnosticsPackageJSON != nil { Label(model.text("Collected preview · refresh for later events", "已收集的预览 · 新事件需重新收集"), systemImage: "doc.text").font(.caption).foregroundStyle(.secondary) }
                }
                Notice(title: model.diagnosticsPackageJSON == nil ? model.text("Package unavailable · current-state fallback only", "数据包尚不可用 · 仅显示当前状态备用摘要") : model.text("Collection limits and history availability", "收集范围与历史可用性"), detail: model.diagnosticsHistorySummary)
                if let error = model.diagnosticsCollectionError { Text(error).font(.callout).foregroundStyle(.red).textSelection(.enabled) }
                Text(model.diagnosticsPreview).font(.system(.caption, design: .monospaced)).textSelection(.enabled).padding(15).frame(maxWidth: .infinity, alignment: .leading).background(canvas, in: RoundedRectangle(cornerRadius: 8))
                HStack { Button(model.text("Export this preview…", "导出此预览…")) { model.exportDiagnostics() }.disabled(model.diagnosticsPackageJSON == nil); if let result = model.exportResult { Text(result).font(.caption).foregroundStyle(.secondary) } }
            }
        }
        Muted(model.text("Timing files rotate at 1 MiB with five backups. Older records can be lost; exports do not reconstruct missing events. Command transport, input readback and user-observed pictures are separate evidence.", "计时文件达到 1 MiB 后轮换，保留五份备份。更早记录可能丢失，导出不会重建缺失事件。指令传输、输入回报与用户观察画面是不同证据。"))
    }
}
struct PreferencesView: View {
    @ObservedObject var model: PortKeyStore
    var body: some View {
        SectionHeading(title: model.text("Settings", "设置"), subtitle: model.text("A small utility, at home in your menu bar.", "安静地留在菜单栏，随时可用。"))
        Surface {
            VStack(alignment: .leading, spacing: 17) {
                Picker(model.text("Language", "语言"), selection: Binding(get: { model.language }, set: model.setLanguage)) { Text("English").tag("en"); Text("简体中文").tag("zh-Hans") }
                Divider()
                DetailRow(title: model.text("Startup", "启动"), value: model.text("Launch manually · no login item installed", "手动启动 · 未安装登录项"))
                DetailRow(title: model.text("Window behavior", "窗口行为"), value: model.text("Closing keeps the menu bar controller running", "关闭窗口后菜单栏控制核心继续运行"))
                DetailRow(title: model.text("Pause scope", "暂停范围"), value: model.text("This computer only", "仅本机"))
            }
        }
        Muted(model.text("PortKey switches the monitor’s video input. It does not move USB, wake another computer, or know whether the other PortKey instance is running.", "PortKey 切换显示器的视频输入，不会转移 USB、唤醒另一台电脑或获知另一台 PortKey 是否运行。"))
    }
}
