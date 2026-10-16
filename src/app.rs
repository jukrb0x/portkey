use crate::core::{Config, Ownership, Policy, Target};
use crate::platform::{self, Inventory, WriteOutcome};
use crate::runtime::{Dispatch, Latest};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime},
};

struct Job {
    config: Config,
    target: Target,
    expected: Option<Ownership>,
    queued: SystemTime,
    epoch: u64,
}
#[derive(Default)]
struct Shared {
    jobs: Latest<Job>,
    inventory: Option<(u64, Result<Inventory, String>)>,
    completion: Option<(u64, Result<WriteOutcome, String>)>,
    stop: bool,
    refresh: bool,
    resumed: bool,
}
struct Worker {
    shared: Arc<(Mutex<Shared>, Condvar)>,
}
impl Worker {
    fn new() -> Self {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let thread_shared = shared.clone();
        std::thread::spawn(move || {
            let (lock, wake) = &*thread_shared;
            let mut last_inventory = Instant::now() - Duration::from_secs(10);
            let mut last_cycle = SystemTime::now();
            loop {
                let gap = last_cycle.elapsed().unwrap_or(Duration::MAX);
                last_cycle = SystemTime::now();
                let mut state = lock.lock().unwrap();
                if state.stop {
                    break;
                }
                if gap > Duration::from_secs(5) {
                    state.jobs.invalidate();
                    state.resumed = true;
                }
                let inventory_generation = state.jobs.generation();
                let job = state.jobs.take();
                let refresh = state.refresh || last_inventory.elapsed() >= Duration::from_secs(2);
                state.refresh = false;
                if job.is_none() && !refresh {
                    drop(
                        wake.wait_timeout(state, Duration::from_millis(250))
                            .unwrap(),
                    );
                    continue;
                }
                drop(state);
                // Every write rechecks discovery; a failed read is never absence.
                let inventory_epoch = crate::lifecycle::snapshot().0;
                let wall_started = SystemTime::now();
                let monotonic_started = Instant::now();
                let inventory = platform::inventory();
                let suspended = wall_started
                    .elapsed()
                    .unwrap_or(Duration::MAX)
                    .saturating_sub(monotonic_started.elapsed())
                    > Duration::from_secs(2);
                if suspended {
                    let mut s = lock.lock().unwrap();
                    s.jobs.invalidate();
                    s.resumed = true;
                }
                last_inventory = Instant::now();
                let result = job.as_ref().map(|(generation, job)| {
                    if job.queued.elapsed().unwrap_or(Duration::MAX) > Duration::from_secs(30)
                        || !lock.lock().unwrap().jobs.current(*generation)
                    {
                        return Err(
                            "Request expired or was cancelled; retry after discovery.".into()
                        );
                    }
                    let inventory = inventory.as_ref().map_err(Clone::clone)?;
                    if let Some(expected) = job.expected {
                        if job.config.observe(&inventory.hubs) != expected {
                            return Err(
                                "USB ownership changed; automatic request cancelled.".into()
                            );
                        }
                    }
                    let mut outcome = Err("Switch not attempted".into());
                    for attempt in 0..3 {
                        let (epoch, sleeping) = crate::lifecycle::snapshot();
                        if sleeping
                            || epoch != job.epoch
                            || !lock.lock().unwrap().jobs.current(*generation)
                        {
                            return Err("Request cancelled by lifecycle or newer action.".into());
                        }
                        if attempt > 0 {
                            let state = lock.lock().unwrap();
                            let (state, _) = wake
                                .wait_timeout(state, Duration::from_millis(750 * attempt))
                                .unwrap();
                            if state.stop || !state.jobs.current(*generation) {
                                return Err("Retry cancelled.".into());
                            }
                            drop(state);
                            let (epoch, sleeping) = crate::lifecycle::snapshot();
                            if sleeping || epoch != job.epoch {
                                return Err("Retry cancelled by sleep/wake.".into());
                            }
                            let fresh = platform::inventory()?;
                            if job
                                .expected
                                .is_some_and(|expected| job.config.observe(&fresh.hubs) != expected)
                            {
                                return Err("USB ownership changed; retry cancelled.".into());
                            }
                            let (epoch, sleeping) = crate::lifecycle::snapshot();
                            if sleeping
                                || epoch != job.epoch
                                || !lock.lock().unwrap().jobs.current(*generation)
                            {
                                return Err("Request cancelled before retry.".into());
                            }
                        }
                        let wall = SystemTime::now();
                        let monotonic = Instant::now();
                        outcome = platform::set_input(&job.config, job.config.input(job.target));
                        if wall
                            .elapsed()
                            .unwrap_or(Duration::MAX)
                            .saturating_sub(monotonic.elapsed())
                            > Duration::from_secs(2)
                        {
                            let mut state = lock.lock().unwrap();
                            state.jobs.invalidate();
                            state.resumed = true;
                            return Err(
                                "Sleep interrupted the operation; outcome is unknown.".into()
                            );
                        }
                        if outcome.is_ok() {
                            break;
                        }
                    }
                    outcome
                });
                let mut state = lock.lock().unwrap();
                if state.jobs.current(inventory_generation) {
                    state.inventory = Some((inventory_epoch, inventory));
                }
                last_cycle = SystemTime::now();
                if let (Some((generation, _)), Some(result)) = (job, result) {
                    if state.jobs.current(generation) {
                        state.completion = Some((generation, result));
                    }
                }
            }
        });
        Self { shared }
    }
    fn invalidate(&self) {
        let mut s = self.shared.0.lock().unwrap();
        s.jobs.invalidate();
        s.completion = None;
        s.inventory = None;
    }
    fn submit(&self, config: Config, target: Target, expected: Option<Ownership>) {
        self.shared.0.lock().unwrap().jobs.replace(Job {
            config,
            target,
            expected,
            queued: SystemTime::now(),
            epoch: crate::lifecycle::snapshot().0,
        });
        self.shared.1.notify_one();
    }
    fn refresh(&self) {
        self.shared.0.lock().unwrap().refresh = true;
        self.shared.1.notify_one();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut s = self.shared.0.lock().unwrap();
        s.stop = true;
        s.jobs.invalidate();
        self.shared.1.notify_one();
    }
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([620.0, 720.0])
            .with_min_inner_size([500.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PortKey",
        options,
        Box::new(|cc| {
            install_fonts(&cc.egui_ctx);
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(App::new(cc.egui_ctx.clone())))
        }),
    )
}
fn install_fonts(ctx: &egui::Context) {
    let paths = [
        PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
        PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
        PathBuf::from("/System/Library/Fonts/STHeiti Medium.ttc"),
        PathBuf::from(std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into()))
            .join("Fonts/msyh.ttc"),
    ];
    for path in paths {
        if let Ok(bytes) = std::fs::read(path) {
            let mut fonts = egui::FontDefinitions::default();
            fonts
                .font_data
                .insert("cjk".into(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .get_mut(&egui::FontFamily::Proportional)
                .unwrap()
                .push("cjk".into());
            ctx.set_fonts(fonts);
            break;
        }
    }
}
struct App {
    worker: Worker,
    policy: Policy,
    config: Option<Config>,
    draft: Config,
    inventory: Option<Inventory>,
    status: String,
    error: bool,
    editing: bool,
    started: Instant,
    last_frame: SystemTime,
    dispatch: Dispatch,
    tray: Option<crate::tray::Tray>,
    quitting: bool,
    lifecycle_ok: bool,
    lifecycle_epoch: u64,
    #[cfg(target_os = "macos")]
    legacy_path: String,
}
impl App {
    fn new(context: egui::Context) -> Self {
        let loaded = Config::path().and_then(|p| Config::load(&p));
        let (config, mut status, mut error) = match loaded {
            Ok(c) => (c, String::new(), false),
            Err(e) => (None, e, true),
        };
        let lifecycle_ok = match crate::lifecycle::start() {
            Ok(()) => true,
            Err(e) => {
                status = e;
                error = true;
                false
            }
        };
        let tray = match crate::tray::Tray::new(context) {
            Ok(tray) => Some(tray),
            Err(e) => {
                status = format!(
                    "{status} Tray unavailable: {e}. Closing the window will quit PortKey."
                );
                error = true;
                None
            }
        };
        let language = prefs_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .filter(|s| s == "zh-Hans")
            .unwrap_or_else(|| "en".into());
        let draft = config.clone().unwrap_or(Config {
            version: 1,
            platform: std::env::consts::OS.into(),
            display_id: String::new(),
            local_input: 15,
            remote_input: 17,
            hubs: vec![],
            automatic: false,
            language,
        });
        let mut policy = Policy::new();
        policy.set_automatic(draft.automatic, 0);
        let editing = config.is_none();
        Self {
            worker: Worker::new(),
            policy,
            config,
            draft,
            inventory: None,
            status,
            error,
            editing,
            started: Instant::now(),
            last_frame: SystemTime::now(),
            dispatch: Dispatch::default(),
            tray,
            quitting: false,
            lifecycle_ok,
            lifecycle_epoch: crate::lifecycle::snapshot().0,
            #[cfg(target_os = "macos")]
            legacy_path: dirs::home_dir()
                .unwrap_or_default()
                .join("Library/Application Support/PortKey/config.json")
                .display()
                .to_string(),
        }
    }
    fn text<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.draft.language == "zh-Hans" {
            zh
        } else {
            en
        }
    }
    fn reset(&mut self) {
        self.worker.invalidate();
        self.policy.sleep();
        self.policy.wake(self.started.elapsed().as_millis() as u64);
    }
    fn save(&mut self) {
        match Config::path().and_then(|p| self.draft.save(&p)) {
            Ok(()) => {
                self.reset();
                self.config = Some(self.draft.clone());
                self.policy.set_automatic(
                    self.draft.automatic,
                    self.started.elapsed().as_millis() as u64,
                );
                self.editing = false;
                self.status = self
                    .text(
                        "Settings saved. Waiting for fresh USB evidence.",
                        "设置已保存，正在等待新的 USB 状态。",
                    )
                    .into();
                self.error = false;
                self.worker.refresh();
            }
            Err(e) => {
                self.status = e;
                self.error = true;
            }
        }
    }
    fn pause_automation(&mut self) {
        {
            let mut shared = self.worker.shared.0.lock().unwrap();
            crate::runtime::pause(
                &mut self.policy,
                &mut shared.jobs,
                self.started.elapsed().as_millis() as u64,
            );
            shared.completion = None;
        }
        self.draft.automatic = false;
        if let Some(config) = &mut self.config {
            config.automatic = false;
            let result = Config::path().and_then(|path| config.save(&path));
            if let Err(error) = result {
                self.error = true;
                self.status = format!("{} {error}", self.text("Paused for this session; could not save. Check settings before restarting.", "本次运行已暂停，但保存失败。重启前请检查设置。"));
                return;
            }
        }
        self.status = self
            .text("Automatic switching paused.", "自动切换已暂停。")
            .into();
        self.error = false;
    }
    fn manual(&mut self, target: Target) {
        if self.editing || !self.lifecycle_ok || crate::lifecycle::snapshot().1 {
            return;
        }
        if let Some(config) = self.config.clone() {
            self.worker.invalidate();
            self.policy.manual(target);
            if let Some(request) = self.policy.request() {
                self.dispatch.accept(request.generation);
            }
            self.worker.submit(config, target, None);
            self.status = self
                .text("Switch request queued…", "切换请求已排队…")
                .into();
            self.error = false;
        }
    }
}
fn prefs_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("portkey/language"))
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.tray.is_some() && !self.quitting && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        let actions = if let Some(tray) = &self.tray {
            tray.update(
                self.draft.language == "zh-Hans",
                self.config.is_some()
                    && !self.editing
                    && self.lifecycle_ok
                    && !crate::lifecycle::snapshot().1,
            );
            tray.actions()
        } else {
            vec![]
        };
        for action in actions {
            match action {
                crate::tray::Action::Open => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                crate::tray::Action::Pause => self.pause_automation(),
                crate::tray::Action::Local => self.manual(Target::Local),
                crate::tray::Action::Remote => self.manual(Target::Remote),
                crate::tray::Action::Quit => {
                    self.quitting = true;
                    self.policy.sleep();
                    self.worker.invalidate();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    return;
                }
            }
        }
        let now = self.started.elapsed().as_millis() as u64;
        let (epoch, sleeping) = crate::lifecycle::snapshot();
        if epoch != self.lifecycle_epoch {
            self.lifecycle_epoch = epoch;
            self.reset();
            self.inventory = None;
            self.worker.refresh();
        }
        if self.last_frame.elapsed().unwrap_or(Duration::MAX) > Duration::from_secs(5) {
            self.reset();
            self.worker.refresh();
        }
        self.last_frame = SystemTime::now();
        let (inventory, completion, resumed) = {
            let mut s = self.worker.shared.0.lock().unwrap();
            let resumed = std::mem::take(&mut s.resumed);
            (s.inventory.take(), s.completion.take(), resumed)
        };
        if resumed {
            self.reset();
        }
        if let Some((_, result)) =
            inventory.filter(|(inventory_epoch, _)| *inventory_epoch == epoch && !sleeping)
        {
            match result {
                Ok(inventory) => {
                    if !self.editing && !sleeping && self.lifecycle_ok {
                        if let Some(c) = &self.config {
                            let ownership = c.observe(&inventory.hubs);
                            if ownership != self.policy.ownership() {
                                self.worker.invalidate();
                            }
                            let was_present = self
                                .inventory
                                .as_ref()
                                .is_some_and(|i| i.displays.iter().any(|d| d.id == c.display_id));
                            let present = inventory.displays.iter().any(|d| d.id == c.display_id);
                            self.policy.observe(ownership, now);
                            if present && !was_present {
                                self.policy.monitor_reconnected(now);
                            }
                        }
                    }
                    self.inventory = Some(inventory);
                }
                Err(e) => {
                    self.policy.observe(Ownership::Unknown, now);
                    self.worker.invalidate();
                    self.status = e;
                    self.error = true;
                }
            }
        }
        if let Some((generation, result)) = completion.filter(|(generation, _)| {
            self.worker
                .shared
                .0
                .lock()
                .unwrap()
                .jobs
                .current(*generation)
        }) {
            let _ = generation;
            match result {
                Ok(WriteOutcome::Confirmed) => {
                    self.status = self
                        .text(
                            "Monitor reported the requested input. Confirm the picture on screen.",
                            "显示器已回报目标输入，请观察画面确认。",
                        )
                        .into();
                    self.error = false;
                }
                Ok(WriteOutcome::SentUnverified(detail)) => {
                    self.status = format!(
                        "{} {detail}",
                        self.text(
                            "Command sent; visual switching is unverified.",
                            "命令已发送，尚未验证实际画面切换。"
                        )
                    );
                    self.error = false;
                }
                Err(e) => {
                    self.status = e;
                    self.error = true;
                }
            }
        }
        if !self.editing && !sleeping && self.lifecycle_ok {
            if let Some(request) = self.policy.tick(now) {
                if self.dispatch.accept(request.generation) {
                    if let Some(config) = self.config.clone() {
                        let expected = match request.target {
                            Target::Local => Ownership::Owned,
                            Target::Remote => Ownership::Absent,
                        };
                        self.worker.submit(config, request.target, Some(expected));
                        self.status = self
                            .text("Automatic switch queued…", "自动切换已排队…")
                            .into();
                    }
                }
            }
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("PortKey").size(28.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let previous = self.draft.language.clone();
                    egui::ComboBox::from_id_salt("language").selected_text(if previous == "zh-Hans" { "简体中文" } else { "English" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.draft.language, "en".into(), "English");
                        ui.selectable_value(&mut self.draft.language, "zh-Hans".into(), "简体中文");
                    });
                    if previous != self.draft.language {
                        if !self.error { self.status.clear(); }
                        if let Some(path) = prefs_path() { if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); } let _ = std::fs::write(path, &self.draft.language); }
                        if let Some(c) = &mut self.config { c.language = self.draft.language.clone(); if let Ok(p) = Config::path() { if let Err(e) = c.save(&p) { self.status = e; self.error = true; } } }
                    }
                });
            });
            ui.label(self.text("One desk. Two computers. Your display follows the USB switch.", "一套桌面，两台电脑。显示器跟随 USB 切换器。"));
            ui.add_space(10.0);
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                if self.editing { self.setup_ui(ui); } else { self.control_ui(ui); }
                ui.add_space(12.0);
                if !self.lifecycle_ok { ui.colored_label(egui::Color32::LIGHT_RED, self.text("Power monitoring unavailable. Switching is disabled; restart PortKey after resolving the error.", "电源状态监听不可用，切换功能已禁用。请排除错误后重启 PortKey。")); }
                if !self.status.is_empty() { ui.colored_label(if self.error { egui::Color32::LIGHT_RED } else { egui::Color32::LIGHT_BLUE }, crate::messages::display(&self.status, self.draft.language == "zh-Hans")); }
                ui.add_space(14.0); ui.separator();
                ui.small(self.text("Keep both computers awake. Enable automation on only one host. DDC/CI must be enabled in the monitor menu.", "两台电脑需保持唤醒。只在一台电脑启用自动切换，并在显示器菜单开启 DDC/CI。"));
                if let Ok(path) = Config::path() { ui.small(format!("{}: {}", self.text("Configuration", "配置文件"), path.display())); }
            });
        });
        ctx.request_repaint_after(Duration::from_millis(200));
    }
}
impl App {
    fn setup_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(self.text("Set up your desk", "设置你的桌面"));
        ui.label(self.text("Select the monitor and exactly two hubs that disappear together when you switch USB to the other computer. Device paths distinguish identical hubs.", "请选择目标显示器，以及 USB 切到另一台电脑时同时消失的两个集线器。设备路径用于区分同型号集线器。"));
        if ui
            .button(self.text("Refresh devices", "刷新设备"))
            .clicked()
        {
            self.worker.refresh();
        }
        ui.add_space(10.0);
        ui.strong(self.text("1 · Monitor", "1 · 显示器"));
        if let Some(inventory) = &self.inventory {
            if inventory.displays.is_empty() {
                ui.label(self.text(
                    "No supported monitor found. Check the cable and DDC/CI setting, then refresh.",
                    "未发现支持的显示器，请检查连接线及 DDC/CI 设置后刷新。",
                ));
            }
            for display in &inventory.displays {
                ui.radio_value(
                    &mut self.draft.display_id,
                    display.id.clone(),
                    &display.label,
                )
                .on_hover_text(&display.id);
            }
            ui.add_space(10.0);
            ui.strong(self.text(
                "2 · USB switch hubs (select exactly two)",
                "2 · USB 切换器集线器（选择两个）",
            ));
            if inventory.hubs.is_empty() {
                ui.label(self.text(
                    "No hubs discovered. Connect the USB switch to this computer and refresh.",
                    "未发现集线器，请将 USB 切换器连接到本机后刷新。",
                ));
            }
            for hub in &inventory.hubs {
                let mut selected = self.draft.hubs.iter().any(|h| h.id == hub.id);
                if ui
                    .checkbox(&mut selected, &hub.label)
                    .on_hover_text(&hub.id)
                    .changed()
                {
                    if selected {
                        self.draft.hubs.push(hub.clone());
                    } else {
                        self.draft.hubs.retain(|h| h.id != hub.id);
                    }
                }
            }
        } else {
            ui.spinner();
            ui.label(self.text("Discovering devices…", "正在发现设备…"));
        }
        // Keep saved offline enrollment visible and removable.
        let selected = self.draft.hubs.clone();
        for hub in selected {
            let present = self
                .inventory
                .as_ref()
                .is_some_and(|i| i.hubs.iter().any(|h| h.id == hub.id));
            if !present {
                ui.horizontal(|ui| {
                    ui.label(format!("{} ({})", hub.label, self.text("offline", "离线")));
                    if ui.small_button(self.text("Remove", "移除")).clicked() {
                        self.draft.hubs.retain(|h| h.id != hub.id);
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.strong(self.text("3 · Monitor inputs", "3 · 显示器输入源"));
        ui.label(self.text("Use your monitor's VCP input values. Common values: DisplayPort 15/16, HDMI 17/18, USB-C often 27 (model dependent).", "请使用显示器的 VCP 输入值。常见值：DisplayPort 15/16，HDMI 17/18，USB-C 常为 27（以型号为准）。"));
        let local_label = self.text("This computer", "本机");
        let remote_label = self.text("Other computer", "另一台电脑");
        ui.horizontal(|ui| {
            ui.label(local_label);
            ui.add(egui::DragValue::new(&mut self.draft.local_input).range(1..=255));
            ui.label(remote_label);
            ui.add(egui::DragValue::new(&mut self.draft.remote_input).range(1..=255));
        });
        ui.add_space(10.0);
        if ui
            .button(self.text("Save enrollment", "保存设备配置"))
            .clicked()
        {
            self.save();
        }
        if self.config.is_some() && ui.button(self.text("Cancel editing", "取消编辑")).clicked()
        {
            self.draft = self.config.clone().unwrap();
            self.editing = false;
            self.reset();
            self.worker.refresh();
        }
        #[cfg(target_os = "macos")]
        ui.collapsing(
            self.text("Import existing Swift setup", "导入原 Swift 版本配置"),
            |ui| {
                ui.text_edit_singleline(&mut self.legacy_path);
                if ui
                    .button(self.text("Load for review", "载入并检查"))
                    .clicked()
                {
                    match Config::import_legacy(std::path::Path::new(&self.legacy_path)) {
                        Ok(mut c) => {
                            c.automatic = false;
                            self.draft = c;
                            self.status = self
                                .text(
                                    "Imported. Review the selection and save to finish.",
                                    "已导入，请检查设备选择并保存。",
                                )
                                .into();
                            self.error = false;
                        }
                        Err(e) => {
                            self.status = e;
                            self.error = true;
                        }
                    }
                }
            },
        );
    }
    fn control_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        let ownership = match self.policy.ownership() {
            Ownership::Owned => self.text("USB connected to this computer", "USB 已连接本机"),
            Ownership::Absent => self.text(
                "Enrolled USB hubs are absent",
                "已配置的 USB 集线器均不在本机",
            ),
            Ownership::Partial => self.text(
                "Only one hub detected · automation waiting",
                "只检测到一个集线器 · 等待稳定状态",
            ),
            Ownership::Unknown => {
                self.text("Waiting for reliable USB evidence", "等待可靠的 USB 状态")
            }
        };
        ui.heading(ownership);
        ui.add_space(12.0);
        let mut automatic = self.draft.automatic;
        if ui
            .checkbox(&mut automatic, self.text("Automatic switching", "自动切换"))
            .changed()
        {
            if automatic {
                self.draft.automatic = true;
                self.save();
            } else {
                self.pause_automation();
            }
        }
        ui.label(self.text("Automation waits until both enrolled hubs have been observed on this computer. Startup absence never triggers a switch.", "自动切换需先在本机观察到两个已配置集线器。启动时设备缺失不会触发切换。"));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if ui
                .add_sized(
                    [200.0, 40.0],
                    egui::Button::new(self.text("Use this computer", "切换到本机")),
                )
                .clicked()
            {
                self.manual(Target::Local);
            }
            if ui
                .add_sized(
                    [200.0, 40.0],
                    egui::Button::new(self.text("Use other computer", "切换到另一台电脑")),
                )
                .clicked()
            {
                self.manual(Target::Remote);
            }
        });
        ui.add_space(16.0);
        if let Some(c) = &self.config {
            ui.label(format!(
                "{}: {}",
                self.text("Enrolled display", "目标显示器"),
                c.display_id
            ));
        }
        ui.horizontal(|ui| {
            if ui.button(self.text("Edit setup", "修改配置")).clicked() {
                self.reset();
                self.editing = true;
            }
            if ui
                .button(self.text("Refresh devices", "刷新设备"))
                .clicked()
            {
                self.worker.refresh();
            }
        });
        ui.small(if self.tray.is_some() {
            self.text("Closing this window keeps PortKey in the menu bar / system tray. Use its menu to reopen or quit.", "关闭窗口后 PortKey 仍在菜单栏或系统托盘运行，可通过托盘菜单重新打开或退出。")
        } else { self.text("Minimize to keep running. Closing the window quits PortKey.", "最小化窗口可保持运行。关闭窗口会退出 PortKey。") });
    }
}
