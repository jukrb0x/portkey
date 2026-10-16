use crate::core::{Config, Ownership, Policy, Target};
use crate::platform::{self, Inventory, WriteOutcome};
use crate::runtime::{Dispatch, Latest};
use serde_json::{json, Value};
use std::fs::File;
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
    fn new(instance: Arc<File>) -> Self {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let thread_shared = shared.clone();
        std::thread::spawn(move || {
            let _instance = instance;
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
    fn idle() -> Self {
        Self {
            shared: Arc::new((Mutex::new(Shared::default()), Condvar::new())),
        }
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

/// UI-neutral state owner. Native frontends call this only from their UI thread.
pub struct Controller {
    worker: Worker,
    policy: Policy,
    config: Option<Config>,
    inventory: Option<Inventory>,
    status: String,
    error: bool,
    editing: bool,
    started: Instant,
    last_frame: SystemTime,
    dispatch: Dispatch,
    quitting: bool,
    lifecycle_ok: bool,
    lifecycle_epoch: u64,
    lifecycle: fn() -> (u64, bool),
    language: String,
    path: Result<PathBuf, String>,
    instance: Option<Arc<File>>,
    startup_error: Option<String>,
}
impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}
impl Controller {
    #[cfg(test)]
    pub(crate) fn test_instance() -> Self {
        Self::base(Err("test: persistence disabled".into()), Ok(None))
    }
    pub fn new() -> Self {
        let path = Config::path();
        let loaded = path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| Config::load(p));
        let mut controller = Self::base(path, loaded);
        let startup = controller
            .path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| {
                crate::instance::acquire(p.parent().ok_or("Invalid configuration directory")?)
            });
        match startup {
            Ok(instance) => {
                let instance = Arc::new(instance);
                controller.worker = Worker::new(instance.clone());
                controller.instance = Some(instance);
                // The subscription lives until process termination; create it only once.
                static POWER: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
                match POWER.get_or_init(crate::lifecycle::start) {
                    Ok(()) => controller.lifecycle_ok = true,
                    Err(error) => controller.startup_error = Some(error.clone()),
                }
            }
            Err(error) => controller.startup_error = Some(error),
        }
        controller
    }
    fn base(path: Result<PathBuf, String>, loaded: Result<Option<Config>, String>) -> Self {
        let loaded = loaded.and_then(|config| {
            if config
                .as_ref()
                .is_some_and(|c| c.platform != std::env::consts::OS)
            {
                Err("Configuration belongs to another platform; enroll this computer.".into())
            } else {
                Ok(config)
            }
        });
        let (config, status, error) = match loaded {
            Ok(c) => (c, String::new(), false),
            Err(error) => (None, error, true),
        };
        let language = config
            .as_ref()
            .map(|c| c.language.clone())
            .or_else(|| {
                let path = path.as_ref().ok()?;
                let read = |path: PathBuf| {
                    std::fs::read_to_string(path)
                        .ok()
                        .filter(|value| matches!(value.as_str(), "en" | "zh-Hans"))
                };
                read(path.with_file_name("language")).or_else(|| {
                    // The preview stored preferences under lowercase portkey.
                    let directory = path.parent()?;
                    if directory.file_name()? != "PortKey" {
                        return None;
                    }
                    read(directory.parent()?.join("portkey/language"))
                })
            })
            .unwrap_or_else(|| "en".into());
        let mut policy = Policy::new();
        policy.set_automatic(config.as_ref().is_some_and(|c| c.automatic), 0);
        Self {
            editing: config.is_none(),
            config,
            status,
            error,
            language,
            policy,
            path,
            worker: Worker::idle(),
            inventory: None,
            started: Instant::now(),
            last_frame: SystemTime::now(),
            dispatch: Dispatch::default(),
            quitting: false,
            lifecycle_ok: false,
            lifecycle_epoch: crate::lifecycle::snapshot().0,
            lifecycle: crate::lifecycle::snapshot,
            instance: None,
            startup_error: None,
        }
    }
    fn text<'a>(&self, en: &'a str, zh: &'a str) -> &'a str {
        if self.language == "zh-Hans" {
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
    fn tick(&mut self) {
        if self.quitting || self.startup_error.is_some() {
            return;
        }
        let now = self.started.elapsed().as_millis() as u64;
        let (epoch, sleeping) = (self.lifecycle)();
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
            self.inventory = None;
            self.worker.refresh();
            return;
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
    }
    pub fn snapshot(&mut self) -> Value {
        self.tick();
        let inventory = self
            .inventory
            .as_ref()
            .map(|i| json!({"displays":i.displays,"hubs":i.hubs}));
        let status = self.startup_error.as_ref().unwrap_or(&self.status);
        json!({
            "config": self.config, "inventory": inventory,
            "ownership": match self.policy.ownership() { Ownership::Unknown => "unknown", Ownership::Owned => "owned", Ownership::Absent => "absent", Ownership::Partial => "partial" },
            "status": crate::messages::display(status, self.language == "zh-Hans"),
            "error": self.error || self.startup_error.is_some(),
            "lifecycle_ok": self.lifecycle_ok, "sleeping": (self.lifecycle)().1,
            "language": self.language, "config_path": self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            "editing": self.editing
        })
    }
    pub fn action(&mut self, value: Value) -> Value {
        match self.perform(value) {
            Ok(extra) => extra,
            Err(error) => {
                self.status = error.clone();
                self.error = true;
                json!({"ok":false,"error":crate::messages::display(&error, self.language == "zh-Hans")})
            }
        }
    }
    fn perform(&mut self, value: Value) -> Result<Value, String> {
        let action: Action =
            serde_json::from_value(value).map_err(|e| format!("Invalid action: {e}"))?;
        if self.quitting {
            return Err("PortKey is shutting down.".into());
        }
        if matches!(action, Action::Quit) {
            self.quitting = true;
            self.policy.sleep();
            self.worker.invalidate();
            let mut s = self.worker.shared.0.lock().unwrap();
            s.stop = true;
            self.worker.shared.1.notify_one();
            return Ok(json!({"ok":true}));
        }
        if let Some(error) = &self.startup_error {
            return Err(error.clone());
        }
        match action {
            Action::Refresh => self.worker.refresh(),
            Action::Edit { enabled } => {
                if !enabled && self.config.is_none() {
                    return Err("Complete enrollment first.".into());
                }
                self.reset();
                self.editing = enabled;
                self.worker.refresh();
            }
            Action::Save { config } => {
                self.validate_config(&config)?;
                // Cancel the old enrollment before touching storage, even if saving fails.
                self.pause();
                config.save(self.path.as_ref().map_err(Clone::clone)?)?;
                self.language = config.language.clone();
                self.policy
                    .set_automatic(config.automatic, self.started.elapsed().as_millis() as u64);
                self.config = Some(config);
                self.editing = false;
                self.reset();
                self.worker.refresh();
                self.status = self
                    .text(
                        "Settings saved. Waiting for fresh USB evidence.",
                        "设置已保存，正在等待新的 USB 状态。",
                    )
                    .into();
                self.error = false;
            }
            Action::Manual { target } => {
                if self.editing || !self.lifecycle_ok || (self.lifecycle)().1 {
                    return Err("Switching unavailable while editing, sleeping or power monitoring is unavailable.".into());
                }
                let config = self.config.clone().ok_or("Complete enrollment first.")?;
                self.validate_config(&config)?;
                self.worker.invalidate();
                self.policy.manual(target.into());
                if let Some(request) = self.policy.request() {
                    self.dispatch.accept(request.generation);
                }
                self.worker.submit(config, target.into(), None);
                self.status = self
                    .text("Switch request queued…", "切换请求已排队…")
                    .into();
                self.error = false;
            }
            Action::Automatic { enabled } => {
                if !enabled {
                    self.pause();
                    if let Some(config) = &self.config {
                        config
                            .save(self.path.as_ref().map_err(Clone::clone)?)
                            .map_err(|e| format!("Paused for this session; could not save. {e}"))?;
                    }
                    self.status = self
                        .text("Automatic switching paused.", "自动切换已暂停。")
                        .into();
                    self.error = false;
                } else {
                    if self.editing || !self.lifecycle_ok || (self.lifecycle)().1 {
                        return Err(
                            "Finish setup and restore power monitoring before enabling automation."
                                .into(),
                        );
                    }
                    let mut config = self.config.clone().ok_or("Complete enrollment first.")?;
                    config.automatic = true;
                    return self.perform(json!({"type":"save","config":config}));
                }
            }
            Action::Language { value } => {
                let language = match value {
                    Language::English => "en",
                    Language::Chinese => "zh-Hans",
                };
                if let Some(mut config) = self.config.clone() {
                    config.language = language.into();
                    config.save(self.path.as_ref().map_err(Clone::clone)?)?;
                    self.config = Some(config);
                } else {
                    let path = self
                        .path
                        .as_ref()
                        .map_err(Clone::clone)?
                        .with_file_name("language");
                    std::fs::create_dir_all(path.parent().ok_or("Invalid preference directory")?)
                        .map_err(|e| format!("Cannot save language preference: {e}"))?;
                    std::fs::write(path, language)
                        .map_err(|e| format!("Cannot save language preference: {e}"))?;
                }
                self.language = language.into();
                // Statuses are generated in the active language; discard obsolete UI text.
                if !self.error {
                    self.status.clear();
                }
            }
            Action::Import { path } => {
                let config = Config::import_legacy(std::path::Path::new(&path))?;
                self.validate_config(&config)?;
                self.reset();
                self.editing = true;
                self.status = self
                    .text(
                        "Imported. Review the selection and save to finish.",
                        "已导入，请检查设备选择并保存。",
                    )
                    .into();
                self.error = false;
                return Ok(json!({"ok":true,"config":config}));
            }
            Action::Quit => unreachable!(),
        }
        Ok(json!({"ok":true}))
    }
    fn validate_config(&self, config: &Config) -> Result<(), String> {
        config.validate()?;
        if config.platform != std::env::consts::OS {
            return Err("Configuration belongs to another platform; enroll this computer.".into());
        }
        Ok(())
    }
    fn pause(&mut self) {
        let mut shared = self.worker.shared.0.lock().unwrap();
        crate::runtime::pause(
            &mut self.policy,
            &mut shared.jobs,
            self.started.elapsed().as_millis() as u64,
        );
        shared.completion = None;
        shared.inventory = None;
        if let Some(config) = &mut self.config {
            config.automatic = false;
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Action {
    Refresh,
    Save { config: Config },
    Manual { target: NativeTarget },
    Automatic { enabled: bool },
    Language { value: Language },
    Edit { enabled: bool },
    Import { path: String },
    Quit,
}
#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum NativeTarget {
    Local,
    Remote,
}
impl From<NativeTarget> for Target {
    fn from(value: NativeTarget) -> Self {
        match value {
            NativeTarget::Local => Self::Local,
            NativeTarget::Remote => Self::Remote,
        }
    }
}
#[derive(serde::Deserialize)]
enum Language {
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-Hans")]
    Chinese,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn controller() -> Controller {
        let mut c = Controller::base(
            Ok(std::env::temp_dir().join("portkey-controller-no-write/config.json")),
            Ok(None),
        );
        c.lifecycle_ok = true;
        c.lifecycle = || (0, false);
        c.lifecycle_epoch = 0;
        c
    }
    #[test]
    fn unenrolled_and_malformed_actions_never_enqueue() {
        let mut c = controller();
        for action in [
            json!({"type":"manual","target":"local"}),
            json!({"type":"manual","target":"anything"}),
            json!({"type":"automatic","enabled":"true"}),
            json!({"type":"save","config":{}}),
        ] {
            assert_eq!(c.action(action)["ok"], false);
            assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        }
        assert!(!c.policy.automatic());
    }
    #[test]
    fn quit_cancels_and_rejects_followup_actions() {
        let mut c = controller();
        assert_eq!(c.action(json!({"type":"quit"}))["ok"], true);
        assert_eq!(c.action(json!({"type":"refresh"}))["ok"], false);
        assert!(c.policy.sleeping());
    }
    #[test]
    fn snapshots_expose_native_contract() {
        let mut c = controller();
        let s = c.snapshot();
        assert!(s["config"].is_null());
        assert_eq!(s["ownership"], "unknown");
        assert_eq!(s["language"], "en");
        assert_eq!(s["editing"], true);
    }
    fn enrolled() -> Controller {
        let mut c = controller();
        let mac = std::env::consts::OS == "macos";
        c.config = Some(Config {
            version: 1,
            platform: std::env::consts::OS.into(),
            display_id: if mac {
                "12345678-1234-1234-1234-123456789abc"
            } else {
                r"\\?\display#test#stable"
            }
            .into(),
            local_input: 17,
            remote_input: 15,
            hubs: if mac {
                vec![
                    "macos:05e3:0610:02121000:05e3:0610:02120000",
                    "macos:05e3:0626:02210000:05e3:0626:02200000",
                ]
            } else {
                vec!["windows:pciroot(0)#usb(1)", "windows:pciroot(0)#usb(2)"]
            }
            .into_iter()
            .map(|id| crate::core::Hub {
                id: id.into(),
                label: "test".into(),
            })
            .collect(),
            automatic: false,
            language: "en".into(),
        });
        c.editing = false;
        c
    }
    #[test]
    fn manual_is_dispatched_once_and_edit_cancels() {
        let mut c = enrolled();
        assert_eq!(
            c.action(json!({"type":"manual","target":"remote"}))["ok"],
            true
        );
        let (generation, job) = c.worker.shared.0.lock().unwrap().jobs.take().unwrap();
        assert_eq!(job.target, Target::Remote);
        assert_eq!(job.expected, None);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        assert_eq!(c.action(json!({"type":"edit","enabled":true}))["ok"], true);
        assert!(!c.worker.shared.0.lock().unwrap().jobs.current(generation));
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            false
        );
    }
    #[test]
    fn pause_and_save_failure_cancel_before_persistence() {
        let mut c = enrolled();
        // No filesystem writes: this invalid parent path fails before creation.
        c.path = Ok(PathBuf::from("config.json"));
        c.policy.set_automatic(true, 0);
        c.config.as_mut().unwrap().automatic = true;
        c.action(json!({"type":"manual","target":"local"}));
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":false}))["ok"],
            false
        );
        assert!(!c.policy.automatic());
        assert!(!c.config.as_ref().unwrap().automatic);
        assert!(!c.worker.shared.0.lock().unwrap().jobs.current(generation));
        c.policy.set_automatic(true, 0);
        c.config.as_mut().unwrap().automatic = true;
        let config = c.config.clone().unwrap();
        assert_eq!(
            c.action(json!({"type":"save","config":config}))["ok"],
            false
        );
        assert!(!c.policy.automatic());
    }
    #[test]
    fn stale_completion_and_pre_sleep_inventory_are_ignored() {
        let mut c = enrolled();
        c.status = "current".into();
        {
            let mut shared = c.worker.shared.0.lock().unwrap();
            let stale = shared.jobs.generation();
            shared.jobs.invalidate();
            shared.completion = Some((stale, Err("stale".into())));
            shared.inventory = Some((
                99,
                Ok(Inventory {
                    hubs: c.config.as_ref().unwrap().hubs.clone(),
                    displays: vec![],
                }),
            ));
        }
        c.snapshot();
        assert_eq!(c.status, "current");
        assert!(c.inventory.is_none());
        assert_eq!(c.policy.ownership(), Ownership::Unknown);
    }
    #[test]
    fn lifecycle_failures_and_sleep_block_manual_and_enable() {
        let mut c = enrolled();
        c.lifecycle_ok = false;
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            false
        );
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":true}))["ok"],
            false
        );
        c.lifecycle_ok = true;
        c.lifecycle = || (1, true);
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            false
        );
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        c.startup_error = Some("PortKey is already running".into());
        assert_eq!(
            c.action(json!({"type":"language","value":"zh-Hans"}))["ok"],
            false
        );
        assert_eq!(c.snapshot()["error"], true);
    }
    #[test]
    fn unenrolled_language_persists_across_restart_and_config_wins() {
        let directory =
            std::env::temp_dir().join(format!("portkey-language-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config-rust.json");
        let mut first = Controller::base(Ok(path.clone()), Ok(None));
        assert_eq!(
            first.action(json!({"type":"language","value":"zh-Hans"}))["ok"],
            true
        );
        let restarted = Controller::base(Ok(path.clone()), Ok(None));
        assert_eq!(restarted.language, "zh-Hans");
        assert!(restarted.config.is_none());
        let saved = Controller::base(Ok(path.clone()), Ok(enrolled().config));
        assert_eq!(saved.language, "en");
        std::fs::write(directory.join("language"), "invalid").unwrap();
        let restarted = Controller::base(Ok(path), Ok(None));
        assert_eq!(restarted.language, "en");
        std::fs::remove_dir_all(directory).unwrap();
    }
}
