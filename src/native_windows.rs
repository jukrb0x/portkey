//! Native Win32 settings and notification-area UI. The controller owns all hardware work.
use crate::controller::Controller;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Controls::*, Input::KeyboardAndMouse::EnableWindow, Shell::*, WindowsAndMessaging::*},
};

const TRAY: u32 = WM_APP + 1;
const OPEN: usize = 100;
const LOCAL: usize = 101;
const REMOTE: usize = 102;
const AUTO: usize = 103;
const QUIT: usize = 104;
const REFRESH: usize = 105;
const EDIT: usize = 106;
const SAVE: usize = 107;
const LANGUAGE: usize = 108;
const DISPLAY: usize = 109;
const HUB1: usize = 110;
const HUB2: usize = 111;
const INPUT1: usize = 112;
const INPUT2: usize = 113;
const STATUS: usize = 114;
const PATH: usize = 115;
const PROFILE_NAME: usize = 116;
const NEW_SETUP: usize = 117;
const ADVANCED: usize = 118;
const SETUP_HELP: usize = 119;
const SETUP_CODE: usize = 120;
const PEER_CODE: usize = 121;
const SETUP_NEXT: usize = 122;
const COPY_CODE: usize = 123;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn tr(zh: bool, en: &'static str, cn: &'static str) -> &'static str {
    if zh {
        cn
    } else {
        en
    }
}

const USB_DEVICE_INTERFACE: windows_sys::core::GUID =
    windows_sys::core::GUID::from_u128(0xa5dcbf10_6530_11d2_901f_00c04fb951ed);
const USB_HUB_INTERFACE: windows_sys::core::GUID =
    windows_sys::core::GUID::from_u128(0xf18a0e88_c30c_11d0_8815_00a0c906bed8);

fn usb_interface_hint(event: u32, kind: u32, class: windows_sys::core::GUID) -> bool {
    matches!(event, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE)
        && kind == DBT_DEVTYP_DEVICEINTERFACE
        && [USB_DEVICE_INTERFACE, USB_HUB_INTERFACE]
            .iter()
            .any(|known| {
                (class.data1, class.data2, class.data3, class.data4)
                    == (known.data1, known.data2, known.data3, known.data4)
            })
}
unsafe fn is_usb_interface_hint(event: u32, payload: LPARAM) -> bool {
    if !matches!(event, DBT_DEVICEARRIVAL | DBT_DEVICEREMOVECOMPLETE) || payload == 0 {
        return false;
    }
    let header = &*(payload as *const DEV_BROADCAST_HDR);
    if header.dbch_devicetype != DBT_DEVTYP_DEVICEINTERFACE
        || header.dbch_size < size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32
    {
        return false;
    }
    let interface = &*(payload as *const DEV_BROADCAST_DEVICEINTERFACE_W);
    usb_interface_hint(event, interface.dbcc_devicetype, interface.dbcc_classguid)
}
struct UsbNotifications(Vec<HDEVNOTIFY>);
impl UsbNotifications {
    unsafe fn register(window: HWND) -> Self {
        let mut handles = Vec::new();
        for class in [USB_DEVICE_INTERFACE, USB_HUB_INTERFACE] {
            let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                dbcc_size: size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
                dbcc_classguid: class,
                ..std::mem::zeroed()
            };
            let handle = RegisterDeviceNotificationW(
                window,
                &filter as *const _ as *const _,
                DEVICE_NOTIFY_WINDOW_HANDLE,
            );
            if handle.is_null() {
                crate::timing::event("usb_notification_registration_failed", "native", None, None);
            } else {
                handles.push(handle);
            }
        }
        crate::timing::event(
            "usb_notification_registrations",
            "native",
            None,
            Some(handles.len() as u64),
        );
        Self(handles)
    }
}
impl Drop for UsbNotifications {
    fn drop(&mut self) {
        for handle in self.0.drain(..) {
            unsafe {
                UnregisterDeviceNotification(handle);
            }
        }
    }
}

struct Ui {
    controller: Controller,
    window: HWND,
    snapshot: Value,
    inventory: Value,
    displays: Vec<Value>,
    hubs: Vec<Value>,
    initialized: bool,
    zh: bool,
    updating: bool,
    advanced: bool,
    tray: NOTIFYICONDATAW,
    taskbar_created: u32,
    action_error: Option<String>,
    pending_setup_error: Option<String>,
}
impl Ui {
    unsafe fn control(&self, id: usize) -> HWND {
        GetDlgItem(self.window, id as i32)
    }
    unsafe fn set_text(&self, id: usize, text: &str) {
        if self.text(id) != text {
            SetWindowTextW(self.control(id), wide(text).as_ptr());
        }
    }
    unsafe fn text(&self, id: usize) -> String {
        let hwnd = self.control(id);
        let mut buffer = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
        let n = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..n as usize])
    }
    unsafe fn checked(&self) -> bool {
        SendMessageW(self.control(AUTO), BM_GETCHECK, 0, 0) == BST_CHECKED as isize
    }
    unsafe fn send(&mut self, action: Value) -> bool {
        let response = self.controller.action(action);
        let ok = response["ok"].as_bool() == Some(true);
        self.action_error = if ok {
            None
        } else {
            Some(
                response["error"]
                    .as_str()
                    .unwrap_or("Action failed")
                    .to_owned(),
            )
        };
        ok
    }
    unsafe fn selected(&self, id: usize, values: &[Value]) -> Option<Value> {
        let index = SendMessageW(self.control(id), CB_GETCURSEL, 0, 0);
        if index < 0 || (id == HUB2 && index == 0) {
            None
        } else {
            values
                .get(index as usize - usize::from(id == HUB2))
                .cloned()
        }
    }
    unsafe fn fill(&self, id: usize, values: &[Value], selected: Option<&str>) {
        let hwnd = self.control(id);
        SendMessageW(hwnd, CB_RESETCONTENT, 0, 0);
        let mut index = None;
        let offset = usize::from(id == HUB2);
        if id == HUB2 {
            SendMessageW(
                hwnd,
                CB_ADDSTRING,
                0,
                wide(tr(self.zh, "None (one USB branch)", "无（单个 USB 分支）")).as_ptr() as isize,
            );
        }
        for (i, value) in values.iter().enumerate() {
            let identity = value["id"].as_str().unwrap_or("");
            let label = value["label"].as_str().unwrap_or(identity);
            let kind = if id == DISPLAY { "displays" } else { "hubs" };
            let online = self.inventory[kind].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| item["id"].as_str() == Some(identity))
            });
            let suffix = if online {
                ""
            } else {
                tr(self.zh, " (unavailable)", "（不可用）")
            };
            let text = if id == DISPLAY {
                let friendly = friendly_display_name(value);
                let name = if friendly.is_empty() {
                    format!(
                        "{} {}",
                        tr(self.zh, "External display", "外接显示器"),
                        i + 1
                    )
                } else if values
                    .iter()
                    .filter(|v| friendly_display_name(v) == friendly)
                    .count()
                    > 1
                {
                    format!("{friendly} · {}", i + 1)
                } else {
                    friendly.to_owned()
                };
                if self.advanced {
                    format!("{name} — {identity}{suffix}")
                } else {
                    format!("{name}{suffix}")
                }
            } else {
                format!("{label} — {identity}{suffix}")
            };
            SendMessageW(hwnd, CB_ADDSTRING, 0, wide(&text).as_ptr() as isize);
            if selected == Some(identity) {
                index = Some(i + offset);
            }
        }
        // Never pick the first discovered device on the user's behalf.
        SendMessageW(
            hwnd,
            CB_SETCURSEL,
            index.unwrap_or(if id == HUB2 { 0 } else { usize::MAX }),
            0,
        );
    }
    unsafe fn tick(&mut self) {
        let snapshot = self.controller.snapshot();
        self.updating = true;
        let zh = snapshot["language"].as_str() == Some("zh-Hans");
        let language_changed = zh != self.zh;
        if !self.initialized || language_changed {
            self.zh = zh;
            self.labels();
        }
        let config = &snapshot["config"];
        let editing = snapshot["editing"].as_bool().unwrap_or(false);
        let seed = should_seed_form(self.initialized, &snapshot, &self.snapshot);
        let inventory = &snapshot["inventory"];
        if seed || inventory != &self.inventory || language_changed {
            let old_display = self.selected(DISPLAY, &self.displays);
            let old_hub1 = self.selected(HUB1, &self.hubs);
            let old_hub2 = self.selected(HUB2, &self.hubs);
            let saved_display = config["display_id"].as_str().map(|id| {
                self.displays
                    .iter()
                    .find(|value| value["id"].as_str() == Some(id))
                    .cloned()
                    .unwrap_or_else(|| json!({"id":id,"label":id}))
            });
            self.displays = merge_choices(
                &inventory["displays"],
                &[old_display.as_ref(), saved_display.as_ref()],
            );
            self.hubs = merge_choices(
                &inventory["hubs"],
                &[
                    old_hub1.as_ref(),
                    old_hub2.as_ref(),
                    Some(&config["hubs"][0]),
                    Some(&config["hubs"][1]),
                ],
            );
            let display_id = if seed || !editing {
                config["display_id"].as_str()
            } else {
                old_display.as_ref().and_then(|v| v["id"].as_str())
            };
            let hub1 = if seed || !editing {
                config["hubs"][0]["id"].as_str()
            } else {
                old_hub1.as_ref().and_then(|v| v["id"].as_str())
            };
            let hub2 = if seed || !editing {
                config["hubs"][1]["id"].as_str()
            } else {
                old_hub2.as_ref().and_then(|v| v["id"].as_str())
            };
            self.inventory = inventory.clone();
            self.fill(DISPLAY, &self.displays, display_id);
            self.fill(HUB1, &self.hubs, hub1);
            self.fill(HUB2, &self.hubs, hub2);
        }
        if seed {
            self.set_text(
                PROFILE_NAME,
                snapshot["profile_name"].as_str().unwrap_or(""),
            );
            self.set_text(
                INPUT1,
                &config["local_input"]
                    .as_u64()
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            );
            self.set_text(
                INPUT2,
                &config["remote_input"]
                    .as_u64()
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            );
        }
        for id in [DISPLAY, HUB1, HUB2, INPUT1, INPUT2, PROFILE_NAME, SAVE] {
            EnableWindow(self.control(id), editing as i32);
        }
        let has_profiles = snapshot["profiles"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
        EnableWindow(
            self.control(EDIT),
            (!config.is_null() || (editing && (has_profiles || !snapshot["setup"].is_null())))
                as i32,
        );
        EnableWindow(self.control(NEW_SETUP), (!editing) as i32);
        self.set_text(
            EDIT,
            if editing {
                tr(self.zh, "Cancel changes", "取消修改")
            } else {
                tr(self.zh, "Guided setup", "引导设置")
            },
        );
        let ready = snapshot["lifecycle_ok"].as_bool() == Some(true)
            && snapshot["sleeping"].as_bool() != Some(true);
        let automatic = config["automatic"].as_bool() == Some(true);
        for id in [LOCAL, REMOTE] {
            EnableWindow(
                self.control(id),
                (!config.is_null() && !editing && ready) as i32,
            );
        }
        EnableWindow(
            self.control(AUTO),
            (!config.is_null() && !editing && (ready || automatic)) as i32,
        );
        SendMessageW(
            self.control(AUTO),
            BM_SETCHECK,
            if config["automatic"].as_bool() == Some(true) {
                BST_CHECKED
            } else {
                BST_UNCHECKED
            } as usize,
            0,
        );
        let status = self
            .action_error
            .as_deref()
            .unwrap_or_else(|| snapshot["status"].as_str().unwrap_or(""));
        let ownership = match snapshot["ownership"].as_str() {
            Some("owned") => tr(self.zh, "This PC", "本机"),
            Some("absent") => tr(
                self.zh,
                "USB switch is not connected to this PC",
                "USB 切换器未连接到本机",
            ),
            Some("partial") => tr(self.zh, "USB switch settling", "USB 切换器状态变化中"),
            _ => tr(self.zh, "Unknown", "未知"),
        };
        let state = format!(
            "{}: {} · {}\r\n{}{}",
            tr(self.zh, "USB ownership", "USB 归属"),
            ownership,
            if automatic {
                tr(self.zh, "Automatic", "自动切换")
            } else {
                tr(self.zh, "Paused", "已暂停")
            },
            if self.action_error.is_some() || snapshot["error"].as_bool() == Some(true) {
                tr(self.zh, "Error: ", "错误：")
            } else {
                ""
            },
            status
        );
        let names = snapshot["profiles"]
            .as_array()
            .map(|profiles| {
                profiles
                    .iter()
                    .filter_map(|profile| profile["name"].as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let active_id = snapshot["active_display_id"].as_str();
        let matched = snapshot["profiles"]
            .as_array()
            .and_then(|profiles| {
                profiles.iter().find(|profile| {
                    active_id.is_some() && profile["display_id"].as_str() == active_id
                })
            })
            .and_then(|profile| profile["name"].as_str())
            .unwrap_or_else(|| tr(self.zh, "None", "无"));
        self.set_text(
            STATUS,
            &format!(
                "{}: {} · {}: {}\r\n{}",
                tr(self.zh, "Matched setup", "匹配的环境"),
                if editing {
                    tr(
                        self.zh,
                        "Editing · automatic switching paused",
                        "编辑中 · 自动切换已暂停",
                    )
                } else {
                    matched
                },
                tr(self.zh, "Saved setups", "已保存环境"),
                if names.is_empty() {
                    tr(self.zh, "None", "无")
                } else {
                    &names
                },
                state
            ),
        );
        let tooltip = format!(
            "PortKey · {} · {}",
            ownership,
            if automatic {
                tr(self.zh, "Automatic", "自动切换")
            } else {
                tr(self.zh, "Paused", "已暂停")
            }
        );
        let mut tip = [0u16; 128];
        let mut encoded: Vec<u16> = tooltip.encode_utf16().take(127).collect();
        if encoded
            .last()
            .is_some_and(|c| (0xd800..=0xdbff).contains(c))
        {
            encoded.pop();
        }
        tip[..encoded.len()].copy_from_slice(&encoded);
        if self.tray.szTip != tip {
            self.tray.szTip = tip;
            Shell_NotifyIconW(NIM_MODIFY, &self.tray);
        }
        self.set_text(PATH, snapshot["config_path"].as_str().unwrap_or(""));
        self.setup_ui(&snapshot);
        self.snapshot = snapshot;
        self.initialized = true;
        self.updating = false;
    }
    unsafe fn setup_ui(&self, snapshot: &Value) {
        let editing = snapshot["editing"].as_bool() == Some(true);
        for id in [
            HUB1, HUB2, INPUT1, INPUT2, SAVE, PATH, 202, 203, 204, 205, 206,
        ] {
            ShowWindow(
                self.control(id),
                if self.advanced { SW_SHOW } else { SW_HIDE },
            );
        }
        let phase = snapshot["setup"]["phase"].as_str().unwrap_or("select");
        let (en, cn, button_en, button_cn) = match phase {
            "display" => ("1. Connect the USB switch to this PC. Leave other devices connected. We will learn the switch when you press its physical button away and back.", "1. 将 USB 切换器连接到本机，保持其他设备连接。按下切换器实体按钮切走再切回，即可识别切换器。", "Capture USB baseline", "记录 USB 初始状态"),
            "usb_capturing" => ("Checking the USB connections. Keep the USB switch connected to this PC. Do not switch yet; wait for the next instruction.", "正在检查 USB 连接。请保持 USB 切换器连接到本机，暂时不要切换，等待下一步提示。", "Checking USB…", "正在检查 USB…"),
            "usb_retry" => ("USB detection needs another try. Connect the USB switch to this PC and leave it here, then restart detection. Wait for this window’s cue before switching back next time.", "需要重新检测 USB。请先将 USB 切换器连接到本机并保持连接，然后重新检测。下次请等待此窗口的提示后再切回。", "USB is on this PC — retry detection", "USB 已连接本机 — 重新检测"),
            "usb_wait_departure" => ("2. Press the physical USB switch button to the other computer. Leave the monitor input unchanged so this window stays visible. Keep USB away until this window says “USB departure detected — switch back now”.", "2. 按下 USB 切换器实体按钮切到另一台电脑。不要更改显示器输入，保持此窗口可见。请等待此窗口显示“已检测到 USB 切出 — 现在请切回”后再切回。", "Waiting for USB departure…", "等待检测 USB 切出…"),
            "usb_wait_return" => ("USB departure detected — switch back now. Press the same USB switch button to return to this PC, then keep it here while the connections settle.", "已检测到 USB 切出 — 现在请切回。按下同一个 USB 切换器按钮切回本机，然后保持连接，等待检测完成。", "Waiting for return…", "等待切回…"),
            "usb_ready" => ("3. USB switch learned. Make sure the selected monitor shows THIS PC’s picture; use its input menu if needed. Then read the current input. This reads the monitor without switching it.", "3. 已识别 USB 切换器。请确认所选显示器正在显示本机画面（必要时使用显示器输入菜单切换），然后读取当前输入。此操作只读取，不切换画面。", "This PC is visible — read input", "已显示本机 — 读取输入"),
            "reading_input" => ("Reading the selected monitor’s current input…", "正在读取所选显示器的当前输入…", "Reading…", "读取中…"),
            "exchange" => ("4. Run setup on the other computer using the SAME physical monitor, with its own picture visible. Copy this code there, and enter its code below. Codes transfer input choices offline; no account or server is needed.", "4. 在另一台电脑上使用同一台实体显示器，并显示其自身画面时运行同样的设置。将本机代码复制过去，并在下方输入另一台电脑的代码。代码可离线传递输入配置，无需账号或服务器。", "Use other computer’s code", "使用另一台电脑的代码"),
            "ready" => ("5. Save with automation paused. Test switching to each computer and visually check both pictures, then enable automatic switching on both computers. Keep both awake. If the monitor offers a DDC/CI setting, enable it.", "5. 保存后自动切换仍暂停。请测试切换到每台电脑并检查画面，再在两台电脑上启用自动切换。保持两台电脑唤醒。若显示器提供 DDC/CI 开关，请开启。", "Finish setup · automation off", "完成设置 · 自动切换关闭"),
            _ => ("1. Connect both computers to this monitor and one USB switch. Name this environment and choose the monitor. No input numbers or USB hub choices needed. Keep both computers awake. If the monitor offers a DDC/CI setting, enable it.", "1. 将两台电脑连接到此显示器和同一个 USB 切换器。为此环境命名并选择显示器，无需输入源编号或选择 USB 集线器。保持两台电脑唤醒。若显示器提供 DDC/CI 开关，请开启。", "Set up this monitor", "设置此显示器"),
        };
        let mut help = tr(self.zh, en, cn).to_owned();
        if snapshot["setup"]["error"].as_bool() == Some(true) {
            help.push_str("\r\n");
            help.push_str(snapshot["setup"]["message"].as_str().unwrap_or(""));
        }
        if !editing {
            help = tr(self.zh, "Setup saved. Use the switch buttons above to test each computer and visually check both pictures. Then enable automatic switching on both computers. Keep both computers awake. USB status and command success alone do not verify the picture.", "配置已保存。请使用上方切换按钮测试每台电脑，并检查两台电脑的画面，然后在两台电脑上启用自动切换。保持两台电脑唤醒。USB 状态和命令成功本身不代表画面已确认。").into();
        }
        self.set_text(SETUP_HELP, &help);
        self.set_text(COPY_CODE, tr(self.zh, "Copy code", "复制代码"));
        self.set_text(SETUP_NEXT, tr(self.zh, button_en, button_cn));
        self.set_text(SETUP_CODE, snapshot["setup"]["code"].as_str().unwrap_or(""));
        self.set_text(
            ADVANCED,
            if self.advanced {
                tr(self.zh, "Guided setup", "引导设置")
            } else {
                tr(self.zh, "Advanced…", "高级…")
            },
        );
        let guided = !self.advanced && editing;
        for (id, visible) in [
            (SETUP_HELP, !self.advanced),
            (SETUP_NEXT, guided),
            (SETUP_CODE, guided && matches!(phase, "exchange" | "ready")),
            (PEER_CODE, guided && phase == "exchange"),
            (COPY_CODE, guided && matches!(phase, "exchange" | "ready")),
        ] {
            ShowWindow(self.control(id), if visible { SW_SHOW } else { SW_HIDE });
        }
        EnableWindow(
            self.control(SETUP_NEXT),
            (!matches!(
                phase,
                "usb_capturing" | "usb_wait_departure" | "usb_wait_return" | "reading_input"
            )) as i32,
        );
        for id in [DISPLAY, PROFILE_NAME] {
            EnableWindow(
                self.control(id),
                (editing && (self.advanced || snapshot["setup"].is_null())) as i32,
            );
        }
    }
    unsafe fn setup_next(&mut self) {
        let action = match self.snapshot["setup"]["phase"].as_str() {
            Some("display" | "usb_retry") => json!({"type":"setup_usb_start"}),
            Some("usb_ready") => json!({"type":"setup_read_local"}),
            Some("exchange") => json!({"type":"setup_peer_code","code":self.text(PEER_CODE)}),
            Some("ready") => json!({"type":"setup_finish"}),
            _ => {
                json!({"type":"setup_begin","display_id":self.selected(DISPLAY, &self.displays).map(|d|d["id"].clone()),"name":self.text(PROFILE_NAME)})
            }
        };
        if !self.send(action) {
            self.pending_setup_error = self.action_error.clone();
        }
    }
    unsafe fn labels(&self) {
        for (id, en, cn) in [
            (LOCAL, "Switch to this PC", "切换到本机"),
            (REMOTE, "Switch to other host", "切换到另一台主机"),
            (AUTO, "Automatic switching", "自动切换"),
            (REFRESH, "Refresh devices", "刷新设备"),
            (EDIT, "Edit enrollment", "编辑设备绑定"),
            (SAVE, "Save setup", "保存环境"),
            (NEW_SETUP, "New setup", "新增环境"),
            (
                207,
                "Setup name (for example, Home or Work)",
                "环境名称（例如：家或公司）",
            ),
            (
                201,
                "Enrolled display (choose explicitly)",
                "绑定显示器（请明确选择）",
            ),
            (
                202,
                "USB branch 1 (advanced topology)",
                "共享 USB 集线器 1（拓扑标识）",
            ),
            (203, "USB branch 2 (optional)", "USB 分支 2（可选）"),
            (204, "This PC input (1–255)", "本机输入源（1–255）"),
            (205, "Other host input (1–255)", "另一台主机输入源（1–255）"),
            (206, "Configuration file", "配置文件"),
        ] {
            self.set_text(id, tr(self.zh, en, cn));
        }
        SendMessageW(self.control(LANGUAGE), CB_SETCURSEL, self.zh as usize, 0);
    }
    unsafe fn save(&mut self) {
        let (Some(display), Some(hub1)) = (
            self.selected(DISPLAY, &self.displays),
            self.selected(HUB1, &self.hubs),
        ) else {
            self.action_error = Some(
                tr(
                    self.zh,
                    "Choose a display and at least one USB branch.",
                    "请选择显示器及至少一个 USB 分支。",
                )
                .into(),
            );
            return;
        };
        let inputs = parse_inputs(&self.text(INPUT1), &self.text(INPUT2));
        let Some((local, remote)) = inputs else {
            self.action_error = Some(
                tr(
                    self.zh,
                    "Inputs must be different numbers from 1 to 255.",
                    "两个输入源必须是 1 到 255 之间的不同整数。",
                )
                .into(),
            );
            return;
        };
        let hub2 = self.selected(HUB2, &self.hubs);
        if hub2.as_ref().is_some_and(|hub| hub1["id"] == hub["id"]) {
            self.action_error = Some(
                tr(
                    self.zh,
                    "Choose two different hubs.",
                    "请选择两个不同的集线器。",
                )
                .into(),
            );
            return;
        }
        let mut hubs = vec![hub1];
        if let Some(hub) = hub2 {
            hubs.push(hub);
        }
        self.send(json!({"type":"save", "profile_name":self.text(PROFILE_NAME), "config": {"version":1, "platform":"windows", "display_id":display["id"], "hubs":hubs, "local_input":local,"remote_input":remote,"automatic":self.snapshot["config"]["automatic"].as_bool().unwrap_or(false),"language":if self.zh {"zh-Hans"} else {"en"}}}));
    }
    unsafe fn show(&self) {
        ShowWindow(self.window, SW_SHOW);
        SetForegroundWindow(self.window);
    }
    unsafe fn command(&mut self, id: usize, notification: u16) {
        if self.updating {
            return;
        }
        match id {
            OPEN => self.show(),
            SETUP_NEXT => self.setup_next(),
            COPY_CODE => {
                SendMessageW(self.control(SETUP_CODE), EM_SETSEL, 0, -1);
                SendMessageW(self.control(SETUP_CODE), WM_COPY, 0, 0);
            }
            ADVANCED => {
                if !self.advanced && !self.snapshot["setup"].is_null() {
                    self.send(json!({"type":"setup_cancel"}));
                    self.send(json!({"type":"edit","enabled":true}));
                }
                self.advanced = !self.advanced;
                self.inventory = Value::Null;
            }
            LOCAL | REMOTE => {
                self.send(
                    json!({"type":"manual","target":if id == LOCAL {"local"} else {"remote"}}),
                );
            }
            AUTO => {
                self.send(json!({"type":"automatic","enabled":self.checked()}));
            }
            REFRESH => {
                self.send(json!({"type":"refresh"}));
            }
            EDIT => {
                if !self.snapshot["setup"].is_null() {
                    self.send(json!({"type":"setup_cancel"}));
                }
                self.send(json!({"type":"edit","enabled":self.snapshot["editing"].as_bool() != Some(true)}));
            }
            SAVE => self.save(),
            NEW_SETUP => {
                self.advanced = false;
                if self.send(json!({"type":"new"})) {
                    // A new environment must never inherit an old device selection.
                    self.initialized = false;
                    self.displays.clear();
                    self.hubs.clear();
                    for id in [DISPLAY, HUB1, HUB2] {
                        SendMessageW(self.control(id), CB_SETCURSEL, usize::MAX, 0);
                    }
                }
            }
            LANGUAGE if notification == CBN_SELCHANGE as u16 => {
                let chinese = SendMessageW(self.control(LANGUAGE), CB_GETCURSEL, 0, 0) == 1;
                self.send(json!({"type":"language","value":if chinese {"zh-Hans"} else {"en"}}));
            }
            QUIT => {
                self.send(json!({"type":"quit"}));
                DestroyWindow(self.window);
                return;
            }
            _ => return,
        }
        self.tick();
    }
    unsafe fn make_menu(&self) -> HMENU {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return menu;
        }
        let configured =
            !self.snapshot["config"].is_null() && self.snapshot["editing"].as_bool() != Some(true);
        let automatic = self.snapshot["config"]["automatic"].as_bool() == Some(true);
        let ready = self.snapshot["lifecycle_ok"].as_bool() == Some(true)
            && self.snapshot["sleeping"].as_bool() != Some(true);
        for (id, label, enabled) in [
            (OPEN, tr(self.zh, "Open PortKey", "打开 PortKey"), true),
            (
                LOCAL,
                tr(self.zh, "Switch to this PC", "切换到本机"),
                configured && ready,
            ),
            (
                REMOTE,
                tr(self.zh, "Switch to other host", "切换到另一台主机"),
                configured && ready,
            ),
            (
                AUTO,
                if automatic {
                    tr(self.zh, "Pause automatic switching", "暂停自动切换")
                } else {
                    tr(self.zh, "Enable automatic switching", "启用自动切换")
                },
                configured && (ready || automatic),
            ),
            (QUIT, tr(self.zh, "Quit PortKey", "退出 PortKey"), true),
        ] {
            AppendMenuW(
                menu,
                MF_STRING | if enabled { MF_ENABLED } else { MF_GRAYED },
                id,
                wide(label).as_ptr(),
            );
        }
        menu
    }
}
// The backend appends the exact interface identity for diagnostics. Keep it out
// of the guided display name without changing selection or identity matching.
fn friendly_display_name(value: &Value) -> &str {
    let label = value["label"].as_str().unwrap_or("");
    let identity = value["id"].as_str().unwrap_or("");
    if label == identity {
        return "";
    }
    label
        .strip_suffix(identity)
        .and_then(|s| s.strip_suffix(" · "))
        .unwrap_or(label)
        .trim()
}

// Discovery is not the source of truth for an explicitly selected identity.
// Keep saved and draft choices when hardware temporarily disappears; live rows
// take precedence on reconnect. This does not authorize any hardware write.
fn merge_choices(discovered: &Value, retained: &[Option<&Value>]) -> Vec<Value> {
    let mut choices = discovered.as_array().cloned().unwrap_or_default();
    for value in retained.iter().flatten() {
        if let Some(id) = value["id"].as_str().filter(|id| !id.is_empty()) {
            if !choices
                .iter()
                .any(|choice| choice["id"].as_str() == Some(id))
            {
                choices.push((*value).clone());
            }
        }
    }
    choices
}

fn should_seed_form(initialized: bool, snapshot: &Value, previous: &Value) -> bool {
    !initialized
        || (snapshot["editing"].as_bool() != Some(true)
            && (snapshot["config"] != previous["config"]
                || previous["editing"].as_bool() == Some(true)))
}

fn parse_inputs(local: &str, remote: &str) -> Option<(u8, u8)> {
    let local = local.trim().parse::<u8>().ok()?;
    let remote = remote.trim().parse::<u8>().ok()?;
    (local != 0 && remote != 0 && local != remote).then_some((local, remote))
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // Win32 can synchronously re-enter this callback while controls are updated.
    // RefCell prevents a second mutable UI borrow; menu tracking borrows no state,
    // so timer ticks continue throughout its nested message loop.
    if message == WM_DESTROY {
        KillTimer(hwnd, 1);
        let icon = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..std::mem::zeroed()
        };
        Shell_NotifyIconW(NIM_DELETE, &icon);
        PostQuitMessage(0);
        return 0;
    }
    if message == WM_CLOSE {
        ShowWindow(hwnd, SW_HIDE);
        return 0;
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<Ui>;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, message, w, l);
    }
    let state = &*ptr;
    if message == TRAY && matches!(l as u32, WM_RBUTTONUP | WM_CONTEXTMENU) {
        let menu = match state.try_borrow() {
            Ok(ui) => ui.make_menu(),
            Err(_) => return 0,
        };
        if menu.is_null() {
            return 0;
        }
        let mut point = std::mem::zeroed::<POINT>();
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as usize;
        DestroyMenu(menu);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        if let Ok(mut ui) = state.try_borrow_mut() {
            if chosen == AUTO {
                let automatic = ui.snapshot["config"]["automatic"].as_bool() == Some(true);
                ui.send(json!({"type":"automatic", "enabled":!automatic}));
                ui.tick();
            } else if chosen != 0 {
                ui.command(chosen, 0);
            }
        }
        return 0;
    }
    if let Ok(mut ui) = state.try_borrow_mut() {
        if message == ui.taskbar_created {
            Shell_NotifyIconW(NIM_ADD, &ui.tray);
            return 0;
        }
        match message {
            WM_DEVICECHANGE
                if w == DBT_DEVNODES_CHANGED as usize || is_usb_interface_hint(w as u32, l) =>
            {
                // Device notifications are hints; only worker inventory establishes ownership.
                crate::timing::event(
                    "device_change",
                    if w == DBT_DEVNODES_CHANGED as usize {
                        "native"
                    } else {
                        "usb_interface"
                    },
                    None,
                    None,
                );
                ui.controller.action(json!({"type":"usb_refresh"}));
                return 1;
            }
            WM_DISPLAYCHANGE => {
                ui.controller.action(json!({"type":"refresh"}));
                return 0;
            }
            WM_TIMER => {
                ui.tick();
                return 0;
            }
            WM_COMMAND => {
                ui.command(w & 0xffff, ((w >> 16) & 0xffff) as u16);
                let error = ui.pending_setup_error.take();
                let title = tr(ui.zh, "PortKey setup", "PortKey 设置");
                // The dialog pumps timer messages; release the borrow so policy keeps ticking.
                drop(ui);
                if let Some(error) = error {
                    MessageBoxW(
                        hwnd,
                        wide(&error).as_ptr(),
                        wide(title).as_ptr(),
                        MB_OK | MB_ICONWARNING,
                    );
                }
                return 0;
            }
            TRAY => {
                if matches!(l as u32, WM_LBUTTONUP | WM_LBUTTONDBLCLK) {
                    ui.show();
                }
                return 0;
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, message, w, l)
}

unsafe fn add(
    parent: HWND,
    class: &str,
    title: &str,
    id: usize,
    style: u32,
    rect: [i32; 4],
) -> Result<HWND, String> {
    let dpi = system_dpi();
    let rect = rect.map(|v| v * dpi / 96);
    let hwnd = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(title).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        rect[0],
        rect[1],
        rect[2],
        rect[3],
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    if hwnd.is_null() {
        return Err(format!(
            "Cannot create native control: {}",
            std::io::Error::last_os_error()
        ));
    }
    SendMessageW(
        hwnd,
        WM_SETFONT,
        GetPropW(parent, wide("PortKeyFont").as_ptr()) as usize,
        1,
    );
    Ok(hwnd)
}

pub fn run() -> Result<(), String> {
    // UI state is owned by this thread for the entire Win32 message loop.
    unsafe { run_native() }
}
unsafe fn system_dpi() -> i32 {
    let dc = GetDC(null_mut());
    if dc.is_null() {
        return 96;
    }
    let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
    ReleaseDC(null_mut(), dc);
    dpi.max(96)
}

unsafe fn run_native() -> Result<(), String> {
    let dpi = system_dpi();
    let class = wide("PortKeyNativeSettings");
    let instance = GetModuleHandleW(null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
        ..std::mem::zeroed()
    };
    if RegisterClassW(&wc) == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let hwnd = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class.as_ptr(),
        wide("PortKey").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        760 * dpi / 96,
        710 * dpi / 96,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let mut logfont = LOGFONTW {
        lfHeight: -(9 * dpi / 72),
        ..std::mem::zeroed()
    };
    let face = wide("Segoe UI");
    logfont.lfFaceName[..face.len()].copy_from_slice(&face);
    let font = CreateFontIndirectW(&logfont);
    if font.is_null() || SetPropW(hwnd, wide("PortKeyFont").as_ptr(), font) == 0 {
        if !font.is_null() {
            DeleteObject(font);
        }
        DestroyWindow(hwnd);
        return Err("Cannot create the native interface font".into());
    }
    let result = setup(hwnd);
    if result.is_err() {
        DestroyWindow(hwnd);
    }
    DeleteObject(font);
    UnregisterClassW(class.as_ptr(), instance);
    result
}
unsafe fn setup(hwnd: HWND) -> Result<(), String> {
    add(
        hwnd,
        "BUTTON",
        "",
        AUTO,
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        [20, 16, 280, 28],
    )?;
    add(
        hwnd,
        "COMBOBOX",
        "",
        LANGUAGE,
        WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
        [510, 16, 210, 120],
    )?;
    for text in ["English", "简体中文"] {
        SendMessageW(
            GetDlgItem(hwnd, LANGUAGE as i32),
            CB_ADDSTRING,
            0,
            wide(text).as_ptr() as isize,
        );
    }
    add(hwnd, "BUTTON", "", LOCAL, WS_TABSTOP, [20, 54, 220, 30])?;
    add(hwnd, "BUTTON", "", REMOTE, WS_TABSTOP, [250, 54, 220, 30])?;
    add(
        hwnd,
        "EDIT",
        "",
        STATUS,
        WS_TABSTOP | WS_VSCROLL | ES_MULTILINE as u32 | ES_READONLY as u32,
        [20, 96, 700, 64],
    )?;
    add(hwnd, "STATIC", "", 207, 0, [20, 168, 470, 22])?;
    add(
        hwnd,
        "EDIT",
        "",
        PROFILE_NAME,
        WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
        [20, 192, 470, 28],
    )?;
    SendMessageW(
        GetDlgItem(hwnd, PROFILE_NAME as i32),
        EM_SETLIMITTEXT,
        80,
        0,
    );
    add(
        hwnd,
        "BUTTON",
        "",
        NEW_SETUP,
        WS_TABSTOP,
        [510, 192, 210, 28],
    )?;
    for (label, id, y) in [(201, DISPLAY, 234), (202, HUB1, 300), (203, HUB2, 366)] {
        add(hwnd, "STATIC", "", label, 0, [20, y, 700, 22])?;
        add(
            hwnd,
            "COMBOBOX",
            "",
            id,
            WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
            [20, y + 24, 700, 250],
        )?;
        SendMessageW(GetDlgItem(hwnd, id as i32), CB_SETDROPPEDWIDTH, 1000, 0);
    }
    add(hwnd, "STATIC", "", 204, 0, [20, 436, 300, 22])?;
    add(
        hwnd,
        "EDIT",
        "",
        INPUT1,
        WS_TABSTOP | WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
        [20, 462, 300, 28],
    )?;
    add(hwnd, "STATIC", "", 205, 0, [380, 436, 340, 22])?;
    add(
        hwnd,
        "EDIT",
        "",
        INPUT2,
        WS_TABSTOP | WS_BORDER | ES_NUMBER as u32 | ES_AUTOHSCROLL as u32,
        [380, 462, 340, 28],
    )?;
    for id in [INPUT1, INPUT2] {
        SendMessageW(GetDlgItem(hwnd, id as i32), EM_SETLIMITTEXT, 3, 0);
    }
    for (id, x) in [(REFRESH, 20), (EDIT, 250), (SAVE, 480)] {
        add(hwnd, "BUTTON", "", id, WS_TABSTOP, [x, 512, 220, 32])?;
    }
    add(hwnd, "STATIC", "", 206, 0, [20, 564, 700, 20])?;
    add(
        hwnd,
        "EDIT",
        "",
        PATH,
        ES_READONLY as u32 | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
        [20, 588, 700, 28],
    )?;
    add(hwnd, "STATIC", "", SETUP_HELP, 0, [20, 300, 700, 104])?;
    add(
        hwnd,
        "EDIT",
        "",
        SETUP_CODE,
        WS_TABSTOP | ES_READONLY as u32 | ES_AUTOHSCROLL as u32 | WS_BORDER,
        [20, 410, 470, 26],
    )?;
    add(
        hwnd,
        "BUTTON",
        "",
        COPY_CODE,
        WS_TABSTOP,
        [510, 410, 210, 26],
    )?;
    add(
        hwnd,
        "EDIT",
        "",
        PEER_CODE,
        WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
        [20, 444, 700, 26],
    )?;
    SendMessageW(
        GetDlgItem(hwnd, PEER_CODE as i32),
        EM_SETCUEBANNER,
        0,
        wide("Other computer’s setup code / 另一台电脑的配置代码").as_ptr() as isize,
    );
    SendMessageW(GetDlgItem(hwnd, PEER_CODE as i32), EM_SETLIMITTEXT, 64, 0);
    add(
        hwnd,
        "BUTTON",
        "",
        SETUP_NEXT,
        WS_TABSTOP,
        [20, 478, 700, 28],
    )?;
    add(hwnd, "BUTTON", "", ADVANCED, WS_TABSTOP, [20, 624, 220, 28])?;
    let mut tray = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
        ..std::mem::zeroed()
    };
    let tip = wide("PortKey");
    tray.szTip[..tip.len()].copy_from_slice(&tip);
    let ui = Box::new(RefCell::new(Ui {
        controller: Controller::new(),
        window: hwnd,
        snapshot: Value::Null,
        inventory: Value::Null,
        displays: Vec::new(),
        hubs: Vec::new(),
        initialized: false,
        zh: false,
        updating: false,
        advanced: false,
        tray,
        taskbar_created: RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()),
        action_error: None,
        pending_setup_error: None,
    }));
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&*ui as *const RefCell<Ui>) as isize);
    if Shell_NotifyIconW(NIM_ADD, &ui.borrow().tray) == 0 {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return Err("Cannot create the PortKey notification-area icon".into());
    }
    let _usb_notifications = UsbNotifications::register(hwnd);
    ui.borrow_mut().tick();
    if SetTimer(hwnd, 1, 200, None) == 0 {
        Shell_NotifyIconW(NIM_DELETE, &ui.borrow().tray);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        return Err("Cannot start the PortKey native UI timer".into());
    }
    ui.borrow().show();
    let mut message = std::mem::zeroed::<MSG>();
    let result = loop {
        let result = GetMessageW(&mut message, null_mut(), 0, 0);
        if result == -1 {
            break Err(std::io::Error::last_os_error().to_string());
        }
        if result == 0 {
            break Ok(());
        }
        if IsDialogMessageW(hwnd, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    };
    ui.borrow_mut().controller.action(json!({"type":"quit"}));
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{friendly_display_name, merge_choices, parse_inputs, should_seed_form};
    use serde_json::{json, Value};

    #[test]
    fn usb_notifications_only_wake_for_relevant_interfaces() {
        use super::*;
        for class in [USB_DEVICE_INTERFACE, USB_HUB_INTERFACE] {
            for event in [DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE] {
                assert!(usb_interface_hint(event, DBT_DEVTYP_DEVICEINTERFACE, class));
            }
            assert!(!usb_interface_hint(
                DBT_DEVICEQUERYREMOVE,
                DBT_DEVTYP_DEVICEINTERFACE,
                class
            ));
            assert!(!usb_interface_hint(
                DBT_DEVICEARRIVAL,
                DBT_DEVTYP_VOLUME,
                class
            ));
        }
        assert!(!usb_interface_hint(
            DBT_DEVICEARRIVAL,
            DBT_DEVTYP_DEVICEINTERFACE,
            windows_sys::core::GUID::from_u128(0)
        ));
        unsafe {
            assert!(!is_usb_interface_hint(DBT_DEVICEARRIVAL, 0));
            let short = DEV_BROADCAST_HDR {
                dbch_size: size_of::<DEV_BROADCAST_HDR>() as u32,
                dbch_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
                dbch_reserved: 0,
            };
            assert!(!is_usb_interface_hint(
                DBT_DEVICEARRIVAL,
                &short as *const _ as LPARAM
            ));
        }
    }
    #[test]
    fn guided_display_names_hide_only_the_exact_diagnostic_identity() {
        let monitor = json!({"id":"interface-a", "label":"Philips · interface-a"});
        assert_eq!(friendly_display_name(&monitor), "Philips");
        assert_eq!(monitor["id"], "interface-a");
        assert_eq!(
            friendly_display_name(&json!({"id":"interface-b", "label":"Philips · interface-a"})),
            "Philips · interface-a"
        );
        assert_eq!(
            friendly_display_name(&json!({"id":"interface-a", "label":"interface-a"})),
            ""
        );
        assert_eq!(
            friendly_display_name(&json!({"id":"interface-a", "label":"Philips"})),
            "Philips"
        );
    }

    #[test]
    fn profile_changes_only_replace_forms_outside_editing() {
        let home = json!({"editing":false,"config":{"display_id":"home"}});
        let work = json!({"editing":false,"config":{"display_id":"work"}});
        let draft = json!({"editing":true,"config":null});
        assert!(should_seed_form(true, &work, &home));
        assert!(!should_seed_form(true, &draft, &home));
        assert!(!should_seed_form(true, &draft, &draft));
        // New setup explicitly invalidates initialization before its first tick.
        assert!(should_seed_form(false, &draft, &home));
        assert!(should_seed_form(true, &home, &draft));
        assert!(!should_seed_form(true, &home, &home));
    }

    #[test]
    fn device_refresh_preserves_explicit_offline_drafts_and_saved_choices() {
        let saved = json!({"id":"saved-topology","label":"Saved hub"});
        let draft = json!({"id":"draft-topology","label":"Shared hub"});
        let disconnected = merge_choices(&Value::Null, &[Some(&draft), Some(&saved), None]);
        assert_eq!(disconnected, vec![draft.clone(), saved.clone()]);
        let live = json!({"id":"draft-topology","label":"Reconnected shared hub"});
        let reconnected = merge_choices(
            &json!([live.clone()]),
            &[Some(&disconnected[0]), Some(&saved)],
        );
        assert_eq!(reconnected, vec![live, saved]);
        assert!(merge_choices(&Value::Null, &[None, Some(&Value::Null)]).is_empty());
    }
    #[test]
    fn input_values_fail_closed_without_defaults() {
        for (a, b) in [
            ("", "15"),
            ("0", "15"),
            ("256", "15"),
            ("17", "17"),
            ("-1", "15"),
            ("0x11", "15"),
        ] {
            assert_eq!(parse_inputs(a, b), None);
        }
        assert_eq!(parse_inputs("17", "15"), Some((17, 15)));
    }
}
