#include "pch.h"
#include "ControllerView.h"
#include "../portkey.h"
#include <microsoft.ui.xaml.window.h>
#include <commctrl.h>
#include <dbt.h>
#include <fstream>
#include <commdlg.h>
#include <dwmapi.h>
#include <filesystem>
#include <sstream>
using namespace winrt;
using namespace Windows::Data::Json;
using namespace Microsoft::UI::Xaml;
using namespace Microsoft::UI::Xaml::Controls;
namespace {
JsonObject obj(JsonObject const &parent, wchar_t const *key) {
    auto value = parent.TryLookup(key);
    return value && value.ValueType() == JsonValueType::Object ? value.GetObject() : JsonObject{};
}
hstring str(JsonObject const &parent, wchar_t const *key, hstring const &fallback = L"") {
    auto value = parent.TryLookup(key);
    return value && value.ValueType() == JsonValueType::String ? value.GetString() : fallback;
}
bool flag(JsonObject const &parent, wchar_t const *key) {
    auto value = parent.TryLookup(key);
    return value && value.ValueType() == JsonValueType::Boolean && value.GetBoolean();
}
bool hasObject(JsonObject const &parent, wchar_t const *key) {
    auto value = parent.TryLookup(key);
    return value && value.ValueType() == JsonValueType::Object;
}
JsonObject command(wchar_t const *type) {
    JsonObject value;
    value.SetNamedValue(L"type", JsonValue::CreateStringValue(type));
    return value;
}
void put(JsonObject const &object, wchar_t const *key, hstring const &value) {
    object.SetNamedValue(key, JsonValue::CreateStringValue(value));
}
void put(JsonObject const &object, wchar_t const *key, bool value) {
    object.SetNamedValue(key, JsonValue::CreateBooleanValue(value));
}
JsonObject metadata(JsonObject const &source) {
    JsonObject result;
    for (auto key : {L"local_name", L"remote_name", L"local_os", L"remote_os", L"local_role",
                     L"workspace_id", L"revision", L"mode", L"single_controller", L"verification",
                     L"evidence", L"peer_confirmed", L"peer_paused_confirmed"})
        if (source.HasKey(key))
            result.SetNamedValue(key, source.Lookup(key));
    return result;
}
hstring verificationLabel(hstring const &value, bool zh) {
    for (auto entry :
         {std::tuple{L"not_checked", L"Capability not checked", L"尚未检查能力"},
          std::tuple{L"historical_unknown",
                     L"Existing enrollment · historical evidence provenance unknown",
                     L"现有配置 · 历史证据来源未知"},
          std::tuple{L"single_observed", L"Single controller · both directions observed",
                     L"单机控制 · 已观察两个方向"},
          std::tuple{L"single_inconclusive", L"Single controller · inconclusive",
                     L"单机控制 · 尚无定论"},
          std::tuple{L"dual_observed", L"Two controllers · both directions observed",
                     L"双机控制 · 已观察两个方向"},
          std::tuple{L"needs_attention", L"Capability needs attention", L"能力需要重新检查"}})
        if (value == std::get<0>(entry))
            return zh ? std::get<2>(entry) : std::get<1>(entry);
    return zh ? L"尚未检查能力" : L"Capability not checked";
}
constexpr UINT TrayMessage = WM_APP + 43;
constexpr GUID UsbDevice{
    0xa5dcbf10, 0x6530, 0x11d2, {0x90, 0x1f, 0x00, 0xc0, 0x4f, 0xb9, 0x51, 0xed}};
constexpr GUID UsbHub{0xf18a0e88, 0xc30c, 0x11d0, {0x88, 0x15, 0x00, 0xa0, 0xc9, 0x06, 0xbe, 0xd8}};
std::filesystem::path executableDirectory() {
    wchar_t name[32768];
    auto count = GetModuleFileNameW(nullptr, name, 32768);
    if (!count || count == 32768)
        throw std::runtime_error("Cannot locate application resources");
    return std::filesystem::path(name).parent_path();
}
std::wstring environment(wchar_t const *key) {
    wchar_t value[32768];
    auto n = GetEnvironmentVariableW(key, value, 32768);
    return n && n < 32768 ? std::wstring(value, n) : L"";
}
} // namespace
JsonObject ControllerView::response(char *value) {
    if (!value)
        throw std::runtime_error("The PortKey controller returned no response");
    std::string owned(value);
    pk_string_free(value);
    return JsonObject::Parse(to_hstring(owned));
}
JsonObject ControllerView::send(JsonObject const &value, bool showError) {
    auto bytes = to_string(value.Stringify());
    auto result =
        actionOverride ? actionOverride(value) : response(pk_action(controller, bytes.c_str()));
    if (showError && !flag(result, L"ok")) {
        auto bar = control<InfoBar>(L"ActionError");
        bar.Message(str(result, L"error"));
        bar.IsOpen(true);
    } else if (showError)
        control<InfoBar>(L"ActionError").IsOpen(false);
    return result;
}
bool ControllerView::action(wchar_t const *type) {
    return flag(send(command(type)), L"ok");
}
void ControllerView::text(wchar_t const *name, hstring const &value) {
    auto block = control<TextBlock>(name);
    if (block.Text() != value)
        block.Text(value);
}
void ControllerView::label(wchar_t const *name, wchar_t const *en, wchar_t const *cn) {
    auto element = root.FindName(name);
    auto value = box_value(zh ? cn : en);
    if (auto block = element.try_as<TextBlock>())
        block.Text(zh ? cn : en);
    else if (auto input = element.try_as<TextBox>())
        input.Header(value);
    else if (auto combo = element.try_as<ComboBox>())
        combo.Header(value);
    else if (auto toggle = element.try_as<ToggleSwitch>())
        toggle.Header(value);
    else if (auto expander = element.try_as<Expander>())
        expander.Header(value);
    else if (auto content = element.try_as<ContentControl>())
        content.Content(value);
}
ControllerView::ControllerView() {
    portkeySmokeTrace("view_begin");
    blockedSmoke =
        std::wstring(GetCommandLineW()).find(L"--blocked-smoke-test") != std::wstring::npos;
    restoreSmoke = std::wstring(GetCommandLineW()).find(L"--restore-editor") != std::wstring::npos;
    smokeTicks = blockedSmoke ? 4
                 : std::wstring(GetCommandLineW()).find(L"--smoke-test") != std::wstring::npos
                     ? (restoreSmoke ? 8 : 28)
                     : 0;
    if (smokeTicks && (environment(L"PORTKEY_CONFIG_DIR").empty() ||
                       environment(L"PORTKEY_SMOKE_RESULT").empty()))
        throw std::runtime_error(
            "Smoke test requires explicit isolated configuration and result paths");
    std::ifstream file(executableDirectory() / L"MainWindow.xaml", std::ios::binary);
    if (!file)
        throw std::runtime_error("MainWindow.xaml is missing from the application package");
    std::string xaml((std::istreambuf_iterator<char>(file)), {});
    portkeySmokeTrace("load_xaml");
    root = Markup::XamlReader::Load(to_hstring(xaml)).as<FrameworkElement>();
    portkeySmokeTrace("xaml_loaded");
    window = Window();
    window.Title(L"PortKey");
    window.Content(root);
    try {
        window.SystemBackdrop(Microsoft::UI::Xaml::Media::MicaBackdrop());
    } catch (hresult_error const &) {
    }
    check_hresult(window.as<IWindowNative>()->get_WindowHandle(&hwnd));
    window.AppWindow().Resize({1080, 850});
    auto nativeTheme = [this] {
        BOOL dark = root.ActualTheme() == ElementTheme::Dark;
        DwmSetWindowAttribute(hwnd, 20, &dark, sizeof(dark));
    };
    root.ActualThemeChanged([nativeTheme](auto const &, auto const &) { nativeTheme(); });
    nativeTheme();
    portkeySmokeTrace("create_controller");
    controller = pk_create();
    if (!controller)
        throw std::runtime_error("Cannot create PortKey controller");
    portkeySmokeTrace("controller_created");
    snapshot = response(pk_snapshot(controller));
    startupBlocked = !str(snapshot, L"startup_error").empty();
    if (smokeTicks && hasObject(snapshot, L"config")) {
        pk_destroy(controller);
        controller = nullptr;
        throw std::runtime_error("Smoke test refused an enrolled configuration");
    }
    SetWindowSubclass(hwnd, windowProc, 1, reinterpret_cast<DWORD_PTR>(this));
    taskbarCreated = RegisterWindowMessageW(L"TaskbarCreated");
    tray.cbSize = sizeof(tray);
    tray.hWnd = hwnd;
    tray.uID = 1;
    tray.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    tray.uCallbackMessage = TrayMessage;
    auto instance = GetModuleHandleW(nullptr);
    auto smallIcon = static_cast<HICON>(LoadImageW(instance, MAKEINTRESOURCEW(101), IMAGE_ICON,
                                                   GetSystemMetrics(SM_CXSMICON),
                                                   GetSystemMetrics(SM_CYSMICON), LR_SHARED));
    auto largeIcon = static_cast<HICON>(LoadImageW(instance, MAKEINTRESOURCEW(101), IMAGE_ICON,
                                                   GetSystemMetrics(SM_CXICON),
                                                   GetSystemMetrics(SM_CYICON), LR_SHARED));
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL, reinterpret_cast<LPARAM>(smallIcon));
    SendMessageW(hwnd, WM_SETICON, ICON_BIG, reinterpret_cast<LPARAM>(largeIcon));
    if (!smallIcon || !largeIcon)
        throw std::runtime_error("Cannot load the PortKey notification-area icon");
    tray.hIcon = smallIcon;
    wcscpy_s(tray.szTip, L"PortKey");
    trayReady = Shell_NotifyIconW(NIM_ADD, &tray) != 0;
    if (!trayReady)
        throw std::runtime_error("Cannot add PortKey to the notification area");
    tray.uVersion = NOTIFYICON_VERSION_4;
    Shell_NotifyIconW(NIM_SETVERSION, &tray);
    portkeySmokeTrace("register_notifications");
    if (!startupBlocked)
        registerNotifications();
    portkeySmokeTrace("bind_controls");
    bind();
    if (!startupBlocked)
        restoreEditor();
    timer = window.DispatcherQueue().CreateTimer();
    timer.Interval(std::chrono::milliseconds(200));
    timer.Tick([this](auto const &, auto const &) {
        try {
            tick();
        } catch (hresult_error const &e) {
            portkeySmokeTrace(to_string(e.message()).c_str());
            auto bar = control<InfoBar>(L"ActionError");
            bar.Message(e.message());
            bar.IsOpen(true);
        }
    });
    portkeySmokeTrace("initial_tick");
    tick();
    portkeySmokeTrace("start_timer");
    timer.Start();
    show();
}
ControllerView::~ControllerView() {
    if (timer)
        timer.Stop();
    for (auto notification : notifications)
        UnregisterDeviceNotification(notification);
    if (hwnd) {
        Shell_NotifyIconW(NIM_DELETE, &tray);
        RemoveWindowSubclass(hwnd, windowProc, 1);
    }
    if (controller)
        pk_destroy(controller);
}
void ControllerView::show() {
    window.Activate();
    ShowWindow(hwnd, SW_RESTORE);
    SetForegroundWindow(hwnd);
}
void ControllerView::quit() {
    if (quitting)
        return;
    quitting = true;
    timer.Stop();
    persistEditor();
    action(L"quit");
    Shell_NotifyIconW(NIM_DELETE, &tray);
    window.Close();
    Application::Current().Exit();
}
void ControllerView::registerNotifications() {
    for (auto id : {UsbDevice, UsbHub}) {
        DEV_BROADCAST_DEVICEINTERFACE_W filter{};
        filter.dbcc_size = sizeof(filter);
        filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;
        filter.dbcc_classguid = id;
        if (auto handle = RegisterDeviceNotificationW(hwnd, &filter, DEVICE_NOTIFY_WINDOW_HANDLE))
            notifications.push_back(handle);
    }
    auto value = command(L"usb_notifications");
    put(value, L"ready", notifications.size() == 2);
    send(value, false);
}
LRESULT CALLBACK ControllerView::windowProc(HWND handle, UINT message, WPARAM w, LPARAM l, UINT_PTR,
                                            DWORD_PTR context) {
    auto self = reinterpret_cast<ControllerView *>(context);
    if (message == self->taskbarCreated) {
        self->trayReady = Shell_NotifyIconW(NIM_ADD, &self->tray) != 0;
        self->tray.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &self->tray);
        return 0;
    }
    if (message == WM_COMMAND && LOWORD(w) == 104 && l == 0) {
        self->quit();
        return 0;
    }
    if (message == WM_CLOSE && !self->quitting) {
        self->persistEditor();
        ShowWindow(handle, SW_HIDE);
        return 0;
    }
    if (message == TrayMessage) {
        auto event = LOWORD(l);
        if (event == NIN_SELECT || event == NIN_KEYSELECT || event == WM_RBUTTONUP ||
            event == WM_CONTEXTMENU || event == WM_LBUTTONUP)
            self->trayMenu();
        if (event == WM_LBUTTONDBLCLK)
            self->show();
        return 0;
    }
    if (message == WM_DISPLAYCHANGE || (message == WM_DEVICECHANGE && w == DBT_DEVNODES_CHANGED))
        self->send(command(L"refresh"), false);
    if (message == WM_DEVICECHANGE && (w == DBT_DEVICEARRIVAL || w == DBT_DEVICEREMOVECOMPLETE) &&
        l) {
        auto header = reinterpret_cast<DEV_BROADCAST_HDR *>(l);
        if (header->dbch_devicetype == DBT_DEVTYP_DEVICEINTERFACE &&
            header->dbch_size >= sizeof(DEV_BROADCAST_DEVICEINTERFACE_W)) {
            auto payload = reinterpret_cast<DEV_BROADCAST_DEVICEINTERFACE_W *>(l);
            if (IsEqualGUID(payload->dbcc_classguid, UsbDevice) ||
                IsEqualGUID(payload->dbcc_classguid, UsbHub))
                self->send(command(L"usb_refresh"), false);
        }
    }
    return DefSubclassProc(handle, message, w, l);
}
void ControllerView::trayMenu() {
    auto menu = CreatePopupMenu();
    auto workspace = obj(snapshot, L"workspace");
    auto local = str(workspace, L"local_name", zh ? L"本机" : L"This computer"),
         remote = str(workspace, L"remote_name", zh ? L"另一台电脑" : L"Other computer");
    AppendMenuW(menu, MF_STRING | MF_DISABLED, 0,
                str(snapshot, L"profile_name", L"PortKey").c_str());
    AppendMenuW(menu, MF_STRING | MF_DISABLED, 0, str(snapshot, L"status").c_str());
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    bool available = !startupBlocked && hasObject(snapshot, L"config") &&
                     flag(snapshot, L"lifecycle_ok") && !flag(snapshot, L"editing") &&
                     !flag(snapshot, L"sleeping") && !str(snapshot, L"active_display_id").empty();
    AppendMenuW(menu, MF_STRING | (available ? 0 : MF_GRAYED), 1,
                ((zh ? L"切换显示器到 " : L"Switch display to ") + std::wstring(local)).c_str());
    AppendMenuW(menu, MF_STRING | (available ? 0 : MF_GRAYED), 2,
                ((zh ? L"切换显示器到 " : L"Switch display to ") + std::wstring(remote)).c_str());
    bool automatic = flag(obj(snapshot, L"config"), L"automatic");
    AppendMenuW(menu,
                MF_STRING | (automatic ? MF_CHECKED : 0) |
                    (hasObject(snapshot, L"config") && !startupBlocked ? 0 : MF_GRAYED),
                3, (zh ? L"本机自动切换" : L"Automatic on this computer"));
    AppendMenuW(menu, MF_SEPARATOR, 0, nullptr);
    AppendMenuW(menu, MF_STRING, 4, zh ? L"打开 PortKey" : L"Open PortKey");
    AppendMenuW(menu, MF_STRING, 5, zh ? L"退出 PortKey" : L"Quit PortKey");
    POINT cursor{};
    GetCursorPos(&cursor);
    SetForegroundWindow(hwnd);
    auto result =
        TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, cursor.x, cursor.y, 0, hwnd, nullptr);
    DestroyMenu(menu);
    if (result == 1 || result == 2) {
        auto value = command(L"manual");
        put(value, L"target", hstring(result == 1 ? L"local" : L"remote"));
        send(value);
    }
    if (result == 3) {
        auto value = command(L"automatic");
        put(value, L"enabled", !automatic);
        send(value);
    }
    if (result == 4)
        show();
    if (result == 5)
        quit();
}
void ControllerView::page(wchar_t const *name) {
    auto navigation = control<NavigationView>(L"Navigation");
    auto item = wcscmp(name, L"SettingsPage") == 0
                    ? navigation.SettingsItem()
                    : root.FindName(wcscmp(name, L"ProfilesPage") == 0      ? L"NavProfiles"
                                    : wcscmp(name, L"SetupPage") == 0       ? L"NavSetup"
                                    : wcscmp(name, L"DiagnosticsPage") == 0 ? L"NavDiagnostics"
                                                                            : L"NavOverview");
    if (navigation.SelectedItem() != item)
        navigation.SelectedItem(item);
    for (auto candidate :
         {L"OverviewPage", L"ProfilesPage", L"SetupPage", L"DiagnosticsPage", L"SettingsPage"})
        control<StackPanel>(candidate).Visibility(
            wcscmp(candidate, name) == 0 ? Visibility::Visible : Visibility::Collapsed);
}
void ControllerView::bind() {
    auto click = [this](wchar_t const *name, std::function<void()> fn) {
        control<Button>(name).Click([fn](auto const &, auto const &) { fn(); });
    };
    control<NavigationView>(L"Navigation")
        .SelectionChanged(
            [this](auto const &, NavigationViewSelectionChangedEventArgs const &args) {
                if (args.IsSettingsSelected()) {
                    page(L"SettingsPage");
                    return;
                }
                if (auto item = args.SelectedItem().try_as<NavigationViewItem>()) {
                    auto name = item.Name();
                    page(name == L"NavProfiles"      ? L"ProfilesPage"
                         : name == L"NavSetup"       ? L"SetupPage"
                         : name == L"NavDiagnostics" ? L"DiagnosticsPage"
                                                     : L"OverviewPage");
                    if (name == L"NavSetup")
                        beginSetup();
                }
            });
    control<NavigationView>(L"Navigation").SelectedItem(root.FindName(L"NavOverview"));
    click(L"Quit", [this] { quit(); });
    click(L"BlockedDiagnostics", [this] { page(L"DiagnosticsPage"); });
    click(L"OverviewSetup", [this] { beginSetup(); });
    click(L"ProfilesNew", [this] { beginNewProfile(); });
    click(L"ResumeDraft", [this] { beginSetup(); });
    click(L"Refresh", [this] { action(L"refresh"); });
    click(L"SwitchLocal", [this] {
        auto value = command(L"manual");
        put(value, L"target", hstring(L"local"));
        send(value);
    });
    click(L"SwitchRemote", [this] {
        auto value = command(L"manual");
        put(value, L"target", hstring(L"remote"));
        send(value);
    });
    control<ToggleSwitch>(L"Automatic").Toggled([this](auto const &, auto const &) {
        if (updating)
            return;
        auto value = command(L"automatic");
        put(value, L"enabled", control<ToggleSwitch>(L"Automatic").IsOn());
        send(value);
    });
    control<ComboBox>(L"Language").SelectionChanged([this](auto const &, auto const &) {
        if (updating)
            return;
        auto value = command(L"language");
        put(value, L"value",
            hstring(control<ComboBox>(L"Language").SelectedIndex() == 1 ? L"zh-Hans" : L"en"));
        send(value);
    });
    for (auto name : {L"Mode", L"LocalRole", L"RemoteOs", L"SingleController"})
        control<ComboBox>(name).SelectionChanged([this](auto const &, auto const &) {
            if (!updating)
                clearAttestations();
        });
    control<ComboBox>(L"ProfilesList").SelectionChanged([this](auto const &, auto const &) {
        if (!updating)
            profileSelected();
    });
    click(L"SaveNames", [this] { saveViewedProfile(); });
    click(L"UseProfile", [this] { useViewedProfile(); });
    click(L"RestoreAuto", [this] { restoreAutomaticProfile(); });
    click(L"OverviewRestoreAuto", [this] { restoreAutomaticProfile(); });
    click(L"OverviewProfiles", [this] { page(L"ProfilesPage"); });
    click(L"SaveDraftNames", [this] { setupNames(); });
    click(L"JoinWorkspace", [this] {
        auto value = command(L"setup_join");
        put(value, L"record", control<TextBox>(L"JoinRecord").Text());
        if (flag(send(value), L"ok")) {
            editorPending = false;
            clearAttestations();
            draftLoaded = false;
            setupStep = 1;
        }
    });
    click(L"SetupBack", [this] {
        if (setupStep > 0)
            --setupStep;
    });
    click(L"SetupNext", [this] {
        if (setupStep == 0) {
            setupNames();
            return;
        }
        if (setupStep == 3 && !flag(obj(snapshot, L"setup"), L"ready"))
            return;
        if (setupStep < 5)
            ++setupStep;
    });
    click(L"SelectDisplay", [this] {
        auto i = control<ComboBox>(L"DisplaysList").SelectedIndex();
        if (i < 0 || size_t(i) >= displays.size())
            return;
        if (!hasObject(snapshot, L"setup"))
            setupNames();
        auto value = command(L"setup_select_display");
        put(value, L"display_id", str(displays[i], L"id"));
        if (flag(send(value), L"ok")) {
            clearAttestations();
            setupStep = 2;
        }
    });
    click(L"DetectUsb", [this] { setupAction(L"setup_usb_start"); });
    click(L"ReadLocal", [this] { setupAction(L"setup_read_local"); });
    click(L"ReadRemote", [this] { setupAction(L"setup_read_remote"); });
    click(L"CopyRecord", [this] {
        auto result = send(command(L"setup_export"));
        if (flag(result, L"ok")) {
            Windows::ApplicationModel::DataTransfer::DataPackage data;
            data.SetText(str(result, L"record"));
            Windows::ApplicationModel::DataTransfer::Clipboard::SetContent(data);
        }
    });
    click(L"PreviewRecord", [this] {
        auto value = command(L"setup_peer_preview");
        put(value, L"record", control<TextBox>(L"PeerRecord").Text());
        send(value);
    });
    click(L"ConfirmRecord", [this] {
        if (control<CheckBox>(L"PeerBindingConfirmed").IsChecked().Value() &&
            action(L"setup_peer_confirm"))
            clearAttestations(false);
    });
    click(L"RecordEvidence", [this] { qualify(); });
    click(L"FinishOff", [this] { finishSetup(false); });
    click(L"FinishEnable", [this] { finishSetup(true); });
    click(L"FinishLater", [this] { finishLater(); });
    click(L"CancelDraft", [this] {
        if (action(L"setup_cancel")) {
            resetEditor();
            page(L"OverviewPage");
        }
    });
    click(L"CollectDiagnostics", [this] {
        auto result = send(command(L"diagnostics_preview"));
        if (flag(result, L"ok"))
            control<TextBox>(L"DiagnosticsPreview").Text(obj(result, L"diagnostics").Stringify());
    });
    click(L"CopyDiagnostics", [this] {
        Windows::ApplicationModel::DataTransfer::DataPackage data;
        data.SetText(control<TextBox>(L"DiagnosticsPreview").Text());
        Windows::ApplicationModel::DataTransfer::Clipboard::SetContent(data);
    });
    click(L"ExportDiagnostics", [this] {
        wchar_t path[32768] = L"PortKey-diagnostics.json";
        OPENFILENAMEW dialog{};
        dialog.lStructSize = sizeof(dialog);
        dialog.hwndOwner = hwnd;
        dialog.lpstrFilter = L"JSON (*.json)\0*.json\0\0";
        dialog.lpstrFile = path;
        dialog.nMaxFile = 32768;
        dialog.lpstrDefExt = L"json";
        dialog.Flags = OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR;
        if (GetSaveFileNameW(&dialog)) {
            std::ofstream file(std::filesystem::path(path), std::ios::binary);
            file << to_string(control<TextBox>(L"DiagnosticsPreview").Text());
            if (!file) {
                auto bar = control<InfoBar>(L"ActionError");
                bar.Message(zh ? L"无法保存诊断文件。" : L"Cannot save the diagnostic file.");
                bar.IsOpen(true);
            }
        }
    });
    translate();
    control<TextBox>(L"DraftLocalName").Text(L"This computer");
    control<TextBox>(L"DraftRemoteName").Text(L"Other computer");
}
JsonObject ControllerView::workspaceFromControls(JsonObject const &source, bool saved) {
    auto workspace = metadata(source);
    put(workspace, L"local_name",
        control<TextBox>(saved ? L"SavedLocalName" : L"DraftLocalName").Text());
    put(workspace, L"remote_name",
        control<TextBox>(saved ? L"SavedRemoteName" : L"DraftRemoteName").Text());
    put(workspace, L"local_os", hstring(L"windows"));
    put(workspace, L"remote_os",
        hstring(
            saved ? (control<ComboBox>(L"SavedOs").SelectedIndex() == 1   ? L"macos"
                     : control<ComboBox>(L"SavedOs").SelectedIndex() == 2 ? L"windows"
                                                                          : L"unknown")
                  : (control<ComboBox>(L"RemoteOs").SelectedIndex() == 0 ? L"macos" : L"windows")));
    put(workspace, L"local_role",
        hstring(control<ComboBox>(saved ? L"SavedRole" : L"LocalRole").SelectedIndex() == 1
                    ? L"b"
                    : L"a"));
    auto mode = control<ComboBox>(saved ? L"SavedMode" : L"Mode").SelectedIndex();
    put(workspace, L"mode", hstring(mode == 1 ? L"single" : mode == 2 ? L"dual" : L"unverified"));
    put(workspace, L"single_controller",
        hstring(
            control<ComboBox>(saved ? L"SavedController" : L"SingleController").SelectedIndex() == 1
                ? L"remote"
                : L"local"));
    auto previousWorkspace = source;
    bool changedScope = false;
    for (auto key : {L"mode", L"single_controller", L"local_role", L"local_os", L"remote_os"})
        if (str(previousWorkspace, key,
                key == std::wstring_view(L"mode")                ? hstring(L"unverified")
                : key == std::wstring_view(L"single_controller") ? hstring(L"local")
                : key == std::wstring_view(L"local_role")        ? hstring(L"a")
                : key == std::wstring_view(L"local_os")
                    ? hstring(L"windows")
                    : hstring(L"unknown")) != str(workspace, key))
            changedScope = true;
    if (changedScope) {
        put(workspace, L"verification", hstring(L"not_checked"));
        JsonObject evidence;
        for (auto key : {L"local", L"remote"}) {
            JsonObject direction;
            put(direction, L"result", hstring(L"unverified"));
            put(direction, L"source", hstring(L"none"));
            put(direction, L"note", hstring{});
            evidence.SetNamedValue(key, direction);
        }
        workspace.SetNamedValue(L"evidence", evidence);
        put(workspace, L"peer_confirmed", false);
        put(workspace, L"peer_paused_confirmed", false);
        clearAttestations();
    }
    return workspace;
}
bool ControllerView::setupNames(bool advance) {
    auto workspace = workspaceFromControls(obj(obj(snapshot, L"setup"), L"workspace"));
    auto value = command(hasObject(snapshot, L"setup") ? L"setup_workspace" : L"setup_draft_begin");
    put(value, L"name", control<TextBox>(L"WorkspaceName").Text());
    value.SetNamedValue(L"workspace", workspace);
    if (flag(send(value), L"ok")) {
        editorPending = false;
        if (!actionOverride)
            snapshot = response(pk_snapshot(controller));
        editorProfileId = str(obj(snapshot, L"setup"), L"profile_id");
        if (advance) {
            draftLoaded = false;
            setupStep = 1;
        }
        persistEditor();
        return true;
    }
    return false;
}
std::filesystem::path ControllerView::editorPath() {
    return std::filesystem::path(std::wstring(str(snapshot, L"config_path"))).parent_path() /
           L"windows-setup-editor.json";
}
bool ControllerView::namesValid() {
    auto normalized = [](hstring const &text) {
        std::wstring value(text);
        auto first = value.find_first_not_of(L" \t\r\n"), last = value.find_last_not_of(L" \t\r\n");
        return first == std::wstring::npos ? std::wstring{} : value.substr(first, last - first + 1);
    };
    auto name = normalized(control<TextBox>(L"WorkspaceName").Text()),
         local = normalized(control<TextBox>(L"DraftLocalName").Text()),
         remote = normalized(control<TextBox>(L"DraftRemoteName").Text());
    for (auto const &value : {name, local, remote})
        if (value.empty() || value.size() > 80 ||
            std::any_of(value.begin(), value.end(), [](wchar_t c) { return c < 32 || c == 127; }))
            return false;
    return CompareStringOrdinal(local.c_str(), static_cast<int>(local.size()), remote.c_str(),
                                static_cast<int>(remote.size()), TRUE) != CSTR_EQUAL;
}
bool ControllerView::editorHasContent() {
    return !control<TextBox>(L"WorkspaceName").Text().empty() ||
           control<TextBox>(L"DraftLocalName").Text() != L"This computer" ||
           control<TextBox>(L"DraftRemoteName").Text() != L"Other computer" ||
           !control<TextBox>(L"JoinRecord").Text().empty() ||
           !control<TextBox>(L"PeerRecord").Text().empty();
}
JsonObject ControllerView::editorState() {
    JsonObject value;
    value.SetNamedValue(L"version", JsonValue::CreateNumberValue(1));
    for (auto key : {L"WorkspaceName", L"DraftLocalName", L"DraftRemoteName", L"JoinRecord",
                     L"PeerRecord", L"EvidenceNote"})
        put(value, key, control<TextBox>(key).Text());
    for (auto key : {L"LocalRole", L"RemoteOs", L"Mode", L"SingleController"})
        value.SetNamedValue(key,
                            JsonValue::CreateNumberValue(control<ComboBox>(key).SelectedIndex()));
    auto setup = obj(snapshot, L"setup"), workspace = obj(setup, L"workspace");
    put(value, L"profile_id", str(setup, L"profile_id", editorProfileId));
    bool pending = !hasObject(snapshot, L"setup") ||
                   control<TextBox>(L"WorkspaceName").Text() != str(setup, L"name") ||
                   control<TextBox>(L"DraftLocalName").Text() != str(workspace, L"local_name") ||
                   control<TextBox>(L"DraftRemoteName").Text() != str(workspace, L"remote_name");
    auto mode = control<ComboBox>(L"Mode").SelectedIndex();
    pending =
        pending ||
        str(workspace, L"mode") != (mode == 1   ? L"single"
                                    : mode == 2 ? L"dual"
                                                : L"unverified") ||
        str(workspace, L"remote_os") !=
            (control<ComboBox>(L"RemoteOs").SelectedIndex() == 0 ? L"macos" : L"windows") ||
        str(workspace, L"local_role") !=
            (control<ComboBox>(L"LocalRole").SelectedIndex() == 1 ? L"b" : L"a") ||
        str(workspace, L"single_controller") !=
            (control<ComboBox>(L"SingleController").SelectedIndex() == 1 ? L"remote" : L"local");
    put(value, L"pending", pending);
    return value;
}
bool ControllerView::persistEditor() {
    if (startupBlocked || !controller || str(snapshot, L"config_path").empty() ||
        !editorHasContent())
        return true;
    try {
        auto path = editorPath();
        auto data = to_string(editorState().Stringify());
        if (data == lastEditorSaved)
            return true;
        std::filesystem::create_directories(path.parent_path());
        auto temporary = path;
        temporary += L".tmp";
        {
            std::ofstream file(temporary, std::ios::binary | std::ios::trunc);
            file << data;
            file.close();
            if (!file)
                throw std::runtime_error("Cannot save setup editor draft");
        }
        if (!MoveFileExW(temporary.c_str(), path.c_str(),
                         MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))
            throw std::runtime_error("Cannot replace setup editor draft");
        lastEditorSaved = data;
        return true;
    } catch (...) {
        auto bar = control<InfoBar>(L"ActionError");
        bar.Message(zh ? L"无法保存本机设置草稿。请保留此窗口并重试。"
                       : L"Cannot save the local setup draft. Keep this window open and retry.");
        bar.IsOpen(true);
        return false;
    }
}
bool ControllerView::restoreEditor() {
    if (str(snapshot, L"config_path").empty())
        return false;
    try {
        auto path = editorPath();
        if (!std::filesystem::exists(path))
            return false;
        if (std::filesystem::file_size(path) > 65536)
            throw std::runtime_error("Setup editor draft is too large");
        std::ifstream file(path, std::ios::binary);
        std::string data((std::istreambuf_iterator<char>(file)), {});
        auto value = JsonObject::Parse(to_hstring(data));
        if (value.GetNamedNumber(L"version", 0) != 1)
            throw std::runtime_error("Unsupported editor draft version");
        updating = true;
        for (auto key : {L"WorkspaceName", L"DraftLocalName", L"DraftRemoteName", L"JoinRecord",
                         L"PeerRecord", L"EvidenceNote"}) {
            auto content = str(value, key);
            if (content.size() > 16384)
                throw std::runtime_error("Setup editor field is too large");
            control<TextBox>(key).Text(content);
        }
        for (auto key : {L"LocalRole", L"RemoteOs", L"Mode", L"SingleController"}) {
            int index = static_cast<int>(value.GetNamedNumber(key, 0));
            control<ComboBox>(key).SelectedIndex(
                index >= 0 && static_cast<uint32_t>(index) < control<ComboBox>(key).Items().Size()
                    ? index
                    : 0);
        }
        editorProfileId = str(value, L"profile_id");
        editorPending = flag(value, L"pending");
        editorRestored = true;
        setupStep = 0;
        clearAttestations();
        updating = false;
        lastEditorSaved = data;
        return true;
    } catch (...) {
        updating = false;
        auto bar = control<InfoBar>(L"ActionError");
        bar.Message(
            zh ? L"无法恢复本机设置文字草稿；已有配置仍保留。"
               : L"Cannot restore the local setup text draft; saved enrollments are preserved.");
        bar.IsOpen(true);
        return false;
    }
}
winrt::fire_and_forget ControllerView::beginNewProfile() {
    if (startupBlocked || profilePromptOpen)
        co_return;
    if (hasObject(snapshot, L"setup") || flag(snapshot, L"draft_available") || editorHasContent()) {
        ContentDialog dialog;
        dialog.XamlRoot(root.XamlRoot());
        dialog.Title(box_value(zh ? L"开始新的接线配置？" : L"Start a new wiring profile?"));
        dialog.Content(box_value(
            zh ? L"当前未完成的设置草稿将被替换。已保存的配置会保留。"
               : L"This replaces the unfinished setup draft. Saved profiles are preserved."));
        dialog.PrimaryButtonText(zh ? L"开始新配置" : L"Start new profile");
        dialog.CloseButtonText(zh ? L"保留草稿" : L"Keep draft");
        dialog.DefaultButton(ContentDialogButton::Close);
        profilePromptOpen = true;
        ContentDialogResult decision;
        try {
            decision = co_await dialog.ShowAsync();
        } catch (...) {
            profilePromptOpen = false;
            co_return;
        }
        profilePromptOpen = false;
        if (decision != ContentDialogResult::Primary)
            co_return;
        if ((hasObject(snapshot, L"setup") || flag(snapshot, L"draft_available")) &&
            !action(L"setup_cancel"))
            co_return;
        snapshot = response(pk_snapshot(controller));
    }
    resetEditor();
    page(L"SetupPage");
}
void ControllerView::beginSetup() {
    if (startupBlocked) {
        page(L"DiagnosticsPage");
        return;
    }
    if (!hasObject(snapshot, L"setup") && flag(snapshot, L"draft_available")) {
        if (!action(L"setup_resume"))
            return;
        snapshot = response(pk_snapshot(controller));
        clearAttestations();
        draftLoaded = false;
    }
    if (hasObject(snapshot, L"setup") && !editorProfileId.empty() &&
        editorProfileId != str(obj(snapshot, L"setup"), L"profile_id")) {
        resetEditor();
        loadDraft();
    }
    if (!hasObject(snapshot, L"setup") && !flag(snapshot, L"draft_available"))
        setupStep = 0;
    page(L"SetupPage");
}
void ControllerView::finishLater() {
    if (!persistEditor())
        return;
    if (hasObject(snapshot, L"setup")) {
        if (namesValid() && !setupNames(false))
            return;
        if (!action(L"setup_suspend"))
            return;
    }
    editorPending = !namesValid() || editorPending;
    page(L"OverviewPage");
}
void ControllerView::resetEditor() {
    std::error_code ignored;
    std::filesystem::remove(editorPath(), ignored);
    lastEditorSaved.clear();
    editorProfileId = L"";
    editorPending = false;
    editorRestored = false;
    setupStep = 0;
    draftLoaded = false;
    lastSetupPhase.clear();
    bool before = updating;
    updating = true;
    for (auto key : {L"WorkspaceName", L"JoinRecord", L"PeerRecord", L"EvidenceNote"})
        control<TextBox>(key).Text(L"");
    control<TextBox>(L"DraftLocalName").Text(L"This computer");
    control<TextBox>(L"DraftRemoteName").Text(L"Other computer");
    for (auto key : {L"LocalRole", L"RemoteOs", L"Mode", L"SingleController"})
        control<ComboBox>(key).SelectedIndex(0);
    clearAttestations();
    updating = before;
}
void ControllerView::renderSetupProgress(JsonObject const &setup) {
    auto phase = str(setup, L"phase");
    if (std::wstring(phase) != lastSetupPhase) {
        if (phase == L"usb_ready")
            setupStep = 3;
        if (flag(setup, L"error") && phase == L"display") {
            bool present = std::any_of(displays.begin(), displays.end(), [&](auto const &display) {
                return str(display, L"id") == str(setup, L"display_id");
            });
            setupStep = present ? 2 : 1;
        }
        lastSetupPhase = std::wstring(phase);
    }
    if (flag(setup, L"error")) {
        if (phase == L"usb_ready" || phase == L"exchange")
            setupStep = 3;
        else if (phase == L"usb_retry")
            setupStep = 2;
        else if (phase == L"display") {
            bool present = std::any_of(displays.begin(), displays.end(), [&](auto const &display) {
                return str(display, L"id") == str(setup, L"display_id");
            });
            setupStep = present ? 2 : 1;
        }
    }
    for (auto item : {std::pair{L"SetupIdentity", 0}, std::pair{L"SetupMonitor", 1},
                      std::pair{L"SetupUsb", 2}, std::pair{L"SetupInputs", 3},
                      std::pair{L"SetupEvidence", 4}, std::pair{L"SetupFinish", 5}})
        control<StackPanel>(item.first)
            .Visibility(setupStep == item.second ? Visibility::Visible : Visibility::Collapsed);
    text(L"SetupProgress",
         (zh ? hstring(L"步骤 ") : hstring(L"Step ")) + to_hstring(setupStep + 1) + L" / 6");
    control<Button>(L"SetupBack").IsEnabled(setupStep > 0);
    control<Button>(L"SetupNext")
        .Visibility(setupStep == 3 || setupStep == 4 ? Visibility::Visible : Visibility::Collapsed);
    control<Button>(L"SetupNext").IsEnabled(setupStep == 4 || flag(setup, L"ready"));
    control<TextBlock>(L"RecoveryHint")
        .Visibility(setupStep >= 2 ? Visibility::Visible : Visibility::Collapsed);
    auto phaseBar = control<InfoBar>(L"SetupPhase");
    phaseBar.Visibility(setup.Size() > 0 ? Visibility::Visible : Visibility::Collapsed);
    phaseBar.Title(zh ? L"当前步骤" : L"Current step");
    phaseBar.Message(
        str(setup, L"message", zh ? L"保存设置详情后继续。" : L"Save setup details to continue."));
    phaseBar.Severity(flag(setup, L"error") ? InfoBarSeverity::Error
                                            : InfoBarSeverity::Informational);
}
void ControllerView::finishSetup(bool enable) {
    if (!namesValid()) {
        setupStep = 0;
        auto bar = control<InfoBar>(L"ActionError");
        bar.Message(zh ? L"请填写工作区名称及两个不同的电脑名称。"
                       : L"Complete the workspace name and two different computer names.");
        bar.IsOpen(true);
        return;
    }
    if (!setupNames(false))
        return;
    auto value = command(L"setup_finish");
    put(value, L"enable", enable);
    setupCompleted(send(value));
}
void ControllerView::setupCompleted(JsonObject const &result) {
    if (!flag(result, L"ok"))
        return;
    resetEditor();
    page(L"OverviewPage");
    if (!str(result, L"warning").empty()) {
        auto bar = control<InfoBar>(L"PersistentWarning");
        bar.Message(str(result, L"warning"));
        bar.IsOpen(true);
    }
}
void ControllerView::clearAttestations(bool clearPeer) {
    for (auto name : {L"ObservedLocal", L"ObservedRemote", L"PeerPaused"})
        control<CheckBox>(name).IsChecked(false);
    if (clearPeer)
        control<CheckBox>(L"PeerBindingConfirmed").IsChecked(false);
}
bool ControllerView::setupAction(wchar_t const *type) {
    clearAttestations();
    return action(type);
}
void ControllerView::reconcileSetupEvidence(JsonObject const &before, JsonObject const &after) {
    auto fingerprint = [](JsonObject const &setup) {
        JsonObject key;
        for (auto name : {L"display_id", L"local_input", L"remote_input", L"hubs"})
            if (setup.HasKey(name))
                key.SetNamedValue(name, setup.Lookup(name));
        auto workspace = obj(setup, L"workspace");
        for (auto name :
             {L"mode", L"single_controller", L"local_role", L"local_os", L"remote_os", L"revision"})
            if (workspace.HasKey(name))
                key.SetNamedValue(name, workspace.Lookup(name));
        return key.Stringify();
    };
    if (fingerprint(before) != fingerprint(after) ||
        (str(obj(before, L"workspace"), L"verification") != L"not_checked" &&
         str(obj(after, L"workspace"), L"verification") == L"not_checked"))
        clearAttestations();
}
void ControllerView::qualify() {
    auto value = command(L"setup_evidence");
    JsonObject evidence;
    bool local = control<CheckBox>(L"ObservedLocal").IsChecked().Value(),
         remote = control<CheckBox>(L"ObservedRemote").IsChecked().Value();
    for (auto entry : {std::pair{L"local", local}, std::pair{L"remote", remote}}) {
        JsonObject direction;
        put(direction, L"result", hstring(entry.second ? L"observed" : L"unverified"));
        put(direction, L"source", hstring(entry.second ? L"user_observed" : L"none"));
        put(direction, L"note", control<TextBox>(L"EvidenceNote").Text());
        evidence.SetNamedValue(entry.first, direction);
    }
    value.SetNamedValue(L"evidence", evidence);
    auto mode = str(obj(obj(snapshot, L"setup"), L"workspace"), L"mode");
    put(value, L"verification",
        hstring(local && remote ? (mode == L"single" ? L"single_observed" : L"dual_observed")
                                : L"not_checked"));
    put(value, L"peer_paused_confirmed", control<CheckBox>(L"PeerPaused").IsChecked().Value());
    if (flag(send(value), L"ok"))
        setupStep = 5;
}
void ControllerView::translate() {
    if (auto item =
            control<NavigationView>(L"Navigation").SettingsItem().try_as<NavigationViewItem>())
        item.Content(box_value(zh ? L"应用设置" : L"App settings"));
    struct Phrase {
        wchar_t const *name;
        wchar_t const *en;
        wchar_t const *cn;
    };
    for (
        auto p :
        {Phrase{L"BlockedTitle", L"PortKey could not start", L"PortKey 无法启动"},
         {L"BlockedDiagnostics", L"Inspect read-only diagnostics", L"查看只读诊断"},
         {L"CollectDiagnostics", L"Collect diagnostic preview", L"收集诊断预览"},
         {L"SavedMode", L"Installation plan (local profile)", L"安装方式（本地工作区）"},
         {L"SavedController", L"Designated single controller", L"指定的单机控制器"},
         {L"SavedRole", L"This computer’s role", L"本机角色"},
         {L"SavedOs", L"Other computer’s operating system", L"另一台电脑的操作系统"},
         {L"JoinExpander", L"Continue from another computer (optional)",
          L"使用另一台电脑的记录继续（可选）"},
         {L"EmptyTitle", L"Make room for both computers.", L"让两台电脑自由切换。"},
         {L"EmptyPurpose",
          L"Connect two awake computers to one shared display and physical USB switch. PortKey "
          L"follows the switch and selects your enrolled display input.",
          L"将两台唤醒的电脑连接到共享显示器与实体 USB 切换器。PortKey 会跟随 USB "
          L"切换，为已配置的显示器选择对应输入。"},
         {L"SetupBack", L"Back", L"上一步"},
         {L"SetupNext", L"Continue", L"继续"},
         {L"JoinRecord", L"Continue from another computer's record (optional)",
          L"使用另一台电脑的记录继续（可选）"},
         {L"JoinWorkspace", L"Preview record and join workspace draft",
          L"核对记录并加入工作区草稿"},
         {L"ExportDiagnostics", L"Save diagnostic preview locally…", L"将诊断预览保存到本地…"},
         {L"NavOverview", L"Overview", L"概览"},
         {L"NavProfiles", L"Profiles", L"配置"},
         {L"NavSetup", L"Guided setup", L"引导设置"},
         {L"NavDiagnostics", L"Diagnostics", L"诊断"},
         {L"SettingsTitle", L"App settings", L"应用设置"},
         {L"LanguageHeading", L"Language", L"语言"},
         {L"TrayHint", L"Closing this window keeps PortKey running in the tray.",
          L"关闭此窗口后，PortKey 仍在系统托盘中运行。"},
         {L"Quit", L"Quit PortKey", L"退出 PortKey"},
         {L"OverviewTitle", L"Your profiles", L"你的配置"},
         {L"ThisBadge", L"Windows · This computer", L"Windows · 本机"},
         {L"PeerUnknown", L"Peer runtime unknown", L"另一台电脑的运行状态未知"},
         {L"LocalSemantics",
          L"Manual selection changes the display only. Pause and manual holds apply to this "
          L"computer; the other app is independent.",
          L"手动选择仅切换显示器。暂停和手动保持仅对本机有效；另一台电脑的应用独立运行。"},
         {L"ResumeDraft", L"Resume saved setup", L"继续已保存的设置"},
         {L"OverviewSetup", L"Set up a profile", L"添加配置"},
         {L"ProfilesTitle", L"Profiles", L"配置"},
         {L"OverviewProfiles", L"Choose a profile", L"选择配置"},
         {L"OverviewRestoreAuto", L"Restore automatic recognition", L"恢复自动识别"},
         {L"RestoreAuto", L"Restore automatic recognition", L"恢复自动识别"},
         {L"ViewingLabel", L"Viewing profile", L"正在查看的配置"},
         {L"ProfileLocalOnly",
          L"Changes this computer only; the other app is not updated. Choose matching wiring "
          L"profiles on both computers.",
          L"仅在这台电脑切换配置；另一台不会同步更改。请在两台电脑上选择对应的接线配置。"},
         {L"ProfilesHint",
          L"PortKey recognizes the connected display automatically, then uses its saved wiring "
          L"choice. If several wiring profiles share one display, choose the one matching your "
          L"cables. Browsing does not activate a profile.",
          L"PortKey "
          L"自动识别连接的显示器，并使用为该显示器记住的接线配置。同一显示器有多个配置时，请选择与"
          L"当前接线对应的配置。仅查看不会激活配置。"},
         {L"SavedProfileName", L"Profile name", L"配置名称"},
         {L"SavedLocalName", L"This computer's name", L"本机名称"},
         {L"SavedRemoteName", L"Other computer's name", L"另一台电脑的名称"},
         {L"SaveNames", L"Save profile settings", L"保存配置设置"},
         {L"ProfilesNew", L"Add a wiring profile", L"添加接线配置"},
         {L"SetupTitle", L"Set up your two computers", L"设置两台电脑"},
         {L"SetupIntro",
          L"Both computers must stay awake with a usable video signal. Setup pauses this app only. "
          L"Finish later keeps your draft and reconciles the previous workspace.",
          L"请保持两台电脑唤醒且有可用视频信号。设置仅暂停本机应用。稍后完成会保存草稿并重新核对先"
          L"前工作区。"},
         {L"WorkspaceName", L"1 · Profile name", L"1 · 配置名称"},
         {L"DraftLocalName", L"This computer (Windows)", L"本机（Windows）"},
         {L"DraftRemoteName", L"Other computer", L"另一台电脑"},
         {L"LocalRole", L"This computer's role", L"本机角色"},
         {L"RemoteOs", L"Other computer's operating system", L"另一台电脑的操作系统"},
         {L"Mode", L"Installation plan", L"安装方式"},
         {L"SingleController", L"Single controller", L"单机控制器"},
         {L"SaveDraftNames", L"Save setup details", L"保存设置详情"},
         {L"MonitorStep", L"2 · Choose the shared display", L"2 · 选择共享显示器"},
         {L"SelectDisplay", L"Use selected display", L"使用所选显示器"},
         {L"UsbStep", L"3 · Recognize the physical USB switch", L"3 · 识别实体 USB 切换器"},
         {L"RecoveryHint",
          L"Before moving control: return USB with its physical button and recover the picture "
          L"with the monitor's input menu. Wait for each instruction; unplugging a keyboard is not "
          L"a host change.",
          L"移走控制权前：可用实体按钮切回 "
          L"USB，并用显示器输入菜单恢复画面。请等待每步提示；拔下键盘不代表主机切换。"},
         {L"DetectUsb", L"Start / retry USB detection", L"开始 / 重试 USB 检测"},
         {L"InputsStep", L"4 · Associate display inputs", L"4 · 关联显示器输入"},
         {L"InputsHint",
          L"Select the named computer's picture using the monitor's own input menu, then read that "
          L"input. PortKey does not guess values or scan arbitrary inputs.",
          L"通过显示器自己的输入菜单选择对应电脑的画面，然后读取该输入。PortKey "
          L"不会猜测数值或扫描任意输入。"},
         {L"ReadLocal", L"Read input showing this computer", L"读取显示本机画面的输入"},
         {L"ReadRemote", L"Read input showing the other computer", L"读取显示另一台电脑画面的输入"},
         {L"ExchangeHint",
          L"For two controllers: on the other computer, join this workspace with this record. "
          L"Enroll its local hardware there. Paste its reciprocal record here, preview and "
          L"explicitly confirm.",
          L"双机控制：在另一台电脑用此记录加入工作区，并配置其本地硬件。将其对应记录粘贴到此处，预"
          L"览并明确确认。"},
         {L"CopyRecord", L"Copy local setup record", L"复制本机设置记录"},
         {L"PeerRecord", L"Other computer's setup record", L"另一台电脑的设置记录"},
         {L"PreviewRecord", L"Preview reciprocal mapping", L"预览对应映射"},
         {L"PeerBindingConfirmed",
          L"I confirm these local enrollments belong to the intended shared display and workspace.",
          L"我确认这些本地配置属于预期的共享显示器和工作区。"},
         {L"ConfirmRecord", L"Adopt previewed mapping locally", L"在本机采用预览映射"},
         {L"QualifyStep", L"5 · Record observed capability", L"5 · 记录已观察到的能力"},
         {L"QualifyHint",
          L"Record only observations already made on this exact setup. A sent command is not proof "
          L"of the picture. These attestations do not run tests or establish peer presence. Leave "
          L"unchecked to save with automation off.",
          L"仅记录此前在此精确配置上观察到的结果。命令已发送不代表画面已切换。这些声明不会执行测试"
          L"或证明另一台电脑在线。保持未勾选可保存并关闭自动切换。"},
         {L"ObservedLocal", L"I observed the intended picture when switching to this computer.",
          L"我观察到切换到本机时显示预期画面。"},
         {L"ObservedRemote",
          L"I observed the intended picture when switching to the other computer.",
          L"我观察到切换到另一台电脑时显示预期画面。"},
         {L"PeerPaused",
          L"For single-controller evidence, I paused or quit the other PortKey app during both "
          L"observations.",
          L"记录单机控制证据时，我在两次观察期间均已暂停或退出另一台电脑的 PortKey。"},
         {L"EvidenceNote", L"Observation notes (optional)", L"观察备注（可选）"},
         {L"RecordEvidence", L"Save explicit observations", L"保存明确观察结果"},
         {L"FinishOff", L"Save with automation off", L"保存并关闭自动切换"},
         {L"FinishEnable", L"Save and enable on this computer", L"保存并在本机启用"},
         {L"FinishLater", L"Finish later", L"稍后完成"},
         {L"CancelDraft", L"Discard setup draft", L"放弃设置草稿"},
         {L"DiagnosticsTitle", L"Diagnostics", L"诊断"},
         {L"DiagnosticsHint",
          L"Collect a versioned preview before copying or saving it. Names, hardware identifiers "
          L"and user input are excluded. Bounded history, retention limits and missing records are "
          L"shown; nothing is uploaded.",
          L"复制或保存前请先收集带版本的预览。不含名称、硬件标识或用户输入。会显示有界历史、保留限"
          L"制及缺失记录；不会上传。"},
         {L"CopyDiagnostics", L"Copy diagnostic preview", L"复制诊断预览"},
         {L"Refresh", L"Refresh device discovery", L"刷新设备发现"}})
        label(p.name, p.en, p.cn);
    auto choices = [this](wchar_t const *name, std::initializer_list<wchar_t const *> options) {
        auto combo = control<ComboBox>(name);
        auto selection = combo.SelectedIndex();
        combo.Items().Clear();
        for (auto item : options)
            combo.Items().Append(box_value(item));
        combo.SelectedIndex(selection < 0 ? 0 : selection);
    };
    choices(L"LocalRole",
            zh ? std::initializer_list<wchar_t const *>{L"电脑 A（本机）", L"电脑 B（本机）"}
               : std::initializer_list<wchar_t const *>{L"Computer A (this computer)",
                                                        L"Computer B (this computer)"});
    choices(L"RemoteOs", {L"macOS", L"Windows"});
    choices(L"Mode", zh ? std::initializer_list<wchar_t const *>{L"稍后决定（未验证）",
                                                                 L"仅一台电脑运行 PortKey",
                                                                 L"两台电脑均运行 PortKey"}
                        : std::initializer_list<wchar_t const *>{L"Decide later (unverified)",
                                                                 L"Run PortKey on one computer",
                                                                 L"Run PortKey on both computers"});
    choices(L"SingleController",
            zh ? std::initializer_list<wchar_t const *>{L"本机", L"另一台电脑"}
               : std::initializer_list<wchar_t const *>{L"This computer", L"Other computer"});
    choices(L"SavedRole",
            zh ? std::initializer_list<wchar_t const *>{L"电脑 A（本机）", L"电脑 B（本机）"}
               : std::initializer_list<wchar_t const *>{L"Computer A (this computer)",
                                                        L"Computer B (this computer)"});
    choices(L"SavedOs",
            zh ? std::initializer_list<wchar_t const *>{L"尚未指定", L"macOS", L"Windows"}
               : std::initializer_list<wchar_t const *>{L"Not specified", L"macOS", L"Windows"});
    choices(L"SavedMode",
            zh ? std::initializer_list<wchar_t const *>{L"未验证", L"单机控制", L"双机控制"}
               : std::initializer_list<wchar_t const *>{L"Unverified", L"Single controller",
                                                        L"Two controllers"});
    choices(L"SavedController",
            zh ? std::initializer_list<wchar_t const *>{L"本机", L"另一台电脑"}
               : std::initializer_list<wchar_t const *>{L"This computer", L"Other computer"});
}
void ControllerView::loadDraft() {
    auto setup = obj(snapshot, L"setup"), workspace = obj(setup, L"workspace");
    if (!hasObject(snapshot, L"setup")) {
        draftLoaded = false;
        return;
    }
    if (draftLoaded)
        return;
    if (editorPending) {
        setupStep = 0;
        draftLoaded = true;
        return;
    }
    control<TextBox>(L"WorkspaceName").Text(str(setup, L"name"));
    control<TextBox>(L"DraftLocalName").Text(str(workspace, L"local_name", L"This computer"));
    control<TextBox>(L"DraftRemoteName").Text(str(workspace, L"remote_name", L"Other computer"));
    control<ComboBox>(L"LocalRole").SelectedIndex(str(workspace, L"local_role") == L"b" ? 1 : 0);
    control<ComboBox>(L"RemoteOs")
        .SelectedIndex(str(workspace, L"remote_os") == L"windows" ? 1 : 0);
    auto mode = str(workspace, L"mode");
    control<ComboBox>(L"Mode").SelectedIndex(mode == L"single" ? 1 : mode == L"dual" ? 2 : 0);
    control<ComboBox>(L"SingleController")
        .SelectedIndex(str(workspace, L"single_controller") == L"remote" ? 1 : 0);
    clearAttestations();
    auto phase = str(setup, L"phase");
    if (phase == L"exchange" || phase == L"reading_input" || phase == L"usb_ready")
        setupStep = 3;
    else if (phase == L"ready")
        setupStep = 4;
    else if (str(setup, L"display_id").empty())
        setupStep = 1;
    else
        setupStep = 2;
    draftLoaded = true;
}
void ControllerView::saveViewedProfile() {
    if (str(selectedProfile, L"profile_id").empty())
        return;
    auto value = command(L"workspace_save");
    put(value, L"target_profile_id", str(selectedProfile, L"profile_id"));
    put(value, L"profile_name", control<TextBox>(L"SavedProfileName").Text());
    value.SetNamedValue(L"workspace",
                        workspaceFromControls(obj(selectedProfile, L"workspace"), true));
    send(value);
}
void ControllerView::useViewedProfile() {
    if (str(selectedProfile, L"profile_id").empty())
        return;
    auto value = command(L"profile_select");
    put(value, L"profile_id", str(selectedProfile, L"profile_id"));
    send(value);
}
void ControllerView::restoreAutomaticProfile() {
    action(L"profile_restore_auto");
}
void ControllerView::updateProfiles(JsonObject const &previous) {
    auto incoming = snapshot.TryLookup(L"profiles"), old = previous.TryLookup(L"profiles");
    if (ticks > 0 && incoming && old && incoming.Stringify() == old.Stringify())
        return;
    auto pinned = str(selectedProfile, L"profile_id");
    auto combo = control<ComboBox>(L"ProfilesList");
    combo.Items().Clear();
    profiles.clear();
    if (incoming && incoming.ValueType() == JsonValueType::Array)
        for (auto const &value : incoming.GetArray()) {
            auto profile = value.GetObject();
            profiles.push_back(profile);
            combo.Items().Append(box_value(str(profile, L"name")));
        }
    int selected = -1;
    for (size_t i = 0; i < profiles.size(); ++i)
        if (str(profiles[i], L"profile_id") == pinned) {
            selected = static_cast<int>(i);
            break;
        }
    if (selected < 0) {
        auto active = str(snapshot, L"active_profile_id");
        for (size_t i = 0; i < profiles.size(); ++i)
            if (str(profiles[i], L"profile_id") == active) {
                selected = static_cast<int>(i);
                break;
            }
    }
    if (selected < 0 && !profiles.empty())
        selected = 0;
    combo.SelectedIndex(selected);
    if (selected < 0)
        selectedProfile = JsonObject{};
    else if (pinned != str(profiles[selected], L"profile_id"))
        profileSelected();
    else
        selectedProfile = profiles[selected];
}
void ControllerView::renderProfileSelection() {
    auto selection = obj(snapshot, L"profile_selection");
    bool manual = str(selection, L"mode") == L"manual";
    auto activeId = str(snapshot, L"active_profile_id");
    hstring activeName;
    for (auto const &profile : profiles)
        if (str(profile, L"profile_id") == activeId)
            activeName = str(profile, L"name");
    hstring mode =
        manual ? (zh ? hstring(L"手动选择配置：") : hstring(L"Manual profile: ")) +
                     str(selection, L"selected_profile_name")
               : (zh ? hstring(L"自动识别配置") : hstring(L"Automatic profile recognition"));
    hstring active = (zh ? hstring(L"正在使用：") : hstring(L"Active profile: ")) +
                     (activeName.empty() ? (zh ? hstring(L"无") : hstring(L"None")) : activeName);
    text(L"SelectionMode", mode);
    text(L"ProfilesSelectionMode", mode);
    text(L"ActiveProfileLine", active);
    text(L"ProfilesActive", active);
    text(L"SelectionReason", str(selection, L"message"));
    text(L"ProfilesSelectionReason", str(selection, L"message"));
    control<ComboBox>(L"ProfilesList")
        .Visibility(profiles.empty() ? Visibility::Collapsed : Visibility::Visible);
    control<TextBlock>(L"ViewingLabel")
        .Visibility(profiles.empty() ? Visibility::Collapsed : Visibility::Visible);
    control<StackPanel>(L"ProfileSelectionSummary")
        .Visibility(!startupBlocked && !profiles.empty() ? Visibility::Visible
                                                         : Visibility::Collapsed);
    bool allowed = !startupBlocked && flag(snapshot, L"controls_available") &&
                   !flag(snapshot, L"editing") && !flag(snapshot, L"sleeping") &&
                   flag(snapshot, L"lifecycle_ok");
    bool viewed = !str(selectedProfile, L"profile_id").empty();
    auto name = str(selectedProfile, L"name");
    text(L"UseProfileLabel", (zh ? hstring(L"使用配置：") : hstring(L"Use profile: ")) + name);
    control<Button>(L"UseProfile").IsEnabled(allowed && viewed);
    control<Button>(L"UseProfile").Visibility(viewed ? Visibility::Visible : Visibility::Collapsed);
    control<StackPanel>(L"ProfileEditor")
        .Visibility(viewed ? Visibility::Visible : Visibility::Collapsed);
    control<Button>(L"SaveNames")
        .IsEnabled(!startupBlocked && !flag(snapshot, L"editing") && viewed);
    control<Button>(L"RestoreAuto").IsEnabled(allowed && manual);
    control<Button>(L"RestoreAuto")
        .Visibility(manual ? Visibility::Visible : Visibility::Collapsed);
    control<Button>(L"OverviewRestoreAuto").IsEnabled(allowed && manual);
    control<Button>(L"OverviewRestoreAuto")
        .Visibility(manual ? Visibility::Visible : Visibility::Collapsed);
    hstring viewedState =
        !viewed ? (zh ? hstring(L"尚无已保存的配置。请添加配置。")
                      : hstring(L"No saved profiles. Add a profile to begin."))
        : str(selectedProfile, L"profile_id") == activeId
            ? (zh ? hstring(L"正在查看当前使用的配置。") : hstring(L"Viewing the active profile."))
            : (zh ? hstring(L"仅在查看；尚未使用此配置。")
                  : hstring(L"Viewing only; this profile is not active."));
    text(L"ViewedProfileState", viewedState);
}
void ControllerView::profileSelected() {
    auto i = control<ComboBox>(L"ProfilesList").SelectedIndex();
    if (i < 0 || size_t(i) >= profiles.size())
        return;
    selectedProfile = profiles[i];
    auto workspace = obj(selectedProfile, L"workspace");
    control<TextBox>(L"SavedProfileName").Text(str(selectedProfile, L"name"));
    control<TextBox>(L"SavedLocalName").Text(str(workspace, L"local_name", L"This computer"));
    control<TextBox>(L"SavedRemoteName").Text(str(workspace, L"remote_name", L"Other computer"));
    bool before = updating;
    updating = true;
    auto mode = str(workspace, L"mode");
    control<ComboBox>(L"SavedMode").SelectedIndex(mode == L"single" ? 1 : mode == L"dual" ? 2 : 0);
    control<ComboBox>(L"SavedController")
        .SelectedIndex(str(workspace, L"single_controller") == L"remote" ? 1 : 0);
    control<ComboBox>(L"SavedRole").SelectedIndex(str(workspace, L"local_role") == L"b" ? 1 : 0);
    auto os = str(workspace, L"remote_os");
    control<ComboBox>(L"SavedOs").SelectedIndex(os == L"macos" ? 1 : os == L"windows" ? 2 : 0);
    updating = before;
    text(L"ProfileDetails",
         verificationLabel(str(workspace, L"verification", L"historical_unknown"), zh));
    renderProfileSelection();
}
void ControllerView::tick() {
    portkeySmokeTrace("tick");
    updating = true;
    auto previous = snapshot;
    snapshot = response(pk_snapshot(controller));
    reconcileSetupEvidence(obj(previous, L"setup"), obj(snapshot, L"setup"));
    bool language = str(snapshot, L"language") == L"zh-Hans";
    if (ticks == 0 || language != zh) {
        zh = language;
        translate();
    }
    control<ComboBox>(L"Language").SelectedIndex(zh ? 1 : 0);
    auto workspace = obj(snapshot, L"workspace"), config = obj(snapshot, L"config"),
         setup = obj(snapshot, L"setup");
    auto local = str(workspace, L"local_name", zh ? L"本机" : L"This computer"),
         remote = str(workspace, L"remote_name", zh ? L"另一台电脑" : L"Other computer");
    auto warning = control<InfoBar>(L"PersistentWarning");
    warning.Message(str(snapshot, L"warning"));
    warning.IsOpen(!str(snapshot, L"warning").empty());
    text(L"LocalName", local);
    text(L"RemoteName", remote);
    text(L"ProfileHeading",
         str(snapshot, L"profile_name", zh ? L"尚未添加配置" : L"No profile enrolled"));
    text(L"RuntimeStatus", str(snapshot, L"status"));
    auto state = str(workspace, L"effective_state", L"unavailable");
    std::wstring stateLabel = zh ? L"需要设置" : L"Setup needed";
    for (auto entry :
         {std::tuple{L"active", L"Automatic switching active here", L"本机自动切换运行中"},
          std::tuple{L"paused", L"Paused on this computer", L"本机已暂停"},
          std::tuple{L"editing", L"Setup in progress · paused here", L"正在设置 · 本机已暂停"},
          std::tuple{L"waiting_usb", L"Waiting to observe USB here", L"正在等待本机 USB"},
          std::tuple{L"attention", L"This workspace needs attention", L"此工作区需要处理"},
          std::tuple{L"sleeping", L"Suspended for sleep", L"因睡眠已挂起"}})
        if (state == std::get<0>(entry))
            stateLabel = zh ? std::get<2>(entry) : std::get<1>(entry);
    text(L"EffectiveState", hstring(stateLabel));
    label(L"Automatic", (L"Automatic on " + std::wstring(local)).c_str(),
          (L"在 " + std::wstring(local) + L" 上自动切换").c_str());
    control<ToggleSwitch>(L"Automatic").IsOn(flag(config, L"automatic"));
    control<ToggleSwitch>(L"Automatic")
        .IsEnabled(!startupBlocked && hasObject(snapshot, L"config") &&
                   !flag(snapshot, L"editing") && flag(snapshot, L"lifecycle_ok") &&
                   !flag(snapshot, L"sleeping"));
    bool safe = !startupBlocked && hasObject(snapshot, L"config") &&
                flag(snapshot, L"lifecycle_ok") && !flag(snapshot, L"editing") &&
                !flag(snapshot, L"sleeping") && !str(snapshot, L"active_display_id").empty();
    control<Button>(L"SwitchLocal").IsEnabled(safe);
    control<Button>(L"SwitchRemote").IsEnabled(safe);
    label(L"SwitchLocal", L"Show this computer", L"显示本机画面");
    label(L"SwitchRemote", L"Show other computer", L"显示另一台电脑画面");
    auto ownership = str(snapshot, L"ownership");
    hstring usb = zh ? L"USB 状态未知；缺失并不代表另一台电脑在线。"
                     : L"USB state unknown; absence does not establish peer presence.";
    if (ownership == L"owned")
        usb = zh ? L"已观察到配置的 USB 切换器位于本机。"
                 : L"Enrolled USB switch observed on this computer.";
    if (ownership == L"partial")
        usb = zh ? L"USB 拓扑不完整；正在等待新检测。"
                 : L"Partial USB topology; waiting for fresh discovery.";
    if (ownership == L"absent")
        usb = zh ? L"本机未检测到配置的 USB 切换器。"
                 : L"Enrolled USB switch is absent from this computer.";
    text(L"UsbState", usb);
    auto mode = str(workspace, L"mode");
    text(L"ModeSummary",
         mode == L"single"
             ? (zh ? L"单机控制：指定的控制器必须保持唤醒且运行。"
                   : L"Single controller: the designated computer must remain awake and running.")
         : mode == L"dual"
             ? (zh ? L"双机控制：两台电脑需分别配置并运行；本机无法得知另一台应用状态。"
                   : L"Two controllers: each computer is enrolled and runs independently; peer app "
                     L"state is unknown.")
             : (zh ? L"安装能力尚未验证。" : L"Installation capability has not been verified."));
    text(L"EvidenceSummary", verificationLabel(str(workspace, L"verification"), zh));
    control<Button>(L"ResumeDraft")
        .Visibility((flag(snapshot, L"draft_available") || editorHasContent()) &&
                            !hasObject(snapshot, L"setup")
                        ? Visibility::Visible
                        : Visibility::Collapsed);
    auto updateList = [this, &previous](wchar_t const *controlName, JsonObject const &now,
                                        JsonObject const &before, wchar_t const *key,
                                        std::vector<JsonObject> &rows, wchar_t const *title) {
        auto incoming = now.TryLookup(key), old = before.TryLookup(key);
        if (ticks > 0 && incoming && old && incoming.Stringify() == old.Stringify())
            return;
        auto combo = control<ComboBox>(controlName);
        auto selection = combo.SelectedIndex();
        auto pinned = str(selectedProfile, L"profile_id");
        combo.Items().Clear();
        rows.clear();
        if (incoming && incoming.ValueType() == JsonValueType::Array)
            for (auto value : incoming.GetArray()) {
                auto row = value.GetObject();
                rows.push_back(row);
                combo.Items().Append(box_value(str(row, title)));
            }
        // Display choice is always explicit, including a single monitor.
        if (wcscmp(controlName, L"DisplaysList") == 0)
            combo.SelectedIndex(-1);
        else {
            if (!pinned.empty())
                for (size_t i = 0; i < rows.size(); ++i)
                    if (str(rows[i], L"profile_id") == pinned)
                        selection = static_cast<int>(i);
            combo.SelectedIndex(selection >= 0 && size_t(selection) < rows.size() ? selection
                                : rows.empty()                                    ? -1
                                                                                  : 0);
        }
    };
    updateList(L"DisplaysList", obj(snapshot, L"inventory"), obj(previous, L"inventory"),
               L"displays", displays, L"label");
    updateProfiles(previous);
    loadDraft();
    bool editing = hasObject(snapshot, L"setup");
    for (auto name : {L"SelectDisplay", L"DetectUsb", L"ReadLocal", L"ReadRemote", L"PreviewRecord",
                      L"RecordEvidence", L"FinishOff", L"FinishEnable"})
        control<Button>(name).IsEnabled(editing);
    control<Button>(L"FinishLater").IsEnabled(editorHasContent() || editing);
    control<Button>(L"CancelDraft")
        .IsEnabled(editorHasContent() || editing || flag(snapshot, L"draft_available"));
    control<Button>(L"SelectDisplay").IsEnabled(editing && !displays.empty());
    renderSetupProgress(setup);
    startupBlocked = !str(snapshot, L"startup_error").empty();
    control<StackPanel>(L"BlockedWelcome")
        .Visibility(startupBlocked ? Visibility::Visible : Visibility::Collapsed);
    text(L"BlockedDetail", str(snapshot, L"startup_error"));
    control<NavigationViewItem>(L"NavSetup").IsEnabled(!startupBlocked);
    control<NavigationViewItem>(L"NavProfiles").IsEnabled(!startupBlocked);
    control<Button>(L"Refresh").IsEnabled(!startupBlocked);
    control<ComboBox>(L"Language").IsEnabled(!startupBlocked);
    renderProfileSelection();
    bool enrolled = !startupBlocked && hasObject(snapshot, L"config");
    control<Grid>(L"DestinationCards")
        .Visibility(enrolled ? Visibility::Visible : Visibility::Collapsed);
    control<Border>(L"OperationalCard")
        .Visibility(enrolled ? Visibility::Visible : Visibility::Collapsed);
    control<StackPanel>(L"OperationalContext")
        .Visibility(enrolled ? Visibility::Visible : Visibility::Collapsed);
    control<TextBlock>(L"ProfileHeading")
        .Visibility(enrolled ? Visibility::Visible : Visibility::Collapsed);
    control<StackPanel>(L"EmptyWelcome")
        .Visibility(enrolled || startupBlocked || !profiles.empty() ? Visibility::Collapsed
                                                                    : Visibility::Visible);
    if (startupBlocked)
        control<Button>(L"ResumeDraft").Visibility(Visibility::Collapsed);
    auto displayState = obj(snapshot, L"display_state");
    auto result = str(displayState, L"result");
    if (result != L"idle" && !result.empty()) {
        hstring outcome = (zh ? hstring(L"显示请求到 ") : hstring(L"Display request to ")) +
                          (str(displayState, L"target") == L"remote" ? remote : local) + L" · ";
        if (result == L"pending")
            outcome = outcome + (zh ? L"等待结果" : L"pending");
        if (result == L"sent_unverified")
            outcome = outcome +
                      (zh ? L"已发送；未确认输入或画面" : L"sent; input and picture unconfirmed");
        if (result == L"readback_confirmed")
            outcome = outcome + (zh ? L"回读确认输入；画面未验证"
                                    : L"input confirmed by readback; picture unverified");
        if (result == L"failed")
            outcome =
                outcome + (zh ? L"失败；请查看恢复指引" : L"failed; review recovery guidance");
        text(L"EvidenceSummary", outcome);
    }
    control<ComboBox>(L"SavedController")
        .Visibility(control<ComboBox>(L"SavedMode").SelectedIndex() == 1 ? Visibility::Visible
                                                                         : Visibility::Collapsed);
    control<ComboBox>(L"SingleController")
        .Visibility(control<ComboBox>(L"Mode").SelectedIndex() == 1 ? Visibility::Visible
                                                                    : Visibility::Collapsed);
    auto setupWorkspace = obj(setup, L"workspace");
    bool dual = str(setupWorkspace, L"mode") == L"dual";
    control<StackPanel>(L"DualExchange")
        .Visibility(dual ? Visibility::Visible : Visibility::Collapsed);
    control<Button>(L"ReadRemote").Visibility(dual ? Visibility::Collapsed : Visibility::Visible);
    control<CheckBox>(L"PeerPaused")
        .Visibility(str(setupWorkspace, L"mode") == L"single" ? Visibility::Visible
                                                              : Visibility::Collapsed);
    control<TextBox>(L"LocalRecord").Text(str(setup, L"code"));
    auto preview = obj(setup, L"peer_preview");
    text(L"PeerPreview", preview.Size()
                             ? str(preview, L"local_name") + L" → " + str(preview, L"remote_name") +
                                   (zh ? L" · 请核对共享显示器及映射"
                                       : L" · review the shared display and reciprocal mapping")
                             : hstring{});
    control<Button>(L"ConfirmRecord")
        .IsEnabled(preview.Size() > 0 &&
                   control<CheckBox>(L"PeerBindingConfirmed").IsChecked().Value());
    auto inputLabel = [](JsonObject const &value, wchar_t const *key) {
        auto input = value.TryLookup(key);
        return input && input.ValueType() == JsonValueType::Number
                   ? to_hstring(static_cast<int>(input.GetNumber()))
                   : hstring(L"—");
    };
    text(L"SetupSummary",
         str(setupWorkspace, L"local_name") + L" ⇄ " + str(setupWorkspace, L"remote_name") + L"\n" +
             (str(setupWorkspace, L"mode") == L"single"
                  ? (zh ? hstring(L"单机控制") : hstring(L"Single controller"))
              : str(setupWorkspace, L"mode") == L"dual"
                  ? (zh ? hstring(L"双机控制") : hstring(L"Two controllers"))
                  : (zh ? hstring(L"未验证") : hstring(L"Unverified"))) +
             L" · " + verificationLabel(str(setupWorkspace, L"verification"), zh) + L"\n" +
             (zh ? hstring(L"已观察输入（诊断）：") : hstring(L"Observed inputs (diagnostic): ")) +
             inputLabel(setup, L"local_input") + L" / " + inputLabel(setup, L"remote_input"));
    control<Button>(L"FinishOff").IsEnabled(flag(setup, L"ready"));
    control<Button>(L"FinishEnable")
        .IsEnabled(flag(setup, L"ready") &&
                   !(str(setupWorkspace, L"mode") == L"single" &&
                     str(setupWorkspace, L"single_controller") == L"remote") &&
                   (str(setupWorkspace, L"verification") == L"single_observed" ||
                    str(setupWorkspace, L"verification") == L"dual_observed"));
    control<Button>(L"CopyDiagnostics")
        .IsEnabled(!control<TextBox>(L"DiagnosticsPreview").Text().empty());
    control<Button>(L"ExportDiagnostics")
        .IsEnabled(!control<TextBox>(L"DiagnosticsPreview").Text().empty());
    updating = false;
    if (!smokeTicks && editorHasContent())
        persistEditor();
    ++ticks;
    if (smokeTicks && blockedSmoke) {
        if (ticks == 1) {
            auto diagnosticResult = send(command(L"diagnostics_preview"));
            smokeBlockedDiagnostics = startupBlocked && flag(diagnosticResult, L"ok") &&
                                      hasObject(diagnosticResult, L"diagnostics") &&
                                      !control<NavigationViewItem>(L"NavSetup").IsEnabled() &&
                                      !control<NavigationViewItem>(L"NavProfiles").IsEnabled() &&
                                      !control<ToggleSwitch>(L"Automatic").IsEnabled();
        }
        if (ticks >= smokeTicks) {
            smokeRecord();
            quit();
        }
        return;
    }
    if (smokeTicks && restoreSmoke) {
        if (ticks == 1) {
            smokeReconstruction = editorRestored && editorPending && !editorProfileId.empty() &&
                                  control<TextBox>(L"WorkspaceName").Text() == L"QA incomplete" &&
                                  control<TextBox>(L"DraftRemoteName").Text().empty();
            beginSetup();
        }
        if (ticks == 3) {
            smokeReconstruction = smokeReconstruction &&
                                  control<TextBox>(L"DraftRemoteName").Text().empty() &&
                                  setupStep == 0;
            if (action(L"setup_cancel")) {
                resetEditor();
                beginSetup();
                smokeReset = setupStep == 0 && !editorHasContent() &&
                             control<TextBox>(L"PeerRecord").Text().empty();
            }
        }
        if (ticks >= smokeTicks) {
            smokeRecord();
            quit();
        }
        return;
    }
    if (smokeTicks) {
        if (ticks == 2) {
            control<TextBox>(L"WorkspaceName").Text(L"QA workspace");
            control<TextBox>(L"DraftLocalName").Text(L"QA local unsaved");
            control<TextBox>(L"DraftRemoteName").Text(L"QA peer unsaved");
            auto navigation = control<NavigationView>(L"Navigation");
            navigation.IsPaneOpen(false);
            navigation.SelectedItem(navigation.SettingsItem());
            control<ComboBox>(L"Language").SelectedIndex(1);
        }
        if (ticks == 4)
            captureWindow(L"native-settings-collapsed.png");
        if (ticks == 5) {
            smokeDraftPreserved =
                control<TextBox>(L"DraftLocalName").Text() == L"QA local unsaved" &&
                control<TextBox>(L"DraftRemoteName").Text() == L"QA peer unsaved";
            smokeChinese = control<TextBlock>(L"OverviewTitle").Text() == L"你的配置";
            auto navigation = control<NavigationView>(L"Navigation");
            smokeSettings =
                !navigation.IsPaneOpen() && !navigation.PaneFooter() &&
                navigation.IsSettingsVisible() &&
                navigation.SelectedItem() == navigation.SettingsItem() &&
                control<StackPanel>(L"SettingsPage").Visibility() == Visibility::Visible &&
                control<StackPanel>(L"SetupPage").Visibility() == Visibility::Collapsed &&
                control<ComboBox>(L"Language").Parent() == root.FindName(L"SettingsPage") &&
                control<Button>(L"Quit").Parent() == root.FindName(L"SettingsPage") &&
                control<TextBlock>(L"SettingsTitle").Text() == L"应用设置";
            captureWindow();
        }
        if (ticks == 6) {
            orchestrationTests();
            profileOrchestrationTests();
            setupNames();
        }
        if (ticks == 7) {
            control<CheckBox>(L"ObservedLocal").IsChecked(true);
            control<CheckBox>(L"ObservedRemote").IsChecked(true);
            control<CheckBox>(L"PeerPaused").IsChecked(true);
            control<CheckBox>(L"PeerBindingConfirmed").IsChecked(true);
            setupAction(L"setup_read_local");
            smokeEvidenceCleared = !control<CheckBox>(L"ObservedLocal").IsChecked().Value() &&
                                   !control<CheckBox>(L"ObservedRemote").IsChecked().Value() &&
                                   !control<CheckBox>(L"PeerPaused").IsChecked().Value() &&
                                   !control<CheckBox>(L"PeerBindingConfirmed").IsChecked().Value();
            // Invalid draft sync must leave the editor text and setup view available.
            auto before = control<TextBox>(L"DraftRemoteName").Text();
            control<TextBox>(L"DraftRemoteName").Text(control<TextBox>(L"DraftLocalName").Text());
            auto invalid = control<TextBox>(L"DraftRemoteName").Text();
            smokeInvalidDraftRetained =
                !setupNames() && control<TextBox>(L"DraftRemoteName").Text() == invalid;
            finishLater();
            control<TextBox>(L"DraftRemoteName").Text(before);
            control<InfoBar>(L"ActionError").IsOpen(false);
        }
        if (ticks == 8) {
            auto value = command(L"language");
            put(value, L"value", hstring(L"en"));
            send(value);
        }
        if (ticks == 8) {
            page(L"OverviewPage");
            control<NavigationView>(L"Navigation").IsPaneOpen(true);
        }
        if (ticks == 9)
            captureWindow(L"native-overview-expanded.png");
        if (ticks == 21) {
            auto diagnosticsResult = send(command(L"diagnostics_preview"));
            auto package = obj(diagnosticsResult, L"diagnostics");
            smokeDiagnostics = flag(diagnosticsResult, L"ok") &&
                               package.GetNamedNumber(L"version", 0) == 1 &&
                               hasObject(package, L"history");
            if (smokeDiagnostics)
                control<TextBox>(L"DiagnosticsPreview").Text(package.Stringify());
            page(L"ProfilesPage");
        }
        if (ticks == 22)
            captureWindow(L"native-profiles.png");
        if (ticks == 23) {
            beginSetup();
        }
        if (ticks == 24) {
            captureWindow();
        }
        if (ticks == 26) {
            control<TextBox>(L"WorkspaceName").Text(L"QA incomplete");
            control<TextBox>(L"DraftLocalName").Text(L"QA local");
            control<TextBox>(L"DraftRemoteName").Text(L"");
            finishLater();
        }
        if (ticks == 10)
            ShowWindow(hwnd, SW_HIDE);
        if (ticks == 20)
            show();
        if (ticks >= smokeTicks) {
            smokeRecord();
            quit();
        }
    }
}
void ControllerView::profileOrchestrationTests() {
    bool wasUpdating = updating;
    updating = true;
    auto oldSelected = selectedProfile;
    auto oldSnapshot = JsonObject::Parse(snapshot.Stringify());
    std::vector<std::pair<wchar_t const *, hstring>> oldTexts;
    for (auto key : {L"SavedProfileName", L"SavedLocalName", L"SavedRemoteName"})
        oldTexts.emplace_back(key, control<TextBox>(key).Text());
    std::vector<std::pair<wchar_t const *, int>> oldIndexes;
    for (auto key : {L"SavedMode", L"SavedController", L"SavedRole", L"SavedOs"})
        oldIndexes.emplace_back(key, control<ComboBox>(key).SelectedIndex());
    auto profile = JsonObject::Parse(
        LR"({"profile_id":"fixture-wiring-two","display_id":"fixture-same-display","name":"Home HDMI 2"})");
    selectedProfile = profile;
    put(snapshot, L"active_profile_id", hstring(L"fixture-wiring-one"));
    control<TextBox>(L"SavedProfileName").Text(L"Home HDMI 2 renamed");
    control<TextBox>(L"SavedLocalName").Text(L"QA local");
    control<TextBox>(L"SavedRemoteName").Text(L"QA peer");
    std::vector<JsonObject> actions;
    actionOverride = [&](JsonObject const &value) {
        actions.push_back(JsonObject::Parse(value.Stringify()));
        JsonObject result;
        put(result, L"ok", true);
        return result;
    };
    saveViewedProfile();
    useViewedProfile();
    restoreAutomaticProfile();
    smokeProfiles = actions.size() == 3 && str(actions[0], L"type") == L"workspace_save" &&
                    str(actions[0], L"target_profile_id") == L"fixture-wiring-two" &&
                    !actions[0].HasKey(L"display_id") &&
                    str(actions[1], L"type") == L"profile_select" &&
                    str(actions[1], L"profile_id") == L"fixture-wiring-two" &&
                    str(actions[2], L"type") == L"profile_restore_auto" &&
                    str(snapshot, L"active_profile_id") == L"fixture-wiring-one";
    auto oldProfiles = profiles;
    bool oldZh = zh;
    zh = false;
    auto first = JsonObject::Parse(profile.Stringify());
    put(first, L"profile_id", hstring(L"fixture-wiring-one"));
    put(first, L"name", hstring(L"Home HDMI 1"));
    profiles = {first, profile};
    JsonArray profileRows;
    profileRows.Append(first);
    profileRows.Append(profile);
    snapshot.SetNamedValue(L"profiles", profileRows);
    updateProfiles(JsonObject{});
    smokeProfiles = smokeProfiles && profiles.size() == 2 &&
                    control<ComboBox>(L"ProfilesList").SelectedIndex() == 1 &&
                    str(selectedProfile, L"profile_id") == L"fixture-wiring-two";
    JsonArray reordered;
    reordered.Append(profile);
    reordered.Append(first);
    snapshot.SetNamedValue(L"profiles", reordered);
    updateProfiles(JsonObject{});
    smokeProfiles = smokeProfiles && profiles.size() == 2 &&
                    control<ComboBox>(L"ProfilesList").SelectedIndex() == 0 &&
                    str(selectedProfile, L"profile_id") == L"fixture-wiring-two";
    put(snapshot, L"controls_available", true);
    put(snapshot, L"lifecycle_ok", true);
    put(snapshot, L"editing", false);
    put(snapshot, L"sleeping", false);
    auto selection = JsonObject::Parse(
        LR"({"mode":"auto","availability":"ambiguous","message":"Choose the wiring matching your cables."})");
    snapshot.SetNamedValue(L"profile_selection", selection);
    snapshot.SetNamedValue(L"active_profile_id", JsonValue::CreateNullValue());
    renderProfileSelection();
    smokeProfiles =
        smokeProfiles &&
        control<TextBlock>(L"SelectionMode").Text() == L"Automatic profile recognition" &&
        control<TextBlock>(L"SelectionReason").Text() ==
            L"Choose the wiring matching your cables." &&
        control<TextBlock>(L"ViewedProfileState").Text() ==
            L"Viewing only; this profile is not active." &&
        actions.size() == 3;
    put(selection, L"mode", hstring(L"manual"));
    put(selection, L"selected_profile_name", hstring(L"Home HDMI 2"));
    put(selection, L"availability", hstring(L"waiting"));
    put(selection, L"message", hstring(L"Waiting for this profile's enrolled display."));
    renderProfileSelection();
    smokeProfiles = smokeProfiles &&
                    control<TextBlock>(L"SelectionMode").Text() == L"Manual profile: Home HDMI 2" &&
                    control<TextBlock>(L"SelectionReason").Text() ==
                        L"Waiting for this profile's enrolled display." &&
                    control<Button>(L"RestoreAuto").IsEnabled() && actions.size() == 3;
    put(snapshot, L"editing", true);
    renderProfileSelection();
    smokeProfiles = smokeProfiles && !control<Button>(L"UseProfile").IsEnabled() &&
                    !control<Button>(L"RestoreAuto").IsEnabled();
    selectedProfile = JsonObject{};
    saveViewedProfile();
    useViewedProfile();
    smokeProfiles = smokeProfiles && actions.size() == 3;
    actionOverride = {};
    selectedProfile = oldSelected;
    snapshot = oldSnapshot;
    profiles = oldProfiles;
    zh = oldZh;
    updateProfiles(JsonObject{});
    for (auto const &[key, value] : oldTexts)
        control<TextBox>(key).Text(value);
    for (auto const &[key, value] : oldIndexes)
        control<ComboBox>(key).SelectedIndex(value);
    renderProfileSelection();
    updating = wasUpdating;
}
void ControllerView::orchestrationTests() {
    auto source = JsonObject::Parse(
        LR"({"local_name":"QA local","remote_name":"QA peer","local_os":"windows","remote_os":"macos","local_role":"a","mode":"single","single_controller":"local","verification":"single_observed","peer_confirmed":true,"peer_paused_confirmed":true,"evidence":{"local":{"result":"observed","source":"user_observed","note":""},"remote":{"result":"observed","source":"user_observed","note":""}}})");
    bool before = updating;
    updating = true;
    control<TextBox>(L"DraftLocalName").Text(L"QA local");
    control<TextBox>(L"DraftRemoteName").Text(L"QA peer");
    control<ComboBox>(L"RemoteOs").SelectedIndex(0);
    control<ComboBox>(L"LocalRole").SelectedIndex(0);
    control<ComboBox>(L"SingleController").SelectedIndex(0);
    control<ComboBox>(L"Mode").SelectedIndex(2);
    auto dual = workspaceFromControls(source);
    bool toDual = str(dual, L"mode") == L"dual" && str(dual, L"verification") == L"not_checked" &&
                  !flag(dual, L"peer_confirmed") &&
                  str(obj(obj(dual, L"evidence"), L"local"), L"result") == L"unverified";
    auto qualifiedDual = JsonObject::Parse(source.Stringify());
    put(qualifiedDual, L"mode", hstring(L"dual"));
    put(qualifiedDual, L"verification", hstring(L"dual_observed"));
    control<ComboBox>(L"Mode").SelectedIndex(1);
    auto single = workspaceFromControls(qualifiedDual);
    bool toSingle = str(single, L"mode") == L"single" &&
                    str(single, L"verification") == L"not_checked" &&
                    !flag(single, L"peer_paused_confirmed");
    control<TextBox>(L"DraftLocalName").Text(L"Renamed QA local");
    auto rename = workspaceFromControls(source);
    bool namesOnly =
        str(rename, L"verification") == L"single_observed" && flag(rename, L"peer_confirmed");
    control<TextBox>(L"SavedLocalName").Text(L"Saved QA local");
    control<TextBox>(L"SavedRemoteName").Text(L"Saved QA peer");
    control<ComboBox>(L"SavedMode").SelectedIndex(0);
    control<ComboBox>(L"SavedOs").SelectedIndex(0);
    control<ComboBox>(L"SavedRole").SelectedIndex(0);
    control<ComboBox>(L"SavedController").SelectedIndex(0);
    JsonObject oldProfile;
    put(oldProfile, L"verification", hstring(L"historical_unknown"));
    auto legacy = workspaceFromControls(oldProfile, true);
    bool legacyRename = str(legacy, L"verification") == L"historical_unknown";
    control<ComboBox>(L"SavedMode").SelectedIndex(2);
    control<ComboBox>(L"SavedOs").SelectedIndex(1);
    auto savedMode = workspaceFromControls(source, true);
    bool savedScope =
        str(savedMode, L"mode") == L"dual" && str(savedMode, L"verification") == L"not_checked";
    smokeModes = toDual && toSingle && namesOnly && legacyRename && savedScope;
    auto oldDisplays = displays;
    JsonObject display;
    put(display, L"id", hstring(L"frontend-fixture-display"));
    displays = {display};
    auto failure = JsonObject::Parse(
        LR"({"phase":"usb_ready","error":true,"message":"Local read unavailable","display_id":"frontend-fixture-display"})");
    setupStep = 4;
    renderSetupProgress(failure);
    bool localError = setupStep == 3 &&
                      control<InfoBar>(L"SetupPhase").Visibility() == Visibility::Visible &&
                      control<InfoBar>(L"SetupPhase").Message() == L"Local read unavailable";
    put(failure, L"message", hstring(L"Remote read unavailable"));
    setupStep = 5;
    renderSetupProgress(failure);
    bool remoteError =
        setupStep == 3 && control<InfoBar>(L"SetupPhase").Message() == L"Remote read unavailable";
    put(failure, L"phase", hstring(L"display"));
    setupStep = 5;
    renderSetupProgress(failure);
    bool interrupted = setupStep == 2;
    displays.clear();
    setupStep = 4;
    renderSetupProgress(failure);
    bool disconnected = setupStep == 1;
    displays = oldDisplays;
    smokeAsyncRecovery = localError && remoteError && interrupted && disconnected;
    auto originalSnapshot = JsonObject::Parse(snapshot.Stringify());
    bool finishBranches = true;
    for (auto mode : {L"single", L"dual"}) {
        auto fixture = JsonObject::Parse(source.Stringify());
        put(fixture, L"mode", hstring(mode));
        put(fixture, L"verification",
            hstring(wcscmp(mode, L"single") == 0 ? L"single_observed" : L"dual_observed"));
        auto setup = JsonObject::Parse(LR"({"name":"Fixture desk","phase":"ready","ready":true})");
        setup.SetNamedValue(L"workspace", fixture);
        snapshot.SetNamedValue(L"setup", setup);
        control<TextBox>(L"WorkspaceName").Text(L"Fixture desk");
        control<TextBox>(L"DraftLocalName").Text(L"QA local");
        control<TextBox>(L"DraftRemoteName").Text(L"QA peer");
        control<ComboBox>(L"Mode").SelectedIndex(wcscmp(mode, L"single") == 0 ? 1 : 2);
        setupStep = 5;
        std::vector<hstring> order;
        bool payloadCurrent = true;
        actionOverride = [&](JsonObject const &value) {
            order.push_back(str(value, L"type"));
            if (str(value, L"type") == L"setup_workspace")
                payloadCurrent =
                    payloadCurrent && str(obj(value, L"workspace"), L"mode") == mode &&
                    str(obj(value, L"workspace"), L"verification") == str(fixture, L"verification");
            JsonObject result;
            put(result, L"ok", true);
            if (str(value, L"type") == L"setup_finish") {
                payloadCurrent = payloadCurrent && flag(value, L"enable");
                if (wcscmp(mode, L"dual") == 0)
                    put(result, L"warning", hstring(L"Committed with cleanup warning"));
            }
            return result;
        };
        finishSetup(true);
        finishBranches = finishBranches &&
                         order == std::vector<hstring>{L"setup_workspace", L"setup_finish"} &&
                         payloadCurrent && !editorHasContent() && setupStep == 0;
        if (wcscmp(mode, L"dual") == 0)
            finishBranches = finishBranches && control<InfoBar>(L"PersistentWarning").IsOpen();
    }
    auto setup = JsonObject::Parse(LR"({"name":"Failed fixture","phase":"ready","ready":true})");
    setup.SetNamedValue(L"workspace", source);
    snapshot.SetNamedValue(L"setup", setup);
    control<TextBox>(L"WorkspaceName").Text(L"Failed fixture");
    control<TextBox>(L"DraftLocalName").Text(L"QA local");
    control<TextBox>(L"DraftRemoteName").Text(L"QA peer");
    control<ComboBox>(L"Mode").SelectedIndex(1);
    setupStep = 5;
    actionOverride = [](JsonObject const &value) {
        JsonObject result;
        bool ok = str(value, L"type") != L"setup_finish";
        put(result, L"ok", ok);
        if (!ok)
            put(result, L"error", hstring(L"Fixture save failed"));
        return result;
    };
    finishSetup(true);
    smokeFinishFlow = finishBranches && setupStep == 5 &&
                      control<TextBox>(L"WorkspaceName").Text() == L"Failed fixture" &&
                      control<InfoBar>(L"ActionError").IsOpen();
    actionOverride = {};
    snapshot = originalSnapshot;
    control<InfoBar>(L"PersistentWarning").IsOpen(false);
    control<InfoBar>(L"ActionError").IsOpen(false);
    setupStep = 5;
    control<TextBox>(L"JoinRecord").Text(L"test old join");
    JsonObject success;
    put(success, L"ok", true);
    setupCompleted(success);
    beginSetup();
    smokeReset = setupStep == 0 && control<TextBox>(L"WorkspaceName").Text().empty() &&
                 control<TextBox>(L"JoinRecord").Text().empty() &&
                 !control<CheckBox>(L"ObservedLocal").IsChecked().Value();
    control<TextBox>(L"WorkspaceName").Text(L"QA workspace");
    control<TextBox>(L"DraftLocalName").Text(L"QA local unsaved");
    control<TextBox>(L"DraftRemoteName").Text(L"QA peer unsaved");
    updating = before;
    page(L"OverviewPage");
}
void ControllerView::smokeRecord() {
    JsonObject record;
    put(record, L"frontend", hstring(L"C++/WinRT + WinUI 3 XAML"));
    put(record, L"test_leg",
        hstring(blockedSmoke   ? L"blocked_startup_diagnostics"
                : restoreSmoke ? L"process_reconstruction"
                               : L"startup_and_orchestration"));
    put(record, L"config_null", !hasObject(snapshot, L"config"));
    put(record, L"lifecycle_ok", flag(snapshot, L"lifecycle_ok"));
    put(record, L"usb_notifications_ready", notifications.size() == 2);
    put(record, L"automatic_off", !flag(obj(snapshot, L"config"), L"automatic"));
    record.SetNamedValue(L"ticks", JsonValue::CreateNumberValue(ticks));
    put(record, L"native_window_visible", IsWindowVisible(hwnd) != 0);
    put(record, L"new_setup_reset", smokeReset);
    if (blockedSmoke)
        put(record, L"blocked_startup_diagnostics", smokeBlockedDiagnostics);
    else if (restoreSmoke)
        put(record, L"process_reconstructed_incomplete_draft", smokeReconstruction);
    else {
        put(record, L"continued_ticks_while_hidden", ticks >= 20);
        put(record, L"unsaved_draft_preserved_across_language", smokeDraftPreserved);
        put(record, L"chinese_labels_rendered", smokeChinese);
        put(record, L"native_xaml_capture_complete", smokeCaptureComplete);
        put(record, L"stale_attestations_cleared", smokeEvidenceCleared);
        put(record, L"invalid_draft_retained", smokeInvalidDraftRetained);
        put(record, L"mode_edit_orchestration", smokeModes);
        put(record, L"async_error_recovery", smokeAsyncRecovery);
        put(record, L"shared_diagnostics_preview", smokeDiagnostics);
        put(record, L"qualified_finish_orchestration", smokeFinishFlow);
        put(record, L"independent_profile_actions", smokeProfiles);
        put(record, L"collapsed_sidebar_settings", smokeSettings);
    }
    put(record, L"tray_registered", trayReady);
    put(record, L"tray_icon_loaded", tray.hIcon != nullptr);
    NOTIFYICONIDENTIFIER icon{sizeof(icon), hwnd, 1, {}};
    RECT bounds{};
    put(record, L"tray_rectangle_available", SUCCEEDED(Shell_NotifyIconGetRect(&icon, &bounds)));
    std::ofstream file(std::filesystem::path(environment(L"PORTKEY_SMOKE_RESULT")),
                       std::ios::binary);
    file << to_string(record.Stringify());
}

winrt::fire_and_forget ControllerView::captureWindow(wchar_t const *filename) {
    hstring nativeName =
        filename ? filename : (ticks > 20 ? L"native-setup.png" : L"native-window.png");
    try {
        Microsoft::UI::Xaml::Media::Imaging::RenderTargetBitmap bitmap;
        co_await bitmap.RenderAsync(root);
        auto buffer = co_await bitmap.GetPixelsAsync();
        auto reader = Windows::Storage::Streams::DataReader::FromBuffer(buffer);
        std::vector<uint8_t> pixels(buffer.Length());
        reader.ReadBytes(pixels);
        auto resultPath = std::filesystem::path(environment(L"PORTKEY_SMOKE_RESULT"));
        auto directory = co_await Windows::Storage::StorageFolder::GetFolderFromPathAsync(
            resultPath.parent_path().wstring());
        auto file = co_await directory.CreateFileAsync(
            ticks > 20 ? L"setup-xaml-render.png" : L"xaml-render.png",
            Windows::Storage::CreationCollisionOption::ReplaceExisting);
        auto stream = co_await file.OpenAsync(Windows::Storage::FileAccessMode::ReadWrite);
        auto encoder = co_await Windows::Graphics::Imaging::BitmapEncoder::CreateAsync(
            Windows::Graphics::Imaging::BitmapEncoder::PngEncoderId(), stream);
        encoder.SetPixelData(Windows::Graphics::Imaging::BitmapPixelFormat::Bgra8,
                             Windows::Graphics::Imaging::BitmapAlphaMode::Premultiplied,
                             bitmap.PixelWidth(), bitmap.PixelHeight(), 96, 96, pixels);
        co_await encoder.FlushAsync();
        RECT bounds{};
        GetWindowRect(hwnd, &bounds);
        int width = bounds.right - bounds.left, height = bounds.bottom - bounds.top;
        HDC screen = GetDC(hwnd), memory = CreateCompatibleDC(screen);
        HBITMAP image = CreateCompatibleBitmap(screen, width, height);
        auto old = SelectObject(memory, image);
        bool rendered = PrintWindow(hwnd, memory, 2) != 0;
        BITMAPINFO info{};
        info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        std::vector<uint8_t> nativePixels(size_t(width) * height * 4);
        SelectObject(memory, old);
        if (rendered)
            rendered = GetDIBits(memory, image, 0, height, nativePixels.data(), &info,
                                 DIB_RGB_COLORS) != 0;
        DeleteObject(image);
        DeleteDC(memory);
        ReleaseDC(hwnd, screen);
        if (!rendered)
            throw std::runtime_error("Cannot capture the app's native window");
        auto nativeFile = co_await directory.CreateFileAsync(
            nativeName, Windows::Storage::CreationCollisionOption::ReplaceExisting);
        auto nativeStream =
            co_await nativeFile.OpenAsync(Windows::Storage::FileAccessMode::ReadWrite);
        auto nativeEncoder = co_await Windows::Graphics::Imaging::BitmapEncoder::CreateAsync(
            Windows::Graphics::Imaging::BitmapEncoder::PngEncoderId(), nativeStream);
        nativeEncoder.SetPixelData(Windows::Graphics::Imaging::BitmapPixelFormat::Bgra8,
                                   Windows::Graphics::Imaging::BitmapAlphaMode::Ignore, width,
                                   height, 96, 96, nativePixels);
        co_await nativeEncoder.FlushAsync();
        smokeCaptureComplete = true;
    } catch (...) {
        smokeCaptureComplete = false;
    }
}
