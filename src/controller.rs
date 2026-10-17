use crate::core::{Config, Display, Hub, Ownership, Policy, Target};
use crate::platform::{self, Inventory, WriteOutcome};
use crate::profiles::Profiles;
use crate::runtime::{Dispatch, Latest};
use serde_json::{json, Value};
use std::fs::File;
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant, SystemTime},
};

struct Probe {
    display_id: String,
    epoch: u64,
    generation: u64,
}

struct Job {
    config: Config,
    target: Target,
    expected: Option<Ownership>,
    queued: SystemTime,
    queued_monotonic: Instant,
    epoch: u64,
    display_ids: Vec<String>,
}
#[derive(Default)]
struct Shared {
    jobs: Latest<Job>,
    inventory_revision: u64,
    usb_observer: bool,
    usb_sequence: u64,
    refresh_usb: bool,
    display_cache: Option<Vec<Display>>,
    usb_cache: Option<Result<Vec<Hub>, String>>,
    inventory: Option<(u64, Result<Inventory, String>)>,
    completion: Option<(u64, Result<WriteOutcome, String>)>,
    probe: Option<Probe>,
    probe_completion: Option<(u64, Result<u8, String>)>,
    stop: bool,
    refresh: bool,
    resumed: bool,
}
impl Shared {
    fn cancel_commands(&mut self) {
        self.jobs.invalidate();
        self.completion = None;
        self.probe = None;
        self.probe_completion = None;
    }
    fn invalidate_inventory(&mut self) {
        self.inventory_revision = self.inventory_revision.wrapping_add(1);
        self.inventory = None;
        self.display_cache = None;
        self.usb_cache = None;
        if self.usb_observer {
            self.refresh = true;
            self.refresh_usb = true;
        }
    }
    fn invalidate(&mut self) {
        self.cancel_commands();
        self.invalidate_inventory();
    }
    #[cfg(test)]
    fn publish_inventory(
        &mut self,
        revision: u64,
        epoch: u64,
        inventory: Result<Inventory, String>,
    ) {
        self.publish_inventory_from_scan(revision, epoch, inventory, self.usb_sequence);
    }
    fn publish_inventory_from_scan(
        &mut self,
        revision: u64,
        epoch: u64,
        inventory: Result<Inventory, String>,
        usb_sequence_at_start: u64,
    ) {
        let current = revision == self.inventory_revision;
        crate::timing::event(
            if current {
                "inventory_publish"
            } else {
                "inventory_discarded"
            },
            "inventory",
            None,
            Some(revision),
        );
        if current {
            let inventory = if self.usb_observer {
                match inventory {
                    Ok(mut inventory) => {
                        self.display_cache = Some(inventory.displays.clone());
                        if self.usb_sequence == usb_sequence_at_start {
                            self.usb_sequence = self.usb_sequence.wrapping_add(1);
                            self.usb_cache = Some(Ok(inventory.hubs.clone()));
                        }
                        match &self.usb_cache {
                            Some(Ok(hubs)) => {
                                inventory.hubs = hubs.clone();
                                Ok(inventory)
                            }
                            Some(Err(error)) => Err(error.clone()),
                            None => Ok(inventory),
                        }
                    }
                    Err(error) => {
                        self.display_cache = None;
                        Err(error)
                    }
                }
            } else {
                inventory
            };
            self.inventory = Some((epoch, inventory));
        }
    }
    fn validation_snapshot(
        &self,
        inventory: &Result<Inventory, String>,
    ) -> Result<Inventory, String> {
        let mut snapshot = inventory.clone()?;
        if let Some(cached) = &self.display_cache {
            for display in &mut snapshot.displays {
                let mut matches = cached.iter().filter(|old| old.id == display.id);
                if let Some(old) = matches.next() {
                    if matches.next().is_none() {
                        display.label.clone_from(&old.label);
                    }
                }
            }
        }
        Ok(snapshot)
    }
    fn publish_usb(&mut self, revision: u64, epoch: u64, hubs: Result<Vec<Hub>, String>) {
        if revision != self.inventory_revision {
            return;
        }
        self.usb_sequence = self.usb_sequence.wrapping_add(1);
        self.usb_cache = Some(hubs.clone());
        match hubs {
            Ok(hubs) => {
                if let Some(displays) = &self.display_cache {
                    self.inventory = Some((
                        epoch,
                        Ok(Inventory {
                            displays: displays.clone(),
                            hubs,
                        }),
                    ));
                }
            }
            Err(error) => {
                self.inventory = Some((epoch, Err(error)));
            }
        }
    }
}
// Native USB snapshots are inexpensive; full display discovery has its own cadence.
const USB_POLL_INTERVAL: Duration = Duration::from_millis(250);
fn usb_poll_wait(elapsed: Duration, refresh: bool) -> Option<Duration> {
    if refresh {
        None
    } else {
        USB_POLL_INTERVAL
            .checked_sub(elapsed)
            .filter(|wait| !wait.is_zero())
    }
}

type ValidationRead = Box<dyn FnMut() -> Result<Inventory, String> + Send>;
type UsbRead = Box<dyn FnMut() -> Result<Vec<Hub>, String> + Send>;

struct Worker {
    shared: Arc<(Mutex<Shared>, Condvar)>,
}
impl Worker {
    fn new(instance: Arc<File>) -> Self {
        #[cfg(target_os = "windows")]
        return Self::start_with_readers(
            instance,
            platform::inventory,
            platform::set_input,
            platform::read_input,
            Some(Box::new(platform::usb_inventory)),
            Some(Box::new(platform::prewrite_inventory)),
        );
        #[cfg(not(target_os = "windows"))]
        Self::start(
            instance,
            platform::inventory,
            platform::set_input,
            platform::read_input,
        )
    }
    #[cfg(any(not(target_os = "windows"), test))]
    fn start(
        instance: Arc<File>,
        inventory_read: impl FnMut() -> Result<Inventory, String> + Send + 'static,
        input_write: impl FnMut(&Config, u8) -> Result<WriteOutcome, String> + Send + 'static,
        input_read: impl FnMut(&str) -> Result<u8, String> + Send + 'static,
    ) -> Self {
        Self::start_with_usb(instance, inventory_read, input_write, input_read, None)
    }
    #[cfg(any(not(target_os = "windows"), test))]
    fn start_with_usb(
        instance: Arc<File>,
        inventory_read: impl FnMut() -> Result<Inventory, String> + Send + 'static,
        input_write: impl FnMut(&Config, u8) -> Result<WriteOutcome, String> + Send + 'static,
        input_read: impl FnMut(&str) -> Result<u8, String> + Send + 'static,
        usb_read: Option<UsbRead>,
    ) -> Self {
        Self::start_with_readers(
            instance,
            inventory_read,
            input_write,
            input_read,
            usb_read,
            None,
        )
    }
    fn start_with_readers(
        instance: Arc<File>,
        mut inventory_read: impl FnMut() -> Result<Inventory, String> + Send + 'static,
        mut input_write: impl FnMut(&Config, u8) -> Result<WriteOutcome, String> + Send + 'static,
        mut input_read: impl FnMut(&str) -> Result<u8, String> + Send + 'static,
        usb_read: Option<UsbRead>,
        mut validation_read: Option<ValidationRead>,
    ) -> Self {
        let split_usb = usb_read.is_some();
        let shared = Arc::new((
            Mutex::new(Shared {
                usb_observer: split_usb,
                ..Shared::default()
            }),
            Condvar::new(),
        ));
        if let Some(mut usb_read) = usb_read {
            let usb_shared = shared.clone();
            let usb_instance = instance.clone();
            std::thread::spawn(move || {
                let _instance = usb_instance;
                let (lock, wake) = &*usb_shared;
                let mut last_scan = Instant::now() - Duration::from_secs(10);
                loop {
                    let mut state = lock.lock().unwrap();
                    if state.stop {
                        break;
                    }
                    if let Some(wait) = usb_poll_wait(last_scan.elapsed(), state.refresh_usb) {
                        drop(wake.wait_timeout(state, wait).unwrap());
                        continue;
                    }
                    state.refresh_usb = false;
                    let revision = state.inventory_revision;
                    drop(state);
                    let (epoch, sleeping) = crate::lifecycle::snapshot();
                    if sleeping {
                        last_scan = Instant::now();
                        continue;
                    }
                    let wall = SystemTime::now();
                    let started = Instant::now();
                    crate::timing::event("usb_inventory_begin", "native", None, Some(revision));
                    let hubs = usb_read();
                    crate::timing::event(
                        if hubs.is_ok() {
                            "usb_inventory_end_ok"
                        } else {
                            "usb_inventory_end_error"
                        },
                        "native",
                        Some(started.elapsed()),
                        Some(revision),
                    );
                    last_scan = Instant::now();
                    let mut state = lock.lock().unwrap();
                    if wall
                        .elapsed()
                        .unwrap_or(Duration::MAX)
                        .saturating_sub(started.elapsed())
                        > Duration::from_secs(2)
                    {
                        state.invalidate();
                        state.resumed = true;
                    } else if crate::lifecycle::snapshot() == (epoch, false) {
                        state.publish_usb(revision, epoch, hubs);
                    }
                }
            });
        }
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
                    state.invalidate();
                    state.resumed = true;
                }
                let inventory_revision = state.inventory_revision;
                let usb_sequence_at_start = state.usb_sequence;
                let job = state.jobs.take();
                let probe = state.probe.take();
                let refresh = state.refresh
                    || last_inventory.elapsed()
                        >= Duration::from_secs(if split_usb { 10 } else { 2 });
                state.refresh = false;
                if job.is_none() && probe.is_none() && !refresh {
                    drop(
                        wake.wait_timeout(state, Duration::from_millis(250))
                            .unwrap(),
                    );
                    continue;
                }
                drop(state);
                if let Some((generation, job)) = &job {
                    crate::timing::event(
                        "dispatch_dequeued",
                        if job.expected.is_some() {
                            "automatic"
                        } else {
                            "manual"
                        },
                        Some(job.queued_monotonic.elapsed()),
                        Some(*generation),
                    );
                }
                // Every write rechecks discovery; a failed read is never absence.
                let inventory_epoch = crate::lifecycle::snapshot().0;
                let wall_started = SystemTime::now();
                let monotonic_started = Instant::now();
                let inventory_role = if job.is_some() {
                    "prewrite"
                } else if probe.is_some() {
                    "probe"
                } else {
                    "idle"
                };
                crate::timing::event(
                    "inventory_begin",
                    inventory_role,
                    None,
                    Some(inventory_revision),
                );
                let inventory = if job.is_some() || probe.is_some() {
                    match validation_read.as_mut() {
                        Some(read) => read(),
                        None => inventory_read(),
                    }
                } else {
                    inventory_read()
                };
                crate::timing::event(
                    if inventory.is_ok() {
                        "inventory_end_ok"
                    } else {
                        "inventory_end_error"
                    },
                    inventory_role,
                    Some(monotonic_started.elapsed()),
                    Some(inventory_revision),
                );
                let suspended = wall_started
                    .elapsed()
                    .unwrap_or(Duration::MAX)
                    .saturating_sub(monotonic_started.elapsed())
                    > Duration::from_secs(2);
                if suspended {
                    let mut s = lock.lock().unwrap();
                    s.invalidate();
                    s.resumed = true;
                }
                last_inventory = Instant::now();
                {
                    let mut state = lock.lock().unwrap();
                    let snapshot =
                        if validation_read.is_some() && (job.is_some() || probe.is_some()) {
                            state.validation_snapshot(&inventory)
                        } else {
                            inventory.clone()
                        };
                    state.publish_inventory_from_scan(
                        inventory_revision,
                        inventory_epoch,
                        snapshot,
                        usb_sequence_at_start,
                    );
                }
                let result = job.as_ref().map(|(generation, job)| {
                    if job.queued.elapsed().unwrap_or(Duration::MAX) > Duration::from_secs(30)
                        || !lock.lock().unwrap().jobs.current(*generation)
                    {
                        return Err(
                            "Request expired or was cancelled; retry after discovery.".into()
                        );
                    }
                    let inventory = inventory.as_ref().map_err(Clone::clone)?;
                    require_unique_profile(&job.display_ids, inventory, &job.config.display_id)?;
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
                            crate::timing::event(
                                "retry_wait",
                                "write",
                                Some(Duration::from_millis(750 * attempt)),
                                Some(*generation),
                            );
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
                            let retry_usb_sequence = lock.lock().unwrap().usb_sequence;
                            let retry_started = Instant::now();
                            crate::timing::event(
                                "inventory_begin",
                                "retry",
                                None,
                                Some(*generation),
                            );
                            let fresh = match validation_read.as_mut() {
                                Some(read) => read(),
                                None => inventory_read(),
                            };
                            crate::timing::event(
                                if fresh.is_ok() {
                                    "inventory_end_ok"
                                } else {
                                    "inventory_end_error"
                                },
                                "retry",
                                Some(retry_started.elapsed()),
                                Some(*generation),
                            );
                            {
                                let mut state = lock.lock().unwrap();
                                let snapshot = if validation_read.is_some() {
                                    state.validation_snapshot(&fresh)
                                } else {
                                    fresh.clone()
                                };
                                state.publish_inventory_from_scan(
                                    inventory_revision,
                                    epoch,
                                    snapshot,
                                    retry_usb_sequence,
                                );
                            }
                            let fresh = fresh?;
                            require_unique_profile(
                                &job.display_ids,
                                &fresh,
                                &job.config.display_id,
                            )?;
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
                        crate::timing::event(
                            "write_attempt_begin",
                            if job.target == Target::Local {
                                "local"
                            } else {
                                "remote"
                            },
                            None,
                            Some(attempt),
                        );
                        outcome = input_write(&job.config, job.config.input(job.target));
                        crate::timing::event(
                            match &outcome {
                                Ok(WriteOutcome::Confirmed) => "write_attempt_confirmed",
                                Ok(WriteOutcome::SentUnverified(_)) => "write_attempt_unverified",
                                Err(_) => "write_attempt_error",
                            },
                            "write",
                            Some(monotonic.elapsed()),
                            Some(attempt),
                        );
                        if wall
                            .elapsed()
                            .unwrap_or(Duration::MAX)
                            .saturating_sub(monotonic.elapsed())
                            > Duration::from_secs(2)
                        {
                            let mut state = lock.lock().unwrap();
                            state.invalidate();
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
                let probe_result = probe.as_ref().map(|probe| {
                    let (epoch, sleeping) = crate::lifecycle::snapshot();
                    if sleeping
                        || epoch != probe.epoch
                        || !lock.lock().unwrap().jobs.current(probe.generation)
                    {
                        return Err("Input read cancelled by lifecycle or newer action.".into());
                    }
                    let inventory = inventory.as_ref().map_err(Clone::clone)?;
                    if inventory
                        .displays
                        .iter()
                        .filter(|d| d.id == probe.display_id)
                        .count()
                        != 1
                    {
                        return Err("Selected display is disconnected or ambiguous.".into());
                    }
                    let input = input_read(&probe.display_id);
                    if crate::lifecycle::snapshot() != (probe.epoch, false) {
                        return Err("Input read interrupted by sleep/wake.".into());
                    }
                    input
                });
                let mut state = lock.lock().unwrap();
                if let (Some(probe), Some(result)) = (probe, probe_result) {
                    if state.jobs.current(probe.generation) {
                        state.probe_completion = Some((probe.generation, result));
                    }
                }
                last_cycle = SystemTime::now();
                if let (Some((generation, job)), Some(result)) = (job, result) {
                    crate::timing::event(
                        if result.is_ok() {
                            "completion_ok"
                        } else {
                            "completion_error"
                        },
                        if state.jobs.current(generation) {
                            "current"
                        } else {
                            "stale"
                        },
                        Some(job.queued_monotonic.elapsed()),
                        Some(generation),
                    );
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
        self.shared.0.lock().unwrap().invalidate();
        self.shared.1.notify_all();
    }
    fn cancel_commands(&self) {
        self.shared.0.lock().unwrap().cancel_commands();
    }
    fn submit(
        &self,
        config: Config,
        target: Target,
        expected: Option<Ownership>,
        display_ids: Vec<String>,
    ) {
        let mut shared = self.shared.0.lock().unwrap();
        shared.jobs.replace(Job {
            config,
            target,
            expected,
            queued: SystemTime::now(),
            queued_monotonic: Instant::now(),
            epoch: crate::lifecycle::snapshot().0,
            display_ids,
        });
        crate::timing::event(
            "dispatch_queued",
            if expected.is_some() {
                "automatic"
            } else {
                "manual"
            },
            None,
            Some(shared.jobs.generation()),
        );
        drop(shared);
        self.shared.1.notify_all();
    }
    fn probe(&self, display_id: String, epoch: u64) {
        self.invalidate();
        let mut s = self.shared.0.lock().unwrap();
        s.probe = Some(Probe {
            display_id,
            epoch,
            generation: s.jobs.generation(),
        });
        self.shared.1.notify_all();
    }
    fn refresh(&self) {
        let mut state = self.shared.0.lock().unwrap();
        state.refresh = true;
        state.refresh_usb = true;
        drop(state);
        self.shared.1.notify_all();
    }
    fn refresh_usb(&self) {
        let mut state = self.shared.0.lock().unwrap();
        if state.usb_observer {
            state.refresh_usb = true;
        } else {
            state.refresh = true;
        }
        drop(state);
        self.shared.1.notify_all();
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let mut s = self.shared.0.lock().unwrap();
        s.stop = true;
        s.invalidate();
        self.shared.1.notify_all();
    }
}

fn require_unique_profile(
    ids: &[String],
    inventory: &Inventory,
    expected: &str,
) -> Result<(), String> {
    let mut connected = inventory.displays.iter().filter(|d| ids.contains(&d.id));
    if connected.next().is_some_and(|d| d.id == expected) && connected.next().is_none() {
        Ok(())
    } else {
        Err("Active display profile changed or is ambiguous; request cancelled.".into())
    }
}

fn controller_config_path(directory: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    if let Some(directory) = directory {
        let path = PathBuf::from(directory);
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(
                "PORTKEY_CONFIG_DIR must be an absolute directory without parent traversal.".into(),
            );
        }
        return Ok(path.join("profiles-rust.json"));
    }
    dirs::config_dir()
        .map(|p| p.join("PortKey/profiles-rust.json"))
        .ok_or_else(|| "Cannot locate the per-user configuration directory".into())
}

struct ProfileOverride {
    profile_id: String,
    was_present: bool,
}

/// UI-neutral state owner. Native frontends call this only from their UI thread.
pub struct Controller {
    worker: Worker,
    setup: Option<crate::setup::Setup>,
    setup_action_error: Option<String>,
    setup_usb_sample: Option<(u64, u64)>,
    draft_snapshot: Option<Vec<u8>>,
    display_state: Value,
    policy: Policy,
    config: Option<Config>,
    active_profile_id: Option<String>,
    profile_override: Option<ProfileOverride>,
    profiles: Profiles,
    inventory: Option<Inventory>,
    inventory_received: Option<Instant>,
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
    draft_completed: bool,
    warning: Option<String>,
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
        let override_directory = std::env::var_os("PORTKEY_CONFIG_DIR");
        let path = controller_config_path(override_directory);
        let loaded = path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| Profiles::load(p));
        let mut controller = Self::from_store(path, loaded);
        let startup = controller
            .path
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|p| {
                crate::instance::acquire(p.parent().ok_or("Invalid configuration directory")?)
            });
        match startup {
            Ok(instance) => {
                crate::timing::event("session_start", "controller", None, None);
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
    #[cfg(test)]
    fn base(path: Result<PathBuf, String>, loaded: Result<Option<Config>, String>) -> Self {
        Self::from_store(path, loaded.map(tests::test_profiles))
    }
    fn from_store(path: Result<PathBuf, String>, loaded: Result<Profiles, String>) -> Self {
        let loaded = loaded.and_then(|profiles| {
            profiles.validate()?;
            Ok(profiles)
        });
        let (profiles, status, error) = match loaded {
            Ok(profiles) => (profiles, String::new(), false),
            Err(error) => (Profiles::default(), error, true),
        };
        let config = profiles.profiles.first().map(|p| &p.config);
        let language = config
            .as_ref()
            .map(|c| c.language.clone())
            .or_else(|| {
                std::fs::read_to_string(path.as_ref().ok()?.with_file_name("language"))
                    .ok()
                    .filter(|value| matches!(value.as_str(), "en" | "zh-Hans"))
            })
            .unwrap_or_else(|| "en".into());
        let mut policy = Policy::new();
        policy.set_automatic(false, 0);
        let startup_error = if error { Some(status.clone()) } else { None };
        let draft_completed = path.as_ref().is_ok_and(|p| {
            let draft = p.with_file_name("setup-draft-rust.json");
            if std::fs::metadata(&draft).is_ok_and(|m| m.len() > 65536) {
                return false;
            }
            let value = std::fs::read(draft)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
            value
                .as_ref()
                .and_then(|v| v.get("draft_id"))
                .and_then(Value::as_str)
                .is_some_and(|id| {
                    profiles
                        .profiles
                        .iter()
                        .any(|p| p.completed_draft_id.as_deref() == Some(id))
                })
        });
        let warning = draft_completed.then(|| "A completed setup draft remains on disk and cannot be resumed. Quit PortKey, then remove setup-draft-rust.json from the configuration directory.".to_string());
        Self {
            editing: profiles.profiles.is_empty(),
            config: None,
            active_profile_id: None,
            profile_override: None,
            profiles,
            status,
            error,
            language,
            policy,
            path,
            worker: Worker::idle(),
            setup: None,
            setup_action_error: None,
            setup_usb_sample: None,
            draft_snapshot: None,
            display_state: json!({"result":"idle","target":null,"input":null,"visual_confirmed":false}),
            inventory: None,
            inventory_received: None,
            started: Instant::now(),
            last_frame: SystemTime::now(),
            dispatch: Dispatch::default(),
            quitting: false,
            lifecycle_ok: false,
            lifecycle_epoch: crate::lifecycle::snapshot().0,
            lifecycle: crate::lifecycle::snapshot,
            instance: None,
            startup_error,
            draft_completed,
            warning,
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
        self.clear_display_state();
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
            if let Some(setup) = &mut self.setup {
                setup.interrupted();
            }
            self.reset();
            self.inventory = None;
            self.worker.refresh();
        }
        if self.last_frame.elapsed().unwrap_or(Duration::MAX) > Duration::from_secs(5) {
            if let Some(setup) = &mut self.setup {
                setup.interrupted();
            }
            self.reset();
            self.worker.refresh();
        }
        self.last_frame = SystemTime::now();
        let (inventory, completion, probe_completion, resumed, usb_sample) = {
            let mut s = self.worker.shared.0.lock().unwrap();
            let resumed = std::mem::take(&mut s.resumed);
            (
                s.inventory.take(),
                s.completion.take(),
                s.probe_completion.take(),
                resumed,
                s.usb_observer
                    .then_some((s.inventory_revision, s.usb_sequence)),
            )
        };
        if resumed {
            if let Some(setup) = &mut self.setup {
                setup.interrupted();
            }
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
                    if let Some(setup) = &mut self.setup {
                        if setup.display_id.is_empty() {
                            // Naming and joining happen before explicit monitor selection.
                        } else if inventory
                            .displays
                            .iter()
                            .filter(|d| d.id == setup.display_id)
                            .count()
                            == 1
                        {
                            if usb_sample.is_none() || usb_sample != self.setup_usb_sample {
                                self.setup_usb_sample = usb_sample;
                                if setup.phase == "usb_capturing" {
                                    if let Err(error) = setup.start_usb(&inventory.hubs) {
                                        setup.phase = "usb_retry".into();
                                        setup.fail(&error);
                                    }
                                } else {
                                    setup.observe(&inventory.hubs, now);
                                }
                            }
                        } else if setup.expected_departure()
                            && !inventory.displays.iter().any(|d| d.id == setup.display_id)
                        {
                            if usb_sample.is_none() || usb_sample != self.setup_usb_sample {
                                self.setup_usb_sample = usb_sample;
                                setup.observe(&inventory.hubs, now);
                            }
                        } else {
                            self.worker.invalidate();
                            setup.interrupted();
                            setup.fail("Selected display disconnected or became ambiguous. Reconnect it and restart USB detection.");
                        }
                    }
                    self.inventory_received = Some(Instant::now());
                    let (revision, split_usb) = {
                        let state = self.worker.shared.0.lock().unwrap();
                        (state.inventory_revision, state.usb_observer)
                    };
                    self.select_profile(&inventory);
                    let profile_reset = split_usb
                        && self.worker.shared.0.lock().unwrap().inventory_revision != revision;
                    if !self.editing && !sleeping && self.lifecycle_ok && !profile_reset {
                        if let Some(c) = self.config.clone() {
                            let ownership = c.observe(&inventory.hubs);
                            crate::timing::event(
                                "ownership_observed",
                                match ownership {
                                    Ownership::Owned => "owned",
                                    Ownership::Absent => "absent",
                                    Ownership::Partial => "partial",
                                    Ownership::Unknown => "unknown",
                                },
                                None,
                                None,
                            );
                            if ownership != self.policy.ownership() {
                                // A later scan remains fresh when this observation cancels old commands.
                                self.clear_display_state();
                                self.worker.cancel_commands();
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
                    self.clear_display_state();
                    self.inventory = None;
                    self.inventory_received = None;
                    if let Some(setup) = &mut self.setup {
                        setup.interrupted();
                    }
                    if !self.editing {
                        self.config = None;
                        self.active_profile_id = None;
                    }
                    self.policy.observe(Ownership::Unknown, now);
                    self.worker.invalidate();
                    self.status = e;
                    self.error = true;
                }
            }
        }
        if let Some((_, result)) = probe_completion.filter(|(generation, _)| {
            self.worker
                .shared
                .0
                .lock()
                .unwrap()
                .jobs
                .current(*generation)
        }) {
            if let Some(setup) = &mut self.setup {
                setup.input_result(result);
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
                    self.display_state["result"] = json!("readback_confirmed");
                    self.status = self
                        .text(
                            "Monitor reported the requested input. Confirm the picture on screen.",
                            "显示器已回报目标输入，请观察画面确认。",
                        )
                        .into();
                    self.error = false;
                }
                Ok(WriteOutcome::SentUnverified(detail)) => {
                    self.display_state["result"] = json!("sent_unverified");
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
                    self.display_state["result"] = json!("failed");
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
                        self.pending_display_state(&config, request.target);
                        self.worker.submit(
                            config,
                            request.target,
                            Some(expected),
                            self.profile_ids(),
                        );
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
        if self.setup.is_some() {
            if let Err(error) = self.persist_draft() {
                self.status = error;
                self.error = true;
            }
        }
        let inventory = self
            .inventory
            .as_ref()
            .map(|i| json!({"displays":i.displays,"hubs":i.hubs}));
        if self.draft_completed && self.draft_path().is_ok_and(|p| !p.exists()) {
            self.warning = None;
        }
        let mut status = self.startup_error.as_ref().unwrap_or(&self.status).clone();
        if self.startup_error.is_none()
            && !self.editing
            && self.lifecycle_ok
            && !(self.lifecycle)().1
            && self.policy.automatic()
            && self.policy.ownership() == Ownership::Absent
            && !self.policy.has_established_ownership()
        {
            let waiting = self.text(
                "Automation is waiting to observe both enrolled USB hubs on this computer. Startup absence does not switch the display.",
                "自动切换正在等待本机检测到两个已配置的 USB 集线器。启动时设备缺失不会触发切换。",
            );
            if !status.is_empty() {
                status.push(' ');
            }
            status.push_str(waiting);
        }
        if let Some(warning) = &self.warning {
            status = format!(
                "{} {}",
                crate::messages::display(warning, self.language == "zh-Hans"),
                crate::messages::display(&status, self.language == "zh-Hans")
            )
            .trim()
            .to_owned();
        }
        let mut setup = json!(self.setup);
        if let Some(message) = setup.get("message").and_then(Value::as_str) {
            setup["message"] = json!(crate::messages::display(
                message,
                self.language == "zh-Hans"
            ));
        }
        json!({
            "config": self.config, "inventory": inventory,
            "profiles": self.profiles.profiles.iter().map(|p| json!({"profile_id":p.profile_id,"name":p.name,"display_id":p.config.display_id,"workspace":p.workspace.as_ref().map(|w| w.public()),"local_input":p.config.local_input,"remote_input":p.config.remote_input,"hub_count":p.config.hubs.len()})).collect::<Vec<_>>(),
            "workspace": self.workspace_snapshot(),
            "display_state": self.display_state,
            "draft_available": !self.draft_completed && self.draft_path().is_ok_and(|p| p.exists()),
            "warning": self.warning.as_ref().map(|w| crate::messages::display(w, self.language == "zh-Hans")),
            "active_profile_id": if self.editing { None } else { self.active_profile_id.as_ref() },
            "profile_selection": self.profile_selection_snapshot(),
            "active_display_id": if self.editing { None } else { self.config.as_ref().map(|c| &c.display_id) },
            "profile_name": self.profile_name(),
            "ownership": match self.policy.ownership() { Ownership::Unknown => "unknown", Ownership::Owned => "owned", Ownership::Absent => "absent", Ownership::Partial => "partial" },
            "status": crate::messages::display(&status, self.language == "zh-Hans"),
            "error": self.error || self.startup_error.is_some(),
            "startup_error": self.startup_error.as_ref().map(|e| crate::messages::display(e, self.language == "zh-Hans")),
            "controls_available": self.startup_error.is_none() && !self.quitting,
            "lifecycle_ok": self.lifecycle_ok, "sleeping": (self.lifecycle)().1,
            "language": self.language, "config_path": self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            "editing": self.editing, "setup": setup
        })
    }
    pub fn action(&mut self, value: Value) -> Value {
        let setup_action = value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|action| action.starts_with("setup_"));
        match self.perform(value) {
            Ok(extra) => {
                if setup_action && self.setup.is_some() {
                    if let Err(error) = self.persist_draft() {
                        self.status = error.clone();
                        self.error = true;
                        return json!({"ok":false,"error":crate::messages::display(&error, self.language == "zh-Hans")});
                    }
                }
                if setup_action {
                    if let Some(previous) = self.setup_action_error.take() {
                        // Only retire the error produced by the setup action, not a later hardware failure.
                        if self.status == previous {
                            self.status = self
                                .setup
                                .as_ref()
                                .map(|s| s.message.clone())
                                .unwrap_or_default();
                            self.error = false;
                        }
                    }
                }
                extra
            }
            Err(error) => {
                self.setup_action_error = setup_action.then(|| error.clone());
                if let Some(setup) = &mut self.setup {
                    setup.fail(&error);
                }
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
            if !matches!(action, Action::DiagnosticsPreview) {
                return Err(error.clone());
            }
        }
        match action {
            Action::SetupBegin {
                display_id,
                name,
                target_profile_id,
            } => {
                let inventory = self
                    .inventory
                    .as_ref()
                    .ok_or("Wait for display discovery first.")?;
                if inventory
                    .displays
                    .iter()
                    .filter(|d| d.id == display_id)
                    .count()
                    != 1
                {
                    return Err("Choose one connected display first.".into());
                }
                if name.trim().is_empty() || name.chars().count() > 80 {
                    return Err("Enter a desk name from 1 to 80 characters.".into());
                }
                let target = if let Some(id) = target_profile_id {
                    self.require_profile_id(&id)?;
                    Some(id)
                } else {
                    self.profiles
                        .unique_for_display(&display_id)?
                        .map(|p| p.profile_id.clone())
                };
                let mut setup = crate::setup::Setup::new(display_id, name.trim().into());
                if let Some(id) = target {
                    setup.profile_id = id.clone();
                    setup.target_profile_id = Some(id);
                }
                self.reset();
                self.policy
                    .set_automatic(false, self.started.elapsed().as_millis() as u64);
                self.editing = true;
                self.setup = Some(setup);
            }
            Action::SetupUsbStart => {
                if !self.lifecycle_ok || (self.lifecycle)().1 {
                    return Err(
                        "Restore power monitoring and wake this computer before setup.".into(),
                    );
                }
                self.worker.invalidate();
                if self
                    .inventory_received
                    .is_none_or(|time| time.elapsed() > Duration::from_secs(5))
                {
                    return Err("Wait for fresh USB discovery before starting detection.".into());
                }
                let inventory = self
                    .inventory
                    .as_ref()
                    .ok_or("Wait for USB discovery first.")?;
                let setup = self.setup.as_mut().ok_or("Choose a display first.")?;
                if inventory
                    .displays
                    .iter()
                    .filter(|d| d.id == setup.display_id)
                    .count()
                    != 1
                {
                    return Err(
                        "Reconnect the selected display before starting USB detection.".into(),
                    );
                }
                setup.capture_usb();
                self.worker.refresh();
            }
            Action::SetupReadLocal | Action::SetupReadRemote => {
                let remote = matches!(action, Action::SetupReadRemote);
                if !self.lifecycle_ok || (self.lifecycle)().1 {
                    return Err(
                        "Restore power monitoring and wake this computer before reading its input."
                            .into(),
                    );
                }
                let setup = self.setup.as_mut().ok_or("Start setup first.")?;
                if !matches!(setup.phase.as_str(), "usb_ready" | "exchange" | "ready") {
                    return Err("Complete USB detection first.".into());
                }
                setup.phase = "reading_input".into();
                setup.message = "Reading the selected monitor's current input…".into();
                setup.error = false;
                setup.ready = false;
                setup.input_role = if remote { "remote" } else { "local" }.into();
                if remote {
                    setup.remote_input = None;
                } else {
                    setup.local_input = None;
                }
                setup.workspace.binding = None;
                setup.workspace.verification = "not_checked".into();
                setup.code = None;
                self.worker
                    .probe(setup.display_id.clone(), (self.lifecycle)().0);
            }
            Action::SetupPeerCode { code } => {
                self.setup
                    .as_mut()
                    .ok_or("Start setup first.")?
                    .peer_code(&code)?;
            }
            Action::SetupFinish { enable } => {
                let setup = self.setup.as_ref().ok_or("Start setup first.")?;
                let mut config = setup.config(&self.language)?;
                let mut workspace = setup.workspace.clone();
                if !setup.product_configured {
                    workspace.verification = "not_checked".into();
                }
                if enable && !workspace.can_enable(&config) {
                    return Err(
                        "Qualify this controller and both directions before enabling automation."
                            .into(),
                    );
                }
                config.automatic = enable;
                let mut next = self.profiles.clone();
                if let Some(id) = &setup.target_profile_id {
                    self.require_profile_id(id)?;
                }
                next.upsert_id(
                    setup.profile_id.clone(),
                    Some(setup.name.clone()),
                    config.clone(),
                )?;
                next.preferred_profiles
                    .insert(config.display_id.clone(), setup.profile_id.clone());
                let profile = next
                    .profiles
                    .iter_mut()
                    .find(|p| p.profile_id == setup.profile_id)
                    .unwrap();
                profile.workspace = Some(workspace);
                profile.completed_draft_id = Some(setup.draft_id.clone());
                self.reset();
                next.save(self.path.as_ref().map_err(Clone::clone)?)?;
                self.profiles = next;
                self.profile_override = None;
                self.setup = None;
                self.draft_completed = true;
                self.warning = self.remove_draft().err().map(|error| format!("Workspace saved; completed draft cleanup needs attention. Quit PortKey, then remove setup-draft-rust.json from the configuration directory. {error}"));
                self.editing = false;
                self.config = None;
                self.active_profile_id = None;
                if let Some(inventory) = self.inventory.clone() {
                    self.select_profile(&inventory);
                }
                self.error = false;
                self.status = self.text("Workspace saved.", "工作区已保存。").into();
                self.worker.refresh();
                return Ok(
                    json!({"ok":true,"warning":self.warning.as_ref().map(|w| crate::messages::display(w, self.language == "zh-Hans"))}),
                );
            }
            Action::SetupCancel => {
                self.remove_draft()?;
                self.setup = None;
                return self.perform(json!({"type":"edit","enabled":false}));
            }
            Action::SetupJoin { record, name } => {
                let setup =
                    crate::setup::Setup::join(&record, name.unwrap_or_else(|| "Workspace".into()))?;
                self.reset();
                self.editing = true;
                self.setup = Some(setup);
                self.policy
                    .set_automatic(false, self.started.elapsed().as_millis() as u64);
            }
            Action::SetupDraftBegin {
                name,
                workspace,
                target_profile_id,
            } => {
                crate::product::validate_name(&name)?;
                let mut setup = crate::setup::Setup::new(String::new(), name.trim().into());
                setup.edit_workspace(workspace)?;
                if let Some(id) = target_profile_id {
                    self.require_profile_id(&id)?;
                    setup.profile_id = id.clone();
                    setup.target_profile_id = Some(id);
                }
                self.reset();
                self.editing = true;
                self.setup = Some(setup);
                self.policy
                    .set_automatic(false, self.started.elapsed().as_millis() as u64);
            }
            Action::SetupSelectDisplay { display_id } => {
                if self
                    .inventory
                    .as_ref()
                    .is_none_or(|i| i.displays.iter().filter(|d| d.id == display_id).count() != 1)
                {
                    return Err("Choose one connected display first.".into());
                }
                let setup = self.setup.as_mut().ok_or("Start setup first.")?;
                setup.restart_draft();
                setup.display_id = display_id;
                setup.error = false;
                setup.message =
                    "Monitor selected. Keep USB on this computer, then start USB detection.".into();
                self.worker.invalidate();
                self.worker.refresh();
            }
            Action::SetupWorkspace { workspace, name } => {
                if let Some(name) = &name {
                    crate::product::validate_name(name)?;
                }
                let setup = self.setup.as_mut().ok_or("Start setup first.")?;
                setup.edit_workspace(workspace)?;
                if let Some(name) = name {
                    setup.name = name.trim().into();
                }
            }
            Action::SetupExport => {
                let record = self.setup.as_ref().ok_or("Start setup first.")?.export()?;
                return Ok(json!({"ok":true,"record":record}));
            }
            Action::SetupPeerPreview { record } => {
                let preview = self
                    .setup
                    .as_mut()
                    .ok_or("Start setup first.")?
                    .preview(&record)?;
                return Ok(json!({"ok":true,"preview":preview}));
            }
            Action::SetupPeerConfirm => self
                .setup
                .as_mut()
                .ok_or("Start setup first.")?
                .confirm_peer()?,
            Action::SetupEvidence {
                evidence,
                verification,
                peer_paused_confirmed,
            } => {
                let setup = self.setup.as_mut().ok_or("Start setup first.")?;
                let config = setup.config(&self.language)?;
                let mut next = setup.workspace.edit(json!({"evidence":evidence,"verification":verification,"peer_paused_confirmed":peer_paused_confirmed}))?;
                next.attest(&config)?;
                setup.workspace = next;
                setup.product_configured = true;
                setup.error = false;
            }
            Action::SetupSuspend => {
                self.persist_draft()?;
                self.setup = None;
                return self.perform(json!({"type":"edit","enabled":false}));
            }
            Action::SetupResume => {
                if self.draft_completed {
                    return Err("This setup draft was already saved and cannot be resumed. Start a new setup to make changes.".into());
                }
                let setup = crate::setup::Setup::load_draft(&self.draft_path()?)?;
                if let Some(id) = &setup.target_profile_id {
                    self.require_profile_id(id)?;
                }
                self.reset();
                self.policy
                    .set_automatic(false, self.started.elapsed().as_millis() as u64);
                self.editing = true;
                self.setup = Some(setup);
                self.worker.refresh();
            }
            Action::WorkspaceSave {
                display_id,
                profile_name,
                workspace,
                target_profile_id,
            } => {
                let id =
                    self.metadata_target(target_profile_id.as_deref(), display_id.as_deref())?;
                let mut next = self.profiles.clone();
                let profile = next
                    .profiles
                    .iter_mut()
                    .find(|p| p.profile_id == id)
                    .unwrap();
                let original = profile.workspace.clone().unwrap_or_default();
                let updated = original.edit(workspace)?;
                if let Some(name) = profile_name {
                    crate::product::validate_name(&name)?;
                    profile.name = name.trim().into();
                }
                if !updated.can_enable(&profile.config) {
                    profile.config.automatic = false;
                }
                profile.workspace = Some(updated);
                next.save(self.path.as_ref().map_err(Clone::clone)?)?;
                self.profiles = next;
                if self.active_profile_id.as_ref() == Some(&id) {
                    self.reset();
                    self.config = None;
                    self.active_profile_id = None;
                    if let Some(inventory) = self.inventory.clone() {
                        self.select_profile(&inventory);
                    }
                    self.worker.refresh();
                }
            }
            Action::ProfileSelect { profile_id } => {
                if self.editing || self.setup.is_some() {
                    return Err("Finish or suspend setup before choosing a profile.".into());
                }
                let display = self
                    .require_profile_id(&profile_id)?
                    .config
                    .display_id
                    .clone();
                let mut next = self.profiles.clone();
                next.preferred_profiles.insert(display, profile_id.clone());
                self.reset();
                next.save(self.path.as_ref().map_err(Clone::clone)?)?;
                self.profiles = next;
                self.profile_override = Some(ProfileOverride {
                    profile_id,
                    was_present: false,
                });
                self.config = None;
                self.active_profile_id = None;
                if let Some(inventory) = self.inventory.clone() {
                    self.select_profile(&inventory);
                }
                self.worker.refresh();
            }
            Action::ProfileRestoreAuto => {
                if self.editing || self.setup.is_some() {
                    return Err("Finish or suspend setup before choosing a profile.".into());
                }
                self.reset();
                self.profile_override = None;
                self.config = None;
                self.active_profile_id = None;
                if let Some(inventory) = self.inventory.clone() {
                    self.select_profile(&inventory);
                }
                self.worker.refresh();
            }
            Action::DiagnosticsPreview => {
                let workspace = self.workspace_snapshot();
                let current = json!({
                    "mode":workspace["mode"],"verification":workspace["verification"],"local_os":std::env::consts::OS,"remote_os":workspace["remote_os"],"local_role":workspace["local_role"],
                    "automatic_requested":self.config.as_ref().is_some_and(|c| c.automatic),"effective_state":workspace["effective_state"],
                    "ownership":match self.policy.ownership() { Ownership::Unknown=>"unknown",Ownership::Owned=>"owned",Ownership::Absent=>"absent",Ownership::Partial=>"partial" },
                    "lifecycle_ok":self.lifecycle_ok,"sleeping":(self.lifecycle)().1,"editing":self.editing,"enrolled":self.config.is_some(),"profile_count":self.profiles.profiles.len(),"selected_hub_count":self.config.as_ref().map_or(0,|c| c.hubs.len()),
                    "qualification_current":workspace["qualification_current"],"display_result":self.display_state["result"],"startup_blocked":self.startup_error.is_some(),"cleanup_warning":self.warning.is_some()
                });
                let path = std::env::var_os("PORTKEY_TIMING_LOG").map(PathBuf::from);
                return Ok(
                    json!({"ok":true,"diagnostics":crate::diagnostics::preview(current,path.as_deref())}),
                );
            }
            Action::Refresh => self.worker.refresh(),
            Action::UsbRefresh => {
                crate::timing::event("device_change", "usb", None, None);
                self.worker.refresh_usb();
            }
            Action::UsbNotifications { ready } => crate::timing::event(
                if ready {
                    "usb_notifications_ready"
                } else {
                    "usb_notifications_unavailable"
                },
                "macos",
                None,
                None,
            ),
            Action::New => {
                self.setup = None;
                self.reset();
                self.editing = true;
                self.config = None;
                self.active_profile_id = None;
                self.status = self
                    .text(
                        "Enroll a new setup; saved setups are preserved.",
                        "请配置新环境，已有配置会保留。",
                    )
                    .into();
                self.error = false;
                self.worker.refresh();
            }
            Action::Edit { enabled } => {
                self.setup = None;
                self.reset();
                self.editing = enabled;
                if !enabled {
                    self.config = None;
                    self.active_profile_id = None;
                    if let Some(inventory) = self.inventory.clone() {
                        self.select_profile(&inventory);
                    }
                }
                self.worker.refresh();
            }
            Action::Save {
                config,
                profile_name,
                target_profile_id,
            } => {
                self.validate_config(&config)?;
                let mut next = self.profiles.clone();
                let id = if let Some(id) = target_profile_id {
                    self.require_profile_id(&id)?;
                    id
                } else {
                    next.unique_for_display(&config.display_id)?
                        .map(|p| p.profile_id.clone())
                        .unwrap_or_else(crate::profiles::new_profile_id)
                };
                let existing = next.profiles.iter().find(|p| p.profile_id == id);
                let replace_trust =
                    existing.is_none_or(|p| !crate::product::same_binding(&p.config, &config));
                let workspace = if replace_trust {
                    let mut unverified = existing
                        .and_then(|p| p.workspace.clone())
                        .unwrap_or_default();
                    unverified.verification = "not_checked".into();
                    unverified.binding = None;
                    unverified.evidence = crate::product::Evidence::default();
                    unverified.peer_confirmed = false;
                    unverified.peer_paused_confirmed = false;
                    Some(unverified)
                } else {
                    Some(
                        existing
                            .and_then(|p| p.workspace.clone())
                            .unwrap_or_default(),
                    )
                };
                if config.automatic && workspace.as_ref().is_some_and(|w| !w.can_enable(&config)) {
                    return Err("Qualify both directions before enabling automation.".into());
                }
                next.upsert_id(id.clone(), profile_name, config.clone())?;
                next.profiles
                    .iter_mut()
                    .find(|p| p.profile_id == id)
                    .unwrap()
                    .workspace = workspace;
                // Stop old work before persistence; never alter an unrelated saved profile.
                self.reset();
                self.policy
                    .set_automatic(false, self.started.elapsed().as_millis() as u64);
                self.editing = true;
                next.save(self.path.as_ref().map_err(Clone::clone)?)?;
                self.profiles = next;
                self.language = config.language.clone();
                self.config = None;
                self.active_profile_id = None;
                self.editing = false;
                if let Some(inventory) = self.inventory.clone() {
                    self.select_profile(&inventory);
                }
                self.worker.refresh();
                self.status = self
                    .text(
                        "Profile saved. Waiting for fresh USB evidence.",
                        "桌面配置已保存，正在等待新的 USB 状态。",
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
                self.pending_display_state(&config, target.into());
                self.worker
                    .submit(config, target.into(), None, self.profile_ids());
                self.status = self
                    .text("Switch request queued…", "切换请求已排队…")
                    .into();
                self.error = false;
            }
            Action::Automatic { enabled } => {
                if !enabled {
                    self.pause();
                    self.profiles
                        .save(self.path.as_ref().map_err(Clone::clone)?)
                        .map_err(|e| format!("Paused for this session; could not save. {e}"))?;
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
                    return self.perform(json!({"type":"save","config":config,"target_profile_id":self.active_profile_id}));
                }
            }
            Action::Language { value } => {
                let language = match value {
                    Language::English => "en",
                    Language::Chinese => "zh-Hans",
                };
                let mut next = self.profiles.clone();
                for profile in &mut next.profiles {
                    profile.config.language = language.into();
                }
                next.save(self.path.as_ref().map_err(Clone::clone)?)?;
                let path = self
                    .path
                    .as_ref()
                    .map_err(Clone::clone)?
                    .with_file_name("language");
                std::fs::write(path, language)
                    .map_err(|e| format!("Cannot save language preference: {e}"))?;
                self.profiles = next;
                if let Some(config) = &mut self.config {
                    config.language = language.into();
                }
                self.language = language.into();
                // Statuses are generated in the active language; discard obsolete UI text.
                if !self.error {
                    self.status.clear();
                }
            }

            Action::Quit => unreachable!(),
        }
        Ok(json!({"ok":true}))
    }
    fn clear_display_state(&mut self) {
        self.display_state =
            json!({"result":"idle","target":null,"input":null,"visual_confirmed":false});
    }
    fn pending_display_state(&mut self, config: &Config, target: Target) {
        self.display_state = json!({"result":"pending","target":if target == Target::Local { "local" } else { "remote" },"input":config.input(target),"visual_confirmed":false});
    }
    fn draft_path(&self) -> Result<PathBuf, String> {
        Ok(self
            .path
            .as_ref()
            .map_err(Clone::clone)?
            .with_file_name("setup-draft-rust.json"))
    }
    fn remove_draft(&self) -> Result<(), String> {
        match std::fs::remove_file(self.draft_path()?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Cannot remove setup draft: {e}")),
        }
    }
    fn persist_draft(&mut self) -> Result<(), String> {
        let path = self.draft_path()?;
        if let Some(setup) = &mut self.setup {
            setup.draft_saved = true;
            let bytes = serde_json::to_vec(setup).map_err(|e| e.to_string())?;
            if self.draft_snapshot.as_ref() != Some(&bytes) || !path.exists() {
                if let Err(error) = setup.save_draft(&path) {
                    setup.draft_saved = false;
                    return Err(error);
                }
                self.draft_snapshot = Some(bytes);
                self.draft_completed = false;
                self.warning = None;
            }
        }
        Ok(())
    }
    fn workspace_snapshot(&self) -> Value {
        let Some(config) = &self.config else {
            return Value::Null;
        };
        let Some(profile) = self
            .profiles
            .profiles
            .iter()
            .find(|p| Some(&p.profile_id) == self.active_profile_id.as_ref())
        else {
            return Value::Null;
        };
        let workspace = profile.workspace.clone().unwrap_or_default();
        let can_enable = workspace.can_enable(config);
        let state = if self.editing {
            "editing"
        } else if (self.lifecycle)().1 {
            "sleeping"
        } else if !self.lifecycle_ok || self.startup_error.is_some() || self.inventory.is_none() {
            "unavailable"
        } else if !config.automatic {
            "paused"
        } else if !can_enable || self.policy.ownership() == Ownership::Partial {
            "attention"
        } else if self.policy.ownership() == Ownership::Unknown
            || (self.policy.ownership() == Ownership::Absent
                && !self.policy.has_established_ownership())
        {
            "waiting_usb"
        } else {
            "active"
        };
        let mut value = workspace.public();
        value["automatic_requested"] = json!(config.automatic);
        value["effective_state"] = json!(state);
        value["peer_runtime"] = json!("unknown");
        value["can_enable"] = json!(can_enable);
        value["qualification_current"] = json!(workspace.current(config));
        value
    }
    fn validate_config(&self, config: &Config) -> Result<(), String> {
        config.validate()?;
        if config.platform != std::env::consts::OS {
            return Err("Configuration belongs to another platform; enroll this computer.".into());
        }
        Ok(())
    }
    fn pause(&mut self) {
        self.clear_display_state();
        let mut shared = self.worker.shared.0.lock().unwrap();
        crate::runtime::pause(
            &mut self.policy,
            &mut shared.jobs,
            self.started.elapsed().as_millis() as u64,
        );
        shared.completion = None;
        shared.probe = None;
        shared.probe_completion = None;
        shared.invalidate_inventory();
        let active = self.active_profile_id.clone();
        for profile in &mut self.profiles.profiles {
            if active.as_ref().is_some_and(|id| *id == profile.profile_id) {
                profile.config.automatic = false;
            }
        }
        if let Some(config) = &mut self.config {
            config.automatic = false;
        }
    }
    fn require_profile_id(&self, id: &str) -> Result<&crate::profiles::Profile, String> {
        self.profiles
            .profiles
            .iter()
            .find(|p| p.profile_id == id)
            .ok_or_else(|| "Unknown saved connection profile.".into())
    }
    fn metadata_target(
        &self,
        target: Option<&str>,
        display: Option<&str>,
    ) -> Result<String, String> {
        if let Some(id) = target {
            let profile = self.require_profile_id(id)?;
            if display.is_some_and(|d| d != profile.config.display_id) {
                return Err("Profile and display selectors disagree.".into());
            }
            return Ok(id.into());
        }
        if let Some(display) = display {
            return self
                .profiles
                .unique_for_display(display)?
                .map(|p| p.profile_id.clone())
                .ok_or_else(|| "Unknown saved workspace.".into());
        }
        self.active_profile_id
            .clone()
            .ok_or_else(|| "Choose a saved workspace first.".into())
    }
    fn profile_ids(&self) -> Vec<String> {
        if let Some(selected) = &self.profile_override {
            return self
                .require_profile_id(&selected.profile_id)
                .map(|p| vec![p.config.display_id.clone()])
                .unwrap_or_default();
        }
        self.profiles
            .profiles
            .iter()
            .map(|p| p.config.display_id.clone())
            .collect()
    }
    fn profile_name(&self) -> &str {
        self.active_profile_id
            .as_ref()
            .and_then(|id| self.profiles.profiles.iter().find(|p| &p.profile_id == id))
            .map_or("", |p| p.name.as_str())
    }
    fn profile_selection_snapshot(&self) -> Value {
        let Some(selected) = &self.profile_override else {
            let connected = self.inventory.as_ref().map(|i| {
                i.displays
                    .iter()
                    .filter(|d| {
                        self.profiles
                            .profiles
                            .iter()
                            .any(|p| p.config.display_id == d.id)
                    })
                    .count()
            });
            let (availability, reason, message) = if self.editing {
                (
                    "editing",
                    "editing_active",
                    self.text(
                        "Profile selection is paused while editing setup.",
                        "编辑设置期间暂停选择连接配置。",
                    ),
                )
            } else if connected.is_none() {
                (
                    "unavailable",
                    "discovery_unavailable",
                    self.text(
                        "Waiting for successful display discovery.",
                        "正在等待显示器发现成功。",
                    ),
                )
            } else if connected == Some(0) {
                (
                    "waiting",
                    "no_enrolled_display",
                    self.text(
                        "No enrolled display connected. Connect a saved display or add a new desk.",
                        "未连接已保存的显示器。请连接显示器或添加新桌面。",
                    ),
                )
            } else if connected.is_some_and(|count| count > 1) {
                (
                    "ambiguous",
                    "multiple_displays",
                    self.text(
                        "Multiple enrolled displays are connected. Choose a profile explicitly.",
                        "连接了多台已配置的显示器，请明确选择连接配置。",
                    ),
                )
            } else if self.active_profile_id.is_none() {
                ("ambiguous","choose_profile",self.text("Multiple connection profiles match this display. Choose a profile explicitly.","此显示器有多个连接配置，请明确选择。"))
            } else {
                (
                    "available",
                    "automatic_detection",
                    self.text(
                        "Automatically recognize the connected display and its saved profile.",
                        "自动识别连接的显示器及其已保存的连接配置。",
                    ),
                )
            };
            return json!({"mode":"auto","selected_profile_id":null,"selected_display_id":null,"selected_profile_name":null,"availability":availability,"reason":reason,"message":message});
        };
        let profile = self
            .profiles
            .profiles
            .iter()
            .find(|p| p.profile_id == selected.profile_id);
        let count = self.inventory.as_ref().map(|i| {
            profile.map_or(0, |p| {
                i.displays
                    .iter()
                    .filter(|d| d.id == p.config.display_id)
                    .count()
            })
        });
        let (availability, reason, message) = if self.editing {
            (
                "editing",
                "editing_active",
                self.text(
                    "Profile selection is paused while editing setup.",
                    "编辑设置期间暂停选择连接配置。",
                ),
            )
        } else {
            match count {
            None=>("unavailable","discovery_unavailable",self.text("Waiting for successful display discovery; the manual profile choice is retained.","正在等待显示器发现成功；手动选择的连接配置已保留。")),
            Some(0)=>("waiting","selected_display_missing",self.text("Waiting for the selected profile's enrolled display. No other display will be controlled.","正在等待所选配置绑定的显示器；不会控制其他显示器。")),
            Some(1)=>("available","selected_display_present",self.text("Using the explicitly selected connection profile.","正在使用明确选择的连接配置。")),
            _=>("ambiguous","selected_display_ambiguous",self.text("The selected display identity is ambiguous. No display will be controlled.","所选显示器标识不明确，不会控制任何显示器。")),
        }
        };
        json!({"mode":"manual","selected_profile_id":selected.profile_id,"selected_display_id":profile.map(|p|&p.config.display_id),"selected_profile_name":profile.map(|p|&p.name),"availability":availability,"reason":reason,"message":message})
    }
    fn select_profile(&mut self, inventory: &Inventory) {
        if self.editing {
            return;
        }
        if let Some(selected) = &mut self.profile_override {
            if let Some(profile) = self
                .profiles
                .profiles
                .iter()
                .find(|p| p.profile_id == selected.profile_id)
            {
                let count = inventory
                    .displays
                    .iter()
                    .filter(|d| d.id == profile.config.display_id)
                    .count();
                if count == 1 {
                    selected.was_present = true;
                } else if count == 0 && selected.was_present {
                    self.profile_override = None;
                }
            }
        }
        let selected = if let Some(manual) = &self.profile_override {
            self.require_profile_id(&manual.profile_id).and_then(|p|match inventory.displays.iter().filter(|d|d.id == p.config.display_id).count(){0=>Ok(None),1=>Ok(Some(p)),_=>Err("The selected display identity is ambiguous. No display will be controlled.".into())})
        } else {
            self.profiles.selected(&inventory.displays)
        };
        let selected = selected.map(|p| p.cloned());
        let next = selected.as_ref().ok().cloned().flatten();
        let next_id = next.as_ref().map(|p| p.profile_id.clone());
        let next_config = next.as_ref().map(|p| p.config.clone());
        if self.active_profile_id != next_id || self.config != next_config {
            self.reset();
            self.policy.set_automatic(
                next.as_ref().is_some_and(|p| {
                    p.config.automatic
                        && p.workspace
                            .as_ref()
                            .is_some_and(|w| w.can_enable(&p.config))
                }),
                self.started.elapsed().as_millis() as u64,
            );
            self.active_profile_id = next_id;
            self.config = next_config;
            self.error = false;
            self.status.clear();
        }
        if let Err(error) = selected {
            self.status = crate::messages::display(&error, self.language == "zh-Hans").into_owned();
            self.error = true;
        } else if self.config.is_none() && !self.profiles.profiles.is_empty() {
            self.status=if self.profile_override.is_some(){self.text("Waiting for the selected profile's enrolled display. No other display will be controlled.","正在等待所选配置绑定的显示器；不会控制其他显示器。")}else{self.text("No enrolled display connected. Connect a saved display or add a new desk.","未连接已保存的显示器。请连接显示器或添加新桌面。")}.into();
            self.error = false;
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Action {
    #[serde(rename = "setup_begin")]
    SetupBegin {
        display_id: String,
        name: String,
        #[serde(default)]
        target_profile_id: Option<String>,
    },
    #[serde(rename = "setup_usb_start")]
    SetupUsbStart,
    #[serde(rename = "setup_read_local")]
    SetupReadLocal,
    #[serde(rename = "setup_peer_code")]
    SetupPeerCode {
        code: String,
    },
    #[serde(rename = "setup_finish")]
    SetupFinish {
        #[serde(default)]
        enable: bool,
    },
    #[serde(rename = "workspace_save")]
    WorkspaceSave {
        #[serde(default)]
        target_profile_id: Option<String>,
        #[serde(default)]
        display_id: Option<String>,
        #[serde(default)]
        profile_name: Option<String>,
        workspace: Value,
    },
    #[serde(rename = "setup_join")]
    SetupJoin {
        record: String,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "setup_draft_begin")]
    SetupDraftBegin {
        name: String,
        workspace: Value,
        #[serde(default)]
        target_profile_id: Option<String>,
    },
    #[serde(rename = "setup_select_display")]
    SetupSelectDisplay {
        display_id: String,
    },
    #[serde(rename = "setup_workspace")]
    SetupWorkspace {
        workspace: Value,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "setup_read_remote")]
    SetupReadRemote,
    #[serde(rename = "setup_export")]
    SetupExport,
    #[serde(rename = "setup_peer_preview")]
    SetupPeerPreview {
        record: String,
    },
    #[serde(rename = "setup_peer_confirm")]
    SetupPeerConfirm,
    #[serde(rename = "setup_evidence")]
    SetupEvidence {
        evidence: crate::product::Evidence,
        verification: String,
        #[serde(default)]
        peer_paused_confirmed: bool,
    },
    #[serde(rename = "setup_suspend")]
    SetupSuspend,
    #[serde(rename = "setup_resume")]
    SetupResume,
    #[serde(rename = "setup_cancel")]
    SetupCancel,
    #[serde(rename = "profile_select")]
    ProfileSelect {
        profile_id: String,
    },
    #[serde(rename = "profile_restore_auto")]
    ProfileRestoreAuto,
    #[serde(rename = "diagnostics_preview")]
    DiagnosticsPreview,
    Refresh,
    #[serde(rename = "usb_refresh")]
    UsbRefresh,
    #[serde(rename = "usb_notifications")]
    UsbNotifications {
        ready: bool,
    },
    Save {
        config: Config,
        #[serde(default)]
        target_profile_id: Option<String>,
        #[serde(default)]
        profile_name: Option<String>,
    },
    New,
    Manual {
        target: NativeTarget,
    },
    Automatic {
        enabled: bool,
    },
    Language {
        value: Language,
    },
    Edit {
        enabled: bool,
    },

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

    pub(super) fn test_profiles(config: Option<Config>) -> Profiles {
        let mut profiles = Profiles::default();
        if let Some(config) = config {
            profiles.upsert(None, config.clone()).unwrap();
            let mut workspace = crate::product::Workspace {
                mode: "single".into(),
                remote_os: if std::env::consts::OS == "macos" {
                    "windows"
                } else {
                    "macos"
                }
                .into(),
                verification: "single_observed".into(),
                peer_paused_confirmed: true,
                ..Default::default()
            };
            for direction in [
                &mut workspace.evidence.local,
                &mut workspace.evidence.remote,
            ] {
                direction.result = "observed".into();
                direction.source = "user_observed".into();
            }
            workspace.attest(&config).unwrap();
            profiles.profiles[0].workspace = Some(workspace);
        }
        profiles
    }
    fn controller() -> Controller {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut c = Controller::base(
            Ok(std::env::temp_dir()
                .join(format!(
                    "portkey-controller-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ))
                .join("config.json")),
            Ok(None),
        );
        c.lifecycle_ok = true;
        c.lifecycle = || (0, false);
        c.lifecycle_epoch = 0;
        c
    }
    fn begin_setup(c: &mut Controller) {
        discover(c, &[0], true);
        let id = c.config.as_ref().unwrap().display_id.clone();
        assert_eq!(
            c.action(json!({"type":"setup_begin","display_id":id,"name":"Guided desk"}))["ok"],
            true
        );
        assert_eq!(c.action(json!({"type":"setup_usb_start"}))["ok"], true);
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_capturing");
        discover(c, &[0], true);
        let hubs = c.config.as_ref().unwrap().hubs.clone();
        let setup = c.setup.as_mut().unwrap();
        setup.observe(&[], 0);
        setup.observe(&[], 2000);
        setup.observe(&hubs, 3000);
        setup.observe(&hubs, 5000);
        assert_eq!(setup.phase, "usb_ready");
    }
    #[test]
    fn guided_baseline_waits_for_post_request_scan() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let display = c.inventory.as_ref().unwrap().displays[0].clone();
        c.action(json!({"type":"setup_begin","display_id":display.id,"name":"Desk"}));
        let old_generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        c.worker.shared.0.lock().unwrap().inventory = Some((
            0,
            Ok(Inventory {
                displays: vec![display.clone()],
                hubs: vec![],
            }),
        ));
        c.action(json!({"type":"setup_usb_start"}));
        assert!(!c
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .current(old_generation));
        assert!(c.worker.shared.0.lock().unwrap().inventory.is_none());
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_capturing");
        assert_eq!(c.setup.as_ref().unwrap().hub_count, 0);
        discover(&mut c, &[0], true);
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_wait_departure");
        let hubs = c.config.as_ref().unwrap().hubs.clone();
        let setup = c.setup.as_mut().unwrap();
        setup.observe(&[], 0);
        setup.observe(&[], 4740);
        setup.observe(&hubs, 9480);
        setup.observe(&hubs, 14220);
        assert_eq!(setup.phase, "usb_ready");
        assert_eq!(setup.hub_count, 2);
    }
    #[test]
    fn guided_cancel_and_new_draft_ignore_stale_read() {
        let mut c = enrolled();
        begin_setup(&mut c);
        assert_eq!(c.action(json!({"type":"setup_read_local"}))["ok"], true);
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        assert_eq!(c.snapshot()["setup"]["phase"], "reading_input");
        assert_eq!(c.action(json!({"type":"setup_cancel"}))["ok"], true);
        assert!(c.setup.is_none());
        assert!(!c.editing);
        begin_setup(&mut c);
        c.worker.shared.0.lock().unwrap().probe_completion = Some((generation, Ok(5)));
        c.snapshot();
        assert!(c.setup.as_ref().unwrap().local_input.is_none());
        assert_eq!(c.profiles.profiles.len(), 1);
    }
    #[test]
    fn guided_completion_saves_only_complete_paused_profile() {
        let directory =
            std::env::temp_dir().join(format!("portkey-guided-test-{}", std::process::id()));
        let mut c = enrolled();
        c.path = Ok(directory.join("profiles-rust.json"));
        begin_setup(&mut c);
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], false);
        assert!(!directory.join("profiles-rust.json").exists());
        assert_eq!(c.action(json!({"type":"setup_read_local"}))["ok"], true);
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        c.worker.shared.0.lock().unwrap().probe_completion = Some((generation, Ok(5)));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().code.as_deref(), Some("PK1-05-A2"));
        assert_eq!(
            c.action(json!({"type":"setup_peer_code","code":"PK1-05-A2"}))["ok"],
            false
        );
        assert_eq!(
            c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}))["ok"],
            true
        );
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], true);
        assert!(c.setup.is_none());
        let saved = Profiles::load(c.path.as_ref().unwrap()).unwrap();
        assert_eq!(saved.profiles[0].config.local_input, 5);
        assert_eq!(saved.profiles[0].config.remote_input, 7);
        assert!(!saved.profiles[0].config.automatic);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn guided_sleep_and_read_failure_need_retry() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.action(json!({"type":"setup_read_local"}));
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        c.worker.shared.0.lock().unwrap().probe_completion =
            Some((generation, Err("DDC read failed".into())));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_ready");
        assert!(c.setup.as_ref().unwrap().error);
        c.action(json!({"type":"setup_read_local"}));
        c.lifecycle = || (1, true);
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "display");
        assert!(!c.setup.as_ref().unwrap().ready);
        assert!(c.worker.shared.0.lock().unwrap().probe.is_none());
    }
    #[test]
    fn guided_missing_display_and_stale_baseline_fail_closed() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.inventory_received = Some(Instant::now() - Duration::from_secs(10));
        assert_eq!(c.action(json!({"type":"setup_usb_start"}))["ok"], false);
        let hubs = c.config.as_ref().unwrap().hubs.clone();
        c.worker.shared.0.lock().unwrap().inventory = Some((
            0,
            Ok(Inventory {
                hubs,
                displays: vec![],
            }),
        ));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "display");
        assert!(c.setup.as_ref().unwrap().error);
        assert_eq!(c.action(json!({"type":"setup_usb_start"}))["ok"], false);
        assert_eq!(c.action(json!({"type":"setup_read_local"}))["ok"], false);
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], false);
    }
    #[test]
    fn guided_corrected_peer_code_retires_only_its_own_error() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        assert_eq!(
            c.action(json!({"type":"setup_peer_code","code":"bad"}))["ok"],
            false
        );
        assert_eq!(c.snapshot()["error"], true);
        assert_eq!(
            c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}))["ok"],
            true
        );
        let snapshot = c.snapshot();
        assert_eq!(snapshot["error"], false);
        assert_eq!(snapshot["setup"]["error"], false);
        assert_eq!(snapshot["setup"]["phase"], "ready");
        assert!(snapshot["status"]
            .as_str()
            .unwrap()
            .contains("Ready to save"));
        c.action(json!({"type":"setup_peer_code","code":"bad"}));
        c.status = "New hardware failure".into();
        c.error = true;
        c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}));
        assert!(c.error);
        assert_eq!(c.status, "New hardware failure");
    }
    #[test]
    fn guided_inventory_failure_clears_baseline_until_successful_discovery() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.worker.shared.0.lock().unwrap().inventory = Some((0, Err("USB discovery failed".into())));
        c.snapshot();
        assert!(c.inventory.is_none());
        assert!(c.inventory_received.is_none());
        assert_eq!(c.action(json!({"type":"setup_usb_start"}))["ok"], false);
        assert_eq!(c.setup.as_ref().unwrap().phase, "display");
        discover(&mut c, &[0], true);
        assert_eq!(c.action(json!({"type":"setup_usb_start"}))["ok"], true);
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_capturing");
        discover(&mut c, &[0], true);
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_wait_departure");
    }
    #[test]
    fn one_hub_config_observes_exact_topology_and_duplicates_fail_closed() {
        let mut config = enrolled().config.unwrap();
        config.hubs.truncate(1);
        assert!(config.validate().is_ok());
        assert_eq!(config.observe(&config.hubs), Ownership::Owned);
        assert_eq!(config.observe(&[]), Ownership::Absent);
        assert_eq!(
            config.observe(&[config.hubs[0].clone(), config.hubs[0].clone()]),
            Ownership::Unknown
        );
        config.hubs.clear();
        assert!(config.validate().is_err());
    }
    #[test]
    fn split_usb_cache_orders_samples_and_requires_successful_display_discovery() {
        let config = enrolled().config.unwrap();
        let displays = vec![Display {
            id: config.display_id.clone(),
            label: "test".into(),
        }];
        let full = |hubs| {
            Ok(Inventory {
                displays: displays.clone(),
                hubs,
            })
        };
        let mut state = Shared {
            usb_observer: true,
            ..Shared::default()
        };
        state.publish_usb(0, 0, Ok(config.hubs.clone()));
        assert!(
            state.inventory.is_none(),
            "USB cannot select a display at startup"
        );
        state.publish_inventory(0, 0, full(vec![]));
        assert!(
            state.inventory.take().unwrap().1.unwrap().hubs.is_empty(),
            "older cached USB cannot override a fresh full-scan absence"
        );
        let scan_started = state.usb_sequence;
        state.publish_usb(0, 0, Ok(config.hubs.clone()));
        state.publish_inventory_from_scan(0, 0, full(vec![]), scan_started);
        assert_eq!(
            state.inventory.take().unwrap().1.unwrap().hubs,
            config.hubs,
            "USB published during full scan must survive"
        );
        let scan_started = state.usb_sequence;
        state.publish_usb(0, 0, Ok(vec![]));
        state.publish_inventory_from_scan(0, 0, full(config.hubs.clone()), scan_started);
        assert!(state.inventory.take().unwrap().1.unwrap().hubs.is_empty());
        state.publish_inventory(0, 0, Err("display discovery failed".into()));
        assert!(state.inventory.take().unwrap().1.is_err());
        state.publish_usb(0, 0, Ok(config.hubs.clone()));
        assert!(
            state.inventory.is_none(),
            "USB cannot revive failed display discovery"
        );
        state.publish_inventory(0, 0, full(config.hubs.clone()));
        assert_eq!(state.inventory.take().unwrap().1.unwrap().hubs, config.hubs);
        state.publish_usb(0, 0, Err("USB unavailable".into()));
        assert!(state.inventory.take().unwrap().1.is_err());
        state.invalidate();
        assert!(state.display_cache.is_none());
        assert!(state.usb_cache.is_none());
        assert!(state.refresh);
        assert!(state.refresh_usb);
        state.publish_usb(0, 0, Ok(vec![]));
        assert!(state.usb_cache.is_none());
        assert!(state.inventory.is_none());
    }
    #[test]
    fn repeated_cached_usb_sample_cannot_complete_setup_departure() {
        let mut c = enrolled();
        begin_setup(&mut c);
        let hubs = c.config.as_ref().unwrap().hubs.clone();
        c.setup.as_mut().unwrap().start_usb(&hubs).unwrap();
        let displays = c.inventory.as_ref().unwrap().displays.clone();
        let (revision, full_scan_started) = {
            let mut state = c.worker.shared.0.lock().unwrap();
            state.usb_observer = true;
            state.display_cache = Some(displays.clone());
            let revision = state.inventory_revision;
            let before_usb = state.usb_sequence;
            state.publish_usb(revision, 0, Ok(vec![]));
            (revision, before_usb)
        };
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_wait_departure");
        c.started -= Duration::from_secs(3);
        c.worker
            .shared
            .0
            .lock()
            .unwrap()
            .publish_inventory_from_scan(
                revision,
                0,
                Ok(Inventory { displays, hubs }),
                full_scan_started,
            );
        c.snapshot();
        assert_eq!(
            c.setup.as_ref().unwrap().phase,
            "usb_wait_departure",
            "cached sample is not a second observation"
        );
        c.worker
            .shared
            .0
            .lock()
            .unwrap()
            .publish_usb(revision, 0, Ok(vec![]));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_wait_return");
    }
    #[test]
    fn split_profile_change_waits_for_fresh_usb_and_never_hands_off_from_startup_absence() {
        let mut c = two_desks();
        c.worker.shared.0.lock().unwrap().usb_observer = true;
        c.config = None;
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], false);
        assert_eq!(c.policy.ownership(), Ownership::Unknown);
        let revision = c.worker.shared.0.lock().unwrap().inventory_revision;
        let displays = c.inventory.as_ref().unwrap().displays.clone();
        {
            let mut state = c.worker.shared.0.lock().unwrap();
            state.publish_usb(revision, 0, Ok(vec![]));
            assert!(state.inventory.is_none());
            state.publish_inventory(
                revision,
                0,
                Ok(Inventory {
                    displays,
                    hubs: vec![],
                }),
            );
        }
        c.snapshot();
        assert_eq!(c.policy.ownership(), Ownership::Absent);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        assert!(!c.policy.has_established_ownership());
    }
    #[test]
    fn native_usb_observer_publishes_while_display_worker_is_blocked() {
        use std::sync::mpsc;
        let config = enrolled().config.unwrap();
        let inventory = Inventory {
            displays: vec![Display {
                id: config.display_id,
                label: "test".into(),
            }],
            hubs: config.hubs,
        };
        let cached = inventory.clone();
        let (display_started, display_seen) = mpsc::channel();
        let (release_display, display_release) = mpsc::channel();
        let (usb_started, usb_seen) = mpsc::channel();
        let (release_usb, usb_release) = mpsc::channel();
        let (exited, exit_seen) = mpsc::channel();
        struct ExitSignal(std::sync::mpsc::Sender<()>);
        impl Drop for ExitSignal {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let display_exit = ExitSignal(exited.clone());
        let usb_exit = ExitSignal(exited);
        let path =
            std::env::temp_dir().join(format!("portkey-split-worker-{}", std::process::id()));
        let worker = Worker::start_with_usb(
            Arc::new(File::create(&path).unwrap()),
            move || {
                let _keep_until_exit = &display_exit;
                display_started.send(()).unwrap();
                display_release
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok(inventory.clone())
            },
            |_, _| panic!("no hardware writes"),
            |_| panic!("no hardware reads"),
            Some(Box::new(move || {
                let _keep_until_exit = &usb_exit;
                usb_started.send(()).unwrap();
                usb_release.recv_timeout(Duration::from_secs(3)).unwrap();
                Ok(vec![])
            })),
        );
        display_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        usb_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        worker
            .shared
            .0
            .lock()
            .unwrap()
            .publish_inventory(0, 0, Ok(cached));
        release_usb.send(()).unwrap();
        // No device-change hint: the 250ms fallback must run while display discovery is blocked.
        // The broad timeout allows host scheduling jitter but rejects the old two-second cadence.
        usb_seen.recv_timeout(Duration::from_secs(1)).unwrap();
        let published_absence = worker
            .shared
            .0
            .lock()
            .unwrap()
            .inventory
            .as_ref()
            .is_some_and(|(_, result)| result.as_ref().is_ok_and(|i| i.hubs.is_empty()));
        worker.invalidate();
        drop(worker);
        release_usb.send(()).unwrap();
        release_display.send(()).unwrap();
        exit_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        exit_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            published_absence,
            "native USB discovery must not wait for display discovery"
        );
    }
    #[test]
    fn validation_snapshot_keeps_names_only_for_fresh_exact_ids() {
        let state = Shared {
            display_cache: Some(vec![
                Display {
                    id: "present".into(),
                    label: "Friendly monitor".into(),
                },
                Display {
                    id: "missing".into(),
                    label: "Disconnected monitor".into(),
                },
            ]),
            ..Shared::default()
        };
        let raw = Inventory {
            displays: vec![
                Display {
                    id: "present".into(),
                    label: "Generic monitor".into(),
                },
                Display {
                    id: "new".into(),
                    label: "New monitor".into(),
                },
            ],
            hubs: vec![],
        };
        let snapshot = state.validation_snapshot(&Ok(raw.clone())).unwrap();
        assert_eq!(snapshot.displays.len(), 2);
        assert_eq!(snapshot.displays[0].label, "Friendly monitor");
        assert_eq!(snapshot.displays[1].label, "New monitor");
        assert!(!snapshot.displays.iter().any(|d| d.id == "missing"));
        assert_eq!(raw.displays[0].label, "Generic monitor");
        assert!(state
            .validation_snapshot(&Err("discovery failed".into()))
            .is_err());
    }

    #[test]
    fn worker_uses_fresh_validation_reader_instead_of_idle_display_cache() {
        use std::sync::mpsc;
        let config = enrolled().config.unwrap();
        let idle = Inventory {
            displays: vec![crate::core::Display {
                id: config.display_id.clone(),
                label: "test".into(),
            }],
            hubs: config.hubs.clone(),
        };
        let (started, seen) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (validated, validation_seen) = mpsc::channel();
        let path = std::env::temp_dir().join(format!("portkey-validation-{}", std::process::id()));
        let worker = Worker::start_with_readers(
            Arc::new(File::create(&path).unwrap()),
            move || {
                started.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(3)).unwrap();
                Ok(idle.clone())
            },
            |_, _| panic!("fresh missing display must prevent writes"),
            |_| panic!("unexpected probe"),
            None,
            Some(Box::new(move || {
                validated.send(()).unwrap();
                Ok(Inventory {
                    displays: vec![],
                    hubs: vec![],
                })
            })),
        );
        seen.recv_timeout(Duration::from_secs(3)).unwrap();
        worker.submit(
            config.clone(),
            Target::Local,
            Some(Ownership::Owned),
            vec![config.display_id],
        );
        release.send(()).unwrap();
        validation_seen
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let mut state = worker.shared.0.lock().unwrap();
            if let Some((_, result)) = state.completion.take() {
                assert!(result.is_err());
                break;
            }
            drop(state);
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        drop(worker);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn worker_transfer_legacy_retry_budget_and_success_stop() {
        // The lifecycle regression deliberately changes process-global power state.
        // An exact-test child keeps this uninterrupted retry-budget check independent.
        if std::env::var_os("PORTKEY_RETRY_TEST_CHILD").is_none() {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "controller::tests::worker_transfer_legacy_retry_budget_and_success_stop",
                    "--nocapture",
                ])
                .env("PORTKEY_RETRY_TEST_CHILD", "1")
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(status.success(), "isolated retry-budget check failed");
                    return;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("isolated retry-budget check exceeded deadline");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        for successful in [false, true] {
            let config = enrolled().config.unwrap();
            let inventory = Inventory {
                displays: vec![Display {
                    id: config.display_id.clone(),
                    label: "test".into(),
                }],
                hubs: config.hubs.clone(),
            };
            let path = std::env::temp_dir().join(format!(
                "portkey-retry-transfer-{}-{successful}",
                std::process::id()
            ));
            let count = Arc::new(AtomicUsize::new(0));
            let writes = count.clone();
            let (exited, exit_seen) = mpsc::channel();
            struct ExitSignal(mpsc::Sender<()>);
            impl Drop for ExitSignal {
                fn drop(&mut self) {
                    let _ = self.0.send(());
                }
            }
            let signal = ExitSignal(exited);
            let expected = config.clone();
            let worker = Worker::start(
                Arc::new(File::create(&path).unwrap()),
                move || {
                    let _keep_until_exit = &signal;
                    Ok(inventory.clone())
                },
                move |actual, input| {
                    assert_eq!(actual.display_id, expected.display_id);
                    assert_eq!(input, expected.remote_input);
                    writes.fetch_add(1, Ordering::SeqCst);
                    if successful {
                        Ok(WriteOutcome::Confirmed)
                    } else {
                        Err("injected terminal transport failure".into())
                    }
                },
                |_| panic!("retry test must not request input probe"),
            );
            worker.submit(
                config.clone(),
                Target::Remote,
                None,
                vec![config.display_id],
            );
            let deadline = Instant::now() + Duration::from_secs(6);
            let result = loop {
                if let Some((_, outcome)) = worker.shared.0.lock().unwrap().completion.take() {
                    break outcome;
                }
                assert!(Instant::now() < deadline, "bounded worker did not finish");
                std::thread::yield_now();
            };
            assert_eq!(
                count.load(Ordering::SeqCst),
                if successful { 1 } else { 3 },
                "worker result: {result:?}"
            );
            if successful {
                assert!(matches!(result, Ok(WriteOutcome::Confirmed)));
            } else {
                assert_eq!(result.unwrap_err(), "injected terminal transport failure");
            }
            drop(worker);
            exit_seen.recv_timeout(Duration::from_secs(3)).unwrap();
            std::fs::remove_file(path).unwrap();
        }
    }
    #[test]
    fn worker_publishes_prewrite_inventory_before_blocking_transport() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc,
        };
        let config = enrolled().config.unwrap();
        let inventory = Inventory {
            displays: vec![crate::core::Display {
                id: config.display_id.clone(),
                label: "test".into(),
            }],
            hubs: config.hubs.clone(),
        };
        let (scan_started, scan_seen) = mpsc::channel();
        let (release_scan, scan_release) = mpsc::channel();
        let (write_started, write_seen) = mpsc::channel();
        let (release_write, write_release) = mpsc::channel();
        let (worker_exited, exit_seen) = mpsc::channel();
        struct ExitSignal(mpsc::Sender<()>);
        impl Drop for ExitSignal {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let exited = ExitSignal(worker_exited);
        let writes = Arc::new(AtomicUsize::new(0));
        let count = writes.clone();
        let path =
            std::env::temp_dir().join(format!("portkey-worker-barrier-{}", std::process::id()));
        let instance = Arc::new(File::create(&path).unwrap());
        let mut scans = 0;
        let worker = Worker::start(
            instance,
            move || {
                let _keep_until_worker_exits = &exited;
                scans += 1;
                if scans == 1 {
                    scan_started.send(()).unwrap();
                    scan_release.recv_timeout(Duration::from_secs(3)).unwrap();
                }
                Ok(inventory.clone())
            },
            move |_, _| {
                count.fetch_add(1, Ordering::SeqCst);
                write_started.send(()).unwrap();
                write_release.recv_timeout(Duration::from_secs(3)).unwrap();
                Err("injected transport failure".into())
            },
            |_| panic!("unexpected input probe"),
        );
        scan_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        worker.submit(
            config.clone(),
            Target::Local,
            Some(Ownership::Owned),
            vec![config.display_id],
        );
        // Full invalidation discards the first scan; the queued write uses its own revalidation scan.
        worker.shared.0.lock().unwrap().invalidate_inventory();
        release_scan.send(()).unwrap();
        write_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        let published = worker.shared.0.lock().unwrap().inventory.is_some();
        worker.cancel_commands();
        release_write.send(()).unwrap();
        drop(worker);
        exit_seen.recv_timeout(Duration::from_secs(3)).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            published,
            "fresh prewrite inventory must be available while transport is blocked"
        );
        assert_eq!(
            writes.load(Ordering::SeqCst),
            1,
            "cancelled failed write must not retry"
        );
    }
    #[test]
    fn partial_observation_cancels_write_without_discarding_later_absent_scan() {
        let mut c = enrolled();
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (old_write, _) = c.worker.shared.0.lock().unwrap().jobs.take().unwrap();
        assert!(c.policy.has_established_ownership());
        let displays = c.inventory.as_ref().unwrap().displays.clone();
        let partial = vec![c.config.as_ref().unwrap().hubs[0].clone()];
        // Scan 9 starts at 22.721 before UI consumes scan 8's partial result at 22.789.
        let revision = c.worker.shared.0.lock().unwrap().inventory_revision;
        c.worker.shared.0.lock().unwrap().publish_inventory(
            revision,
            0,
            Ok(Inventory {
                displays: displays.clone(),
                hubs: partial,
            }),
        );
        c.snapshot();
        assert_eq!(c.policy.ownership(), Ownership::Partial);
        {
            let mut state = c.worker.shared.0.lock().unwrap();
            assert!(!state.jobs.current(old_write));
            assert!(state.jobs.take().is_none());
            assert_eq!(state.inventory_revision, revision);
            // Scan 9 completes at 24.269. The command cancellation must not discard it.
            state.publish_inventory(
                revision,
                0,
                Ok(Inventory {
                    displays,
                    hubs: vec![],
                }),
            );
            assert!(state.inventory.is_some());
            state.completion = Some((old_write, Err("stale write result".into())));
        }
        c.snapshot();
        assert_eq!(c.policy.ownership(), Ownership::Absent);
        assert_ne!(c.status, "stale write result");
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (_, remote) = c
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .take()
            .expect("retained absence must settle without another scan");
        assert_eq!(remote.target, Target::Remote);
    }
    #[test]
    fn explicit_invalidation_rejects_inflight_inventory_for_setup_and_lifecycle() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let display = c.inventory.as_ref().unwrap().displays[0].clone();
        c.action(json!({"type":"setup_begin","display_id":display.id,"name":"Desk"}));
        let before_baseline = c.worker.shared.0.lock().unwrap().inventory_revision;
        c.action(json!({"type":"setup_usb_start"}));
        {
            let mut state = c.worker.shared.0.lock().unwrap();
            state.publish_inventory(
                before_baseline,
                0,
                Ok(Inventory {
                    displays: vec![display.clone()],
                    hubs: vec![],
                }),
            );
            assert!(state.inventory.is_none());
        }
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_capturing");
        let before_sleep = c.worker.shared.0.lock().unwrap().inventory_revision;
        c.lifecycle = || (1, true);
        c.snapshot();
        let mut state = c.worker.shared.0.lock().unwrap();
        state.publish_inventory(
            before_sleep,
            0,
            Ok(Inventory {
                displays: vec![display],
                hubs: vec![],
            }),
        );
        assert!(state.inventory.is_none());
    }
    #[test]
    fn usb_poll_deadline_uses_remaining_time_and_hints_bypass_wait() {
        assert_eq!(
            usb_poll_wait(Duration::ZERO, false),
            Some(Duration::from_millis(250))
        );
        assert_eq!(
            usb_poll_wait(Duration::from_millis(249), false),
            Some(Duration::from_millis(1))
        );
        assert_eq!(usb_poll_wait(Duration::from_millis(250), false), None);
        assert_eq!(usb_poll_wait(Duration::from_secs(2), false), None);
        assert_eq!(usb_poll_wait(Duration::ZERO, true), None);
    }

    #[test]
    fn device_change_hints_coalesce_without_inventing_ownership_or_writes() {
        let mut c = enrolled();
        c.policy.set_automatic(true, 0);
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        for _ in 0..20 {
            assert_eq!(c.action(json!({"type":"refresh"}))["ok"], true);
        }
        assert_eq!(c.policy.ownership(), Ownership::Unknown);
        assert!(!c.policy.has_established_ownership());
        assert!(c.policy.request().is_none());
        let mut state = c.worker.shared.0.lock().unwrap();
        assert!(state.refresh);
        assert_eq!(state.jobs.generation(), generation);
        assert!(state.jobs.take().is_none());
        assert!(state.probe.is_none());
        assert!(state.inventory.is_none());
        assert!(state.completion.is_none());
    }
    #[test]
    fn refresh_during_inflight_work_remains_pending_and_preserves_queued_job() {
        let mut c = enrolled();
        c.action(json!({"type":"manual","target":"local"}));
        // The worker owns this job outside the mutex during discovery/transport.
        let (inflight_generation, _inflight) =
            c.worker.shared.0.lock().unwrap().jobs.take().unwrap();
        for _ in 0..20 {
            c.worker.refresh();
        }
        {
            let mut state = c.worker.shared.0.lock().unwrap();
            assert!(state.refresh);
            assert!(state.jobs.current(inflight_generation));
            // Worker result publication does not consume a hint received during the scan.
            state.inventory = Some((
                0,
                Ok(Inventory {
                    displays: vec![],
                    hubs: vec![],
                }),
            ));
            state.completion = Some((inflight_generation, Ok(WriteOutcome::Confirmed)));
        }
        assert!(c.worker.shared.0.lock().unwrap().refresh);
        c.action(json!({"type":"manual","target":"remote"}));
        for _ in 0..20 {
            c.worker.refresh();
        }
        let mut state = c.worker.shared.0.lock().unwrap();
        assert!(state.refresh);
        let (queued_generation, queued) = state
            .jobs
            .take()
            .expect("refresh must preserve pending write");
        assert_eq!(queued.target, Target::Remote);
        assert!(state.jobs.current(queued_generation));
        assert!(!state.jobs.current(inflight_generation));
        assert!(state.jobs.take().is_none());
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
    fn current_qualified_profile_pause_resume_survives_restart_and_manual_selection() {
        let mut c = enrolled();
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], true);
        let id = c.active_profile_id.clone().unwrap();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":id}))["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":false}))["ok"],
            true
        );
        let mut restarted =
            Controller::from_store(c.path.clone(), Profiles::load(c.path.as_ref().unwrap()));
        restarted.lifecycle_ok = true;
        restarted.lifecycle = || (0, false);
        discover(&mut restarted, &[0], true);
        assert_eq!(restarted.snapshot()["workspace"]["can_enable"], true);
        assert_eq!(
            restarted.action(json!({"type":"automatic","enabled":true}))["ok"],
            true
        );
        assert!(restarted.config.as_ref().unwrap().automatic);
        let directory = c.path.as_ref().unwrap().parent().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn missing_and_historical_workspace_never_authorize_automatic_switching() {
        for historical in [false, true] {
            let mut c = enrolled();
            c.profiles.profiles[0].config.automatic = true;
            if historical {
                let workspace = c.profiles.profiles[0].workspace.as_mut().unwrap();
                workspace.verification = "historical_unknown".into();
                workspace.binding = None;
            } else {
                c.profiles.profiles[0].workspace = None;
            }
            discover(&mut c, &[0], true);
            assert!(!c.policy.automatic());
            assert_eq!(c.snapshot()["workspace"]["can_enable"], false);
            assert_eq!(
                c.action(json!({"type":"automatic","enabled":true}))["ok"],
                false
            );
            assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        }
    }
    #[test]
    fn obsolete_configurations_and_sibling_language_are_ignored_without_mutation() {
        let c = controller();
        let directory = c.path.as_ref().unwrap().parent().unwrap();
        let current = directory.join("PortKey");
        let previous = directory.join("portkey");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&previous).unwrap();
        let old = current.join("config-rust.json");
        let swift = current.join("config.json");
        std::fs::write(&old, b"invalid obsolete Rust configuration").unwrap();
        std::fs::write(&swift, b"invalid obsolete Swift configuration").unwrap();
        std::fs::write(previous.join("language"), "zh-Hans").unwrap();
        // Case-insensitive filesystems alias these directory names; that is the current preference file.
        if current.join("language").exists() {
            std::fs::remove_file(previous.join("language")).unwrap();
        }
        let path = current.join("profiles-rust.json");
        let loaded = Profiles::load(&path).unwrap();
        assert!(loaded.profiles.is_empty());
        let mut fresh = Controller::from_store(Ok(path.clone()), Ok(loaded));
        assert_eq!(fresh.language, "en");
        assert!(!path.exists());
        assert_eq!(
            fresh.action(json!({"type":"import","path":swift}))["ok"],
            false
        );
        assert_eq!(
            std::fs::read(&old).unwrap(),
            b"invalid obsolete Rust configuration"
        );
        assert_eq!(
            std::fs::read(&swift).unwrap(),
            b"invalid obsolete Swift configuration"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn obsolete_profiles_without_ids_are_rejected_without_rewriting() {
        let c = enrolled();
        let mut value = serde_json::to_value(&c.profiles).unwrap();
        value["profiles"][0]
            .as_object_mut()
            .unwrap()
            .remove("profile_id");
        let path = c.path.as_ref().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(path, &bytes).unwrap();
        assert!(Profiles::load(path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn product_new_and_changed_saves_require_current_qualification() {
        {
            let original = enrolled().config.unwrap();
            let mut c = controller();
            assert_eq!(
                c.action(json!({"type":"save","config":original}))["ok"],
                true
            );
            discover(&mut c, &[0], true);
            assert_eq!(c.snapshot()["workspace"]["verification"], "not_checked");
            assert_eq!(c.snapshot()["workspace"]["can_enable"], false);
            assert_eq!(
                c.action(json!({"type":"automatic","enabled":true}))["ok"],
                false
            );
        }
        let mut c = enrolled();
        let mut changed = c.config.clone().unwrap();
        changed.remote_input = 6;
        assert_eq!(
            c.action(json!({"type":"save","config":changed}))["ok"],
            true
        );
        discover(&mut c, &[0], true);
        assert_eq!(c.snapshot()["workspace"]["can_enable"], false);
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":true}))["ok"],
            false
        );
    }
    #[test]
    fn product_metadata_full_payload_cannot_reattest_and_revision_invalidates() {
        let mut c = enrolled();
        let mut workspace = crate::product::Workspace::default().edit(json!({"remote_os":"windows","mode":"single","verification":"single_observed","peer_paused_confirmed":true,"evidence":{"local":{"result":"observed","source":"user_observed"},"remote":{"result":"observed","source":"user_observed"}}})).unwrap();
        let original = c.config.clone().unwrap();
        workspace.attest(&original).unwrap();
        c.profiles.profiles[0].workspace = Some(workspace.clone());
        discover(&mut c, &[0], true);
        let mut renamed = workspace.public();
        renamed["local_name"] = json!("Renamed laptop");
        assert_eq!(
            c.action(json!({"type":"workspace_save","workspace":renamed}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["workspace"]["can_enable"], true);
        for revision in [2, 1] {
            assert_eq!(
                c.action(json!({"type":"workspace_save","workspace":{"revision":revision}}))["ok"],
                true
            );
            assert_eq!(c.snapshot()["workspace"]["can_enable"], false);
        }
        // Restore a qualified fixture, then change a mapping through the real save action.
        c.profiles.profiles[0].workspace = Some(workspace.clone());
        let mut changed = original;
        changed.remote_input = 6;
        assert_eq!(
            c.action(json!({"type":"save","config":changed}))["ok"],
            true
        );
        let mut full_payload = workspace.public();
        full_payload["local_name"] = json!("Another name");
        assert_eq!(
            c.action(json!({"type":"workspace_save","workspace":full_payload}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["workspace"]["can_enable"], false);
        assert_eq!(c.snapshot()["workspace"]["qualification_current"], false);
    }
    #[test]
    fn product_unselected_display_draft_is_not_disconnected_and_selection_recovers() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let display = c.inventory.as_ref().unwrap().displays[0].id.clone();
        assert_eq!(
            c.action(json!({"type":"setup_draft_begin","name":"Home","workspace":{}}))["ok"],
            true
        );
        discover(&mut c, &[0], true);
        assert!(!c.setup.as_ref().unwrap().error);
        assert_eq!(c.setup.as_ref().unwrap().display_id, "");
        c.setup.as_mut().unwrap().interrupted();
        assert_eq!(
            c.action(json!({"type":"setup_select_display","display_id":display}))["ok"],
            true
        );
        assert!(!c.snapshot()["setup"]["error"].as_bool().unwrap());
        assert_eq!(c.setup.as_ref().unwrap().phase, "display");
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn product_setup_messages_localize_without_rewriting_saved_draft() {
        let mut c = controller();
        c.language = "zh-Hans".into();
        let invalid = c.action(json!({"type":"setup_draft_begin","name":"Home","workspace":{"local_name":"same","remote_name":"same"}}));
        assert_eq!(invalid["error"], "请为两台电脑设置不同的名称。");
        assert_eq!(
            c.action(json!({"type":"setup_draft_begin","name":"Home","workspace":{}}))["ok"],
            true
        );
        assert_eq!(
            c.snapshot()["setup"]["message"],
            "请先将 USB 切换到本机，再开始检测 USB 开关。"
        );
        assert_eq!(
            c.setup.as_ref().unwrap().message,
            "Keep USB on this computer, then start USB detection."
        );
        c.language = "en".into();
        assert_eq!(
            c.snapshot()["setup"]["message"],
            "Keep USB on this computer, then start USB detection."
        );
    }
    #[test]
    fn product_startup_failure_is_structured_and_controls_blocked() {
        let mut c = controller();
        c.startup_error = Some("PortKey is already running".into());
        let state = c.snapshot();
        assert_eq!(state["startup_error"], "PortKey is already running");
        assert_eq!(state["controls_available"], false);
        assert_eq!(c.action(json!({"type":"refresh"}))["ok"], false);
    }
    #[test]
    fn product_readback_state_is_truthful_and_invalidated_by_usb_change() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        assert_eq!(
            c.action(json!({"type":"manual","target":"remote"}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["display_state"]["result"], "pending");
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        c.worker.shared.0.lock().unwrap().completion =
            Some((generation, Ok(WriteOutcome::Confirmed)));
        assert_eq!(
            c.snapshot()["display_state"]["result"],
            "readback_confirmed"
        );
        assert_eq!(c.snapshot()["display_state"]["visual_confirmed"], false);
        discover(&mut c, &[0], false);
        assert_eq!(c.snapshot()["display_state"]["result"], "idle");
    }
    #[test]
    fn product_usb_progress_is_persisted_and_expected_departure_is_not_fault() {
        let mut c = enrolled();
        let dir =
            std::env::temp_dir().join(format!("portkey-product-progress-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        begin_setup(&mut c);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(c.draft_path().unwrap()).unwrap()).unwrap();
        // begin_setup advanced hardware state directly, snapshot must persist it.
        c.snapshot();
        let progressed: Value =
            serde_json::from_slice(&std::fs::read(c.draft_path().unwrap()).unwrap()).unwrap();
        assert_ne!(saved["phase"], progressed["phase"]);
        assert_eq!(progressed["phase"], "usb_ready");
        c.setup
            .as_mut()
            .unwrap()
            .start_usb(&c.inventory.as_ref().unwrap().hubs)
            .unwrap();
        let epoch = (c.lifecycle)().0;
        c.worker.shared.0.lock().unwrap().inventory = Some((
            epoch,
            Ok(Inventory {
                displays: vec![],
                hubs: vec![],
            }),
        ));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().phase, "usb_wait_departure");
        assert!(!c.setup.as_ref().unwrap().error);
        c.action(json!({"type":"setup_cancel"}));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_exchange_preview_is_nonmutating_and_stale_preview_cannot_apply() {
        let mut c = enrolled();
        let dir =
            std::env::temp_dir().join(format!("portkey-product-exchange-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        c.action(json!({"type":"setup_workspace","workspace":{"workspace_id":"pk-exchange","local_name":"Laptop","remote_name":"Desktop","remote_os":"windows","mode":"dual"}}));
        let mut record = json!({"version":2,"workspace_id":"pk-exchange","revision":1,"role":"b","local_name":"Desktop","remote_name":"Laptop","local_os":"windows","remote_os":std::env::consts::OS,"local_input":7,"remote_input":5});
        for key in ["version", "revision"] {
            let original = record[key].clone();
            record[key] = json!(9);
            assert_eq!(
                c.action(json!({"type":"setup_peer_preview","record":record.to_string()}))["ok"],
                false
            );
            record[key] = original;
        }
        assert_eq!(
            c.action(json!({"type":"setup_peer_preview","record":record.to_string()}))["ok"],
            true
        );
        assert_eq!(c.setup.as_ref().unwrap().remote_input, None);
        assert!(!c.setup.as_ref().unwrap().workspace.peer_confirmed);
        c.setup.as_mut().unwrap().input_result(Ok(8));
        assert_eq!(c.action(json!({"type":"setup_peer_confirm"}))["ok"], false);
        assert_eq!(
            c.action(json!({"type":"setup_peer_preview","record":record.to_string()}))["ok"],
            false
        );
        c.setup.as_mut().unwrap().input_result(Ok(5));
        assert_eq!(
            c.action(json!({"type":"setup_peer_preview","record":record.to_string()}))["ok"],
            true
        );
        assert_eq!(c.action(json!({"type":"setup_peer_confirm"}))["ok"], true);
        assert_eq!(c.setup.as_ref().unwrap().remote_input, Some(7));
        assert_eq!(
            c.setup.as_ref().unwrap().workspace.verification,
            "not_checked"
        );
        assert_eq!(
            c.action(json!({"type":"setup_finish","enable":true}))["ok"],
            false
        );
        c.action(json!({"type":"setup_cancel"}));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn connection_profile_evidence_pause_and_enable_are_scoped_to_active_id() {
        let mut c = enrolled();
        let a = c.profiles.profiles[0].profile_id.clone();
        let config = c.profiles.profiles[0].config.clone();
        let mut evidence=crate::product::Workspace::default().edit(json!({"local_name":"Qualified laptop","remote_os":"windows","mode":"single","verification":"single_observed","peer_paused_confirmed":true,"evidence":{"local":{"result":"observed","source":"user_observed"},"remote":{"result":"observed","source":"user_observed"}}})).unwrap();
        evidence.attest(&config).unwrap();
        c.profiles.profiles[0].workspace = Some(evidence.clone());
        c.profiles.profiles[0].config.automatic = true;
        let b = crate::profiles::new_profile_id();
        c.profiles
            .upsert_id(b.clone(), Some("Unverified cable".into()), config)
            .unwrap();
        c.profiles.profiles[1].workspace = Some(crate::product::Workspace::default());
        discover(&mut c, &[0], true);
        assert!(c.config.is_none());
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":false}))["ok"],
            true
        );
        assert!(
            c.profiles.profiles[0].config.automatic,
            "pause without an active profile cannot change another profile's preference"
        );
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":a}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["workspace"]["verification"], "single_observed");
        assert_eq!(c.snapshot()["workspace"]["automatic_requested"], true);
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":b}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["workspace"]["verification"], "not_checked");
        assert_eq!(c.snapshot()["workspace"]["automatic_requested"], false);
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":true}))["ok"],
            false
        );
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":false}))["ok"],
            true
        );
        assert!(c.profiles.profiles[0].config.automatic);
        assert!(!c.profiles.profiles[1].config.automatic);
        assert_eq!(c.profiles.profiles[0].workspace, Some(evidence));
    }
    #[test]
    fn connection_profile_duplicate_target_and_lifecycle_keep_manual_choice_safe() {
        let mut c = two_desks();
        discover(&mut c, &[0], true);
        let id = c.profiles.profiles[0].profile_id.clone();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":id}))["ok"],
            true
        );
        discover(&mut c, &[0, 0], true);
        assert_eq!(
            c.snapshot()["profile_selection"]["reason"],
            "selected_display_ambiguous"
        );
        assert!(c.config.is_none());
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            false
        );
        c.worker.shared.0.lock().unwrap().resumed = true;
        c.snapshot();
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "manual");
        assert_eq!(
            c.snapshot()["profile_selection"]["availability"],
            "unavailable"
        );
        discover(&mut c, &[0], true);
        assert_eq!(c.snapshot()["active_profile_id"], id);
        c.lifecycle = || (1, false);
        c.snapshot();
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "manual");
        discover(&mut c, &[0], true);
        assert_eq!(c.snapshot()["active_profile_id"], id);
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
    }
    #[test]
    fn obsolete_draft_without_profile_identity_is_rejected_without_rewriting() {
        let mut c = enrolled();
        let mut old = serde_json::to_value(crate::setup::Setup::new(
            c.config.as_ref().unwrap().display_id.clone(),
            "Old draft".into(),
        ))
        .unwrap();
        old.as_object_mut().unwrap().remove("profile_id");
        std::fs::create_dir_all(c.path.as_ref().unwrap().parent().unwrap()).unwrap();
        let path = c.draft_path().unwrap();
        let bytes = old.to_string();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(c.action(json!({"type":"setup_resume"}))["ok"], false);
        assert!(c.setup.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn connection_profile_auto_ambiguity_and_identical_configs_use_real_profile_identity() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let first = c.profiles.profiles[0].profile_id.clone();
        let second = crate::profiles::new_profile_id();
        let config = c.config.clone().unwrap();
        c.profiles
            .upsert_id(second.clone(), Some("Second cable".into()), config.clone())
            .unwrap();
        discover(&mut c, &[0], true);
        assert!(c.config.is_none());
        assert_eq!(
            c.snapshot()["profile_selection"]["reason"],
            "choose_profile"
        );
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":first}))["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"manual","target":"remote"}))["ok"],
            true
        );
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":second}))["ok"],
            true
        );
        assert_eq!(c.config.as_ref(), Some(&config));
        assert_eq!(c.snapshot()["active_profile_id"], second);
        assert!(!c.worker.shared.0.lock().unwrap().jobs.current(generation));
        c.worker.shared.0.lock().unwrap().completion =
            Some((generation, Ok(WriteOutcome::Confirmed)));
        assert_eq!(c.snapshot()["display_state"]["result"], "idle");
        let mut reloaded =
            Controller::from_store(c.path.clone(), Profiles::load(c.path.as_ref().unwrap()));
        reloaded.lifecycle_ok = true;
        reloaded.lifecycle = || (0, false);
        reloaded.lifecycle_epoch = 0;
        discover(&mut reloaded, &[0], true);
        assert_eq!(reloaded.snapshot()["active_profile_id"], second);
        assert_eq!(reloaded.snapshot()["profile_selection"]["mode"], "auto");
    }
    #[test]
    fn connection_profiles_create_same_display_without_overwriting_and_edit_by_id() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let original = c.profiles.profiles[0].config.clone();
        let original_name = c.profiles.profiles[0].name.clone();
        let original_id = c.snapshot()["profiles"][0]["profile_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert!(!original_id.is_empty());
        let display = original.display_id.clone();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":original_id}))["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"setup_draft_begin","name":"HDMI second cable","workspace":{}}))
                ["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"setup_select_display","display_id":display}))["ok"],
            true
        );
        let setup = c.setup.as_mut().unwrap();
        setup.start_usb(&original.hubs).unwrap();
        setup.observe(&[], 0);
        setup.observe(&[], 2000);
        setup.observe(&original.hubs, 3000);
        setup.observe(&original.hubs, 5000);
        setup.input_result(Ok(5));
        setup.peer_code("PK1-07-A0").unwrap();
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], true);
        assert_eq!(c.profiles.profiles.len(), 2);
        assert_eq!(c.profiles.profiles[0].config, original);
        assert_eq!(c.profiles.profiles[0].name, original_name);
        assert!(c.profiles.profiles[0]
            .workspace
            .as_ref()
            .unwrap()
            .can_enable(&original));
        let second = c.snapshot()["profiles"][1]["profile_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(second, original_id);
        assert_eq!(c.snapshot()["active_profile_id"], second);
        assert_eq!(c.action(json!({"type":"workspace_save","target_profile_id":second,"profile_name":"Cable B renamed","workspace":{"local_name":"Laptop B"}}))["ok"],true);
        assert_eq!(c.profiles.profiles[0].name, original_name);
        assert_eq!(c.profiles.profiles[1].name, "Cable B renamed");
        assert_eq!(c.action(json!({"type":"workspace_save","display_id":display,"workspace":{"local_name":"Ambiguous"}}))["ok"],false);
        assert_eq!(
            c.action(json!({"type":"save","config":original}))["ok"],
            false
        );
        assert_eq!(c.action(json!({"type":"setup_draft_begin","target_profile_id":second,"name":"Cable B re-enrolled","workspace":{}}))["ok"],true);
        assert_eq!(
            c.action(json!({"type":"setup_select_display","display_id":display}))["ok"],
            true
        );
        let hubs = c.profiles.profiles[0].config.hubs.clone();
        let setup = c.setup.as_mut().unwrap();
        setup.start_usb(&hubs).unwrap();
        setup.observe(&[], 0);
        setup.observe(&[], 2000);
        setup.observe(&hubs, 3000);
        setup.observe(&hubs, 5000);
        setup.input_result(Ok(6));
        setup.peer_code("PK1-07-A0").unwrap();
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], true);
        assert_eq!(c.profiles.profiles.len(), 2);
        assert_eq!(c.profiles.profiles[0].name, original_name);
        assert_eq!(c.profiles.profiles[1].config.local_input, 6);
    }
    #[test]
    fn connection_profile_manual_selection_waits_exact_target_and_restores_auto_on_disconnect() {
        let mut c = two_desks();
        discover(&mut c, &[0], true);
        let home = c.snapshot()["profiles"][0]["profile_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let work = c.snapshot()["profiles"][1]["profile_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":work}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "manual");
        assert_eq!(
            c.snapshot()["profile_selection"]["reason"],
            "selected_display_missing"
        );
        assert!(c.config.is_none());
        discover(&mut c, &[0], false);
        assert!(c.config.is_none());
        discover(&mut c, &[1], true);
        assert_eq!(c.snapshot()["active_profile_id"], work);
        let epoch = (c.lifecycle)().0;
        c.worker.shared.0.lock().unwrap().inventory =
            Some((epoch, Err("temporary discovery failure".into())));
        c.snapshot();
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "manual");
        discover(&mut c, &[1], true);
        assert_eq!(c.snapshot()["active_profile_id"], work);
        discover(&mut c, &[0], false);
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "auto");
        assert_eq!(c.snapshot()["active_profile_id"], home);
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":home}))["ok"],
            true
        );
        assert_eq!(c.action(json!({"type":"profile_restore_auto"}))["ok"], true);
        assert_eq!(c.snapshot()["profile_selection"]["mode"], "auto");
    }
    #[test]
    fn connection_profile_manual_disambiguates_displays_and_cancels_before_failed_persistence() {
        let mut c = two_desks();
        discover(&mut c, &[0, 1], true);
        let home = c.snapshot()["profiles"][0]["profile_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(c.config.is_none());
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":home}))["ok"],
            true
        );
        assert_eq!(c.snapshot()["active_profile_id"], home);
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            true
        );
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        let before = serde_json::to_value(&c.profiles).unwrap();
        c.path = Err("injected persistence failure".into());
        let work = c.snapshot()["profiles"][1]["profile_id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":work}))["ok"],
            false
        );
        assert!(!c.worker.shared.0.lock().unwrap().jobs.current(generation));
        assert_eq!(
            c.snapshot()["profile_selection"]["selected_profile_id"],
            home
        );
        assert_eq!(serde_json::to_value(&c.profiles).unwrap(), before);
        assert_eq!(c.action(json!({"type":"edit","enabled":true}))["ok"], true);
        assert_eq!(
            c.action(json!({"type":"profile_select","profile_id":home}))["ok"],
            false
        );
    }
    #[test]
    fn product_diagnostics_preview_is_sanitized_and_hardware_free() {
        let mut c = enrolled();
        discover(&mut c, &[0], true);
        let raw_display = c.config.as_ref().unwrap().display_id.clone();
        let reply = c.action(json!({"type":"diagnostics_preview"}));
        assert_eq!(reply["ok"], true);
        let diagnostics = &reply["diagnostics"];
        assert_eq!(diagnostics["version"], 1);
        assert_eq!(diagnostics["current"]["enrolled"], true);
        assert_eq!(diagnostics["current"]["peer_runtime"], "unknown");
        assert!(diagnostics.get("history").is_some());
        assert!(!diagnostics.to_string().contains(&raw_display));
        assert!(!diagnostics.to_string().contains("config_path"));
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        assert!(c.worker.shared.0.lock().unwrap().probe.is_none());
        c.startup_error = Some("PRIVATE startup backend detail".into());
        let blocked = c.action(json!({"type":"diagnostics_preview"}));
        assert_eq!(blocked["ok"], true);
        assert_eq!(blocked["diagnostics"]["current"]["startup_blocked"], true);
        assert!(!blocked.to_string().contains("PRIVATE"));
    }
    #[test]
    fn product_committed_setup_reports_cleanup_warning_and_completed_draft_cannot_resume() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        c.action(
            json!({"type":"setup_workspace","workspace":{"mode":"single","remote_os":"windows"}}),
        );
        c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}));
        let evidence = json!({"local":{"result":"observed","source":"user_observed"},"remote":{"result":"observed","source":"user_observed"}});
        assert_eq!(c.action(json!({"type":"setup_evidence","evidence":evidence,"verification":"single_observed","peer_paused_confirmed":true}))["ok"], true);
        let draft = c.draft_path().unwrap();
        let leftover = std::fs::read(&draft).unwrap();
        // A directory at the deletion target forces a real cleanup failure on every host.
        std::fs::remove_file(&draft).unwrap();
        std::fs::create_dir(&draft).unwrap();
        let reply = c.action(json!({"type":"setup_finish","enable":true}));
        assert_eq!(reply["ok"], true);
        assert!(reply["warning"].as_str().is_some_and(|s| !s.is_empty()));
        assert!(c.setup.is_none());
        assert!(c.profiles.profiles[0].config.automatic);
        assert_eq!(c.snapshot()["draft_available"], false);
        assert!(c.snapshot()["warning"].as_str().is_some());
        c.status = "Ordinary worker status".into();
        assert!(c.snapshot()["status"]
            .as_str()
            .unwrap()
            .contains("cleanup needs attention"));
        assert_eq!(c.snapshot()["error"], false);
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        // Model a Windows sharing failure leaving the completed regular draft in place.
        std::fs::remove_dir(&draft).unwrap();
        std::fs::write(&draft, leftover).unwrap();
        let mut reopened =
            Controller::from_store(c.path.clone(), Profiles::load(c.path.as_ref().unwrap()));
        assert_eq!(reopened.snapshot()["draft_available"], false);
        assert_eq!(reopened.action(json!({"type":"setup_resume"}))["ok"], false);
        assert!(reopened.setup.is_none());
        assert!(reopened.profiles.profiles[0].config.automatic);
        assert!(reopened
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .take()
            .is_none());
        std::fs::remove_dir_all(c.path.as_ref().unwrap().parent().unwrap()).unwrap();
    }
    #[test]
    fn product_failed_save_preserves_saved_profile_and_edit_draft() {
        let mut c = enrolled();
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}));
        let previous = c.profiles.profiles[0].config.clone();
        let dir = c.path.as_ref().unwrap().parent().unwrap().to_path_buf();
        // A directory at the profile file is an actual atomic-replace failure.
        std::fs::create_dir_all(c.path.as_ref().unwrap()).unwrap();
        assert_eq!(c.action(json!({"type":"setup_finish"}))["ok"], false);
        assert!(c.setup.is_some());
        assert_eq!(c.profiles.profiles[0].config, previous);
        assert!(c.editing);
        assert!(!c.policy.automatic());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_config_override_is_absolute_and_scoped() {
        assert!(controller_config_path(Some("relative".into())).is_err());
        let directory = std::env::temp_dir().join("portkey-isolated");
        assert!(controller_config_path(Some(directory.join("..").into_os_string())).is_err());
        assert_eq!(
            controller_config_path(Some(directory.clone().into_os_string())).unwrap(),
            directory.join("profiles-rust.json")
        );
    }
    #[test]
    fn product_qualification_is_bound_and_cannot_be_forged_as_history() {
        let mut c = enrolled();
        let dir = std::env::temp_dir().join(format!(
            "portkey-product-qualification-{}",
            std::process::id()
        ));
        c.path = Ok(dir.join("profiles-rust.json"));
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        assert_eq!(c.action(json!({"type":"setup_workspace","workspace":{"mode":"single","remote_os":"windows"}}))["ok"], true);
        assert_eq!(
            c.action(json!({"type":"setup_peer_code","code":"PK1-07-A0"}))["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"setup_finish","enable":true}))["ok"],
            false
        );
        let evidence = json!({"local":{"result":"observed","source":"user_observed","note":"User saw local picture"},"remote":{"result":"observed","source":"user_observed","note":"User saw other picture"}});
        assert_eq!(c.action(json!({"type":"setup_evidence","evidence":evidence,"verification":"single_observed"}))["ok"], false);
        assert_eq!(c.action(json!({"type":"setup_evidence","evidence":evidence,"verification":"single_observed","peer_paused_confirmed":true}))["ok"], true);
        assert_eq!(
            c.action(json!({"type":"setup_finish","enable":true}))["ok"],
            true
        );
        assert!(c.profiles.profiles[0].config.automatic);
        assert_eq!(c.action(json!({"type":"workspace_save","workspace":{"verification":"historical_unknown","mode":"unverified"}}))["ok"], false);
        // A changed binding in a loaded config never inherits old qualified automation.
        c.profiles.profiles[0].config.local_input = 8;
        c.config = None;
        discover(&mut c, &[0], true);
        assert!(!c.policy.automatic());
        assert_eq!(c.snapshot()["workspace"]["qualification_current"], false);
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":true}))["ok"],
            false
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_single_path_reads_other_picture_without_input_writes() {
        let mut c = enrolled();
        let dir =
            std::env::temp_dir().join(format!("portkey-product-single-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        begin_setup(&mut c);
        c.setup.as_mut().unwrap().input_result(Ok(5));
        assert_eq!(c.action(json!({"type":"setup_workspace","workspace":{"mode":"single","remote_os":"windows"}}))["ok"], true);
        assert_eq!(c.action(json!({"type":"setup_read_remote"}))["ok"], true);
        let generation = c.worker.shared.0.lock().unwrap().jobs.generation();
        c.worker.shared.0.lock().unwrap().probe_completion = Some((generation, Ok(7)));
        c.snapshot();
        assert_eq!(c.setup.as_ref().unwrap().remote_input, Some(7));
        assert!(c.setup.as_ref().unwrap().ready);
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        c.action(json!({"type":"setup_cancel"}));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_join_adopts_only_portable_identity_and_requires_local_enrollment() {
        let mut c = controller();
        let dir = std::env::temp_dir().join(format!("portkey-product-join-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        let os = std::env::consts::OS;
        let record = json!({"version":2,"workspace_id":"pk-shared-123","revision":4,"role":"b","local_name":"Desktop","remote_name":"Laptop","local_os":"windows","remote_os":os,"local_input":7,"remote_input":5}).to_string();
        assert_eq!(
            c.action(json!({"type":"setup_join","record":record,"name":"Home"}))["ok"],
            true
        );
        assert_eq!(
            c.snapshot()["setup"]["workspace"]["workspace_id"],
            "pk-shared-123"
        );
        assert_eq!(c.snapshot()["setup"]["workspace"]["local_name"], "Laptop");
        assert_eq!(c.snapshot()["setup"]["workspace"]["local_role"], "a");
        assert!(c.setup.as_ref().unwrap().local_input.is_none());
        assert!(c.setup.as_ref().unwrap().remote_input.is_none());
        assert_eq!(
            c.action(json!({"type":"setup_finish","enable":true}))["ok"],
            false
        );
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        c.action(json!({"type":"setup_cancel"}));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_current_metadata_preserves_preference_and_scope() {
        let mut c = two_desks();
        let dir = std::env::temp_dir().join(format!("portkey-product-meta-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], true);
        c.profiles.save(c.path.as_ref().unwrap()).unwrap();
        let home = c.config.as_ref().unwrap().display_id.clone();
        assert_eq!(c.snapshot()["workspace"]["verification"], "single_observed");
        let id_before_metadata = c.snapshot()["workspace"]["workspace_id"].clone();
        assert_eq!(
            c.snapshot()["workspace"]["workspace_id"],
            id_before_metadata
        );
        assert_eq!(c.action(json!({"type":"workspace_save","display_id":home,"workspace":{"local_name":"Desk Mac","remote_name":"Gaming PC"}}))["ok"], true);
        assert!(c.profiles.profiles[0].config.automatic);
        assert_eq!(c.snapshot()["workspace"]["local_name"], "Desk Mac");
        assert!(c.snapshot()["profiles"][1]["workspace"].is_null());
        assert_eq!(c.action(json!({"type":"workspace_save","workspace":{"local_name":"same","remote_name":" SAME "}}))["ok"], false);
        assert_eq!(c.snapshot()["workspace"]["local_name"], "Desk Mac");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn product_draft_survives_restart_without_replaying_hardware() {
        let mut c = enrolled();
        let dir =
            std::env::temp_dir().join(format!("portkey-product-draft-{}", std::process::id()));
        c.path = Ok(dir.join("profiles-rust.json"));
        let workspace = json!({"local_name":"Office laptop","remote_name":"Home desktop","remote_os":"windows","mode":"single"});
        assert_eq!(
            c.action(json!({"type":"setup_draft_begin","name":"Work","workspace":workspace}))["ok"],
            true
        );
        assert_eq!(c.action(json!({"type":"setup_suspend"}))["ok"], true);
        let mut restarted = Controller::from_store(c.path.clone(), Ok(c.profiles.clone()));
        assert_eq!(restarted.snapshot()["draft_available"], true);
        assert_eq!(restarted.action(json!({"type":"setup_resume"}))["ok"], true);
        assert_eq!(
            restarted.snapshot()["setup"]["workspace"]["local_name"],
            "Office laptop"
        );
        assert_eq!(restarted.snapshot()["setup"]["name"], "Work");
        assert_eq!(restarted.snapshot()["setup"]["ready"], false);
        assert_eq!(restarted.snapshot()["setup"]["error"], false);
        assert!(restarted
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .take()
            .is_none());
        assert_eq!(restarted.action(json!({"type":"setup_cancel"}))["ok"], true);
        assert_eq!(restarted.snapshot()["draft_available"], false);
        std::fs::remove_dir_all(dir).unwrap();
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
        c.profiles = test_profiles(c.config.clone());
        c.active_profile_id = Some(c.profiles.profiles[0].profile_id.clone());
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
        let path = directory.join("profiles-rust.json");
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
    fn two_desks() -> Controller {
        let mut c = enrolled();
        c.profiles.profiles[0].name = "Home".into();
        let mut work = c.config.clone().unwrap();
        if work.platform == "macos" {
            work.display_id = "abcdefab-1234-1234-1234-123456789abc".into();
        } else {
            work.display_id = r"\\?\display#work#stable".into();
        }
        c.profiles.upsert(Some("Work".into()), work).unwrap();
        c
    }
    fn discover(c: &mut Controller, indices: &[usize], hubs: bool) {
        let displays = indices
            .iter()
            .map(|i| crate::core::Display {
                id: c.profiles.profiles[*i].config.display_id.clone(),
                label: "test".into(),
            })
            .collect();
        let hubs = if hubs {
            c.profiles.profiles[0].config.hubs.clone()
        } else {
            vec![]
        };
        c.worker.shared.0.lock().unwrap().inventory = Some((0, Ok(Inventory { displays, hubs })));
        c.snapshot();
    }
    #[test]
    fn home_work_changes_cancel_old_manual_and_reset_absence() {
        let mut c = two_desks();
        discover(&mut c, &[0], true);
        c.action(json!({"type":"manual","target":"remote"}));
        let old = c.worker.shared.0.lock().unwrap().jobs.generation();
        discover(&mut c, &[1], false);
        assert_eq!(c.profile_name(), "Work");
        assert!(!c.worker.shared.0.lock().unwrap().jobs.current(old));
        assert_eq!(c.policy.ownership(), Ownership::Absent);
        assert!(c.policy.tick(100_000).is_none());
        assert_eq!(c.profiles.profiles.len(), 2);
    }
    #[test]
    fn missing_ambiguous_and_duplicate_displays_fail_closed() {
        let mut c = two_desks();
        for indices in [vec![], vec![0, 1], vec![0, 0]] {
            discover(&mut c, &[0], true);
            c.action(json!({"type":"manual","target":"local"}));
            let old = c.worker.shared.0.lock().unwrap().jobs.generation();
            discover(&mut c, &indices, true);
            assert!(c.config.is_none());
            assert!(!c.editing);
            assert_eq!(c.policy.ownership(), Ownership::Unknown);
            assert!(!c.worker.shared.0.lock().unwrap().jobs.current(old));
            assert_eq!(
                c.action(json!({"type":"manual","target":"local"}))["ok"],
                false
            );
        }
    }
    #[test]
    fn editing_and_new_hold_drafts_until_cancel() {
        let mut c = two_desks();
        discover(&mut c, &[0], true);
        c.action(json!({"type":"edit","enabled":true}));
        discover(&mut c, &[1], true);
        assert_eq!(c.profile_name(), "Home");
        assert_eq!(
            c.action(json!({"type":"manual","target":"local"}))["ok"],
            false
        );
        c.action(json!({"type":"edit","enabled":false}));
        assert_eq!(c.profile_name(), "Work");
        c.action(json!({"type":"new"}));
        assert!(c.config.is_none());
        discover(&mut c, &[0], true);
        assert!(c.config.is_none());
        c.action(json!({"type":"edit","enabled":false}));
        assert_eq!(c.profile_name(), "Home");
        assert_eq!(c.profiles.profiles.len(), 2);
    }
    #[test]
    fn worker_profile_revalidation_rejects_new_ambiguity_and_wrong_desk() {
        let c = two_desks();
        let display = |i: usize| crate::core::Display {
            id: c.profiles.profiles[i].config.display_id.clone(),
            label: "test".into(),
        };
        let inventory = |displays| Inventory {
            displays,
            hubs: vec![],
        };
        let ids = c.profile_ids();
        assert!(require_unique_profile(&ids, &inventory(vec![display(0)]), &ids[0]).is_ok());
        assert!(require_unique_profile(&ids, &inventory(vec![display(1)]), &ids[0]).is_err());
        assert!(
            require_unique_profile(&ids, &inventory(vec![display(0), display(1)]), &ids[0])
                .is_err()
        );
        assert!(
            require_unique_profile(&ids, &inventory(vec![display(0), display(0)]), &ids[0])
                .is_err()
        );
        let mut duplicate = c.profiles.clone();
        duplicate.profiles.push(duplicate.profiles[0].clone());
        assert!(duplicate.validate().is_err());
    }
    #[test]
    fn pause_and_language_save_keep_other_desks() {
        let directory =
            std::env::temp_dir().join(format!("portkey-profile-prefs-test-{}", std::process::id()));
        let path = directory.join("profiles-rust.json");
        let mut c = two_desks();
        c.path = Ok(path.clone());
        for profile in &mut c.profiles.profiles {
            profile.config.automatic = true;
        }
        discover(&mut c, &[0], true);
        assert_eq!(
            c.action(json!({"type":"automatic","enabled":false}))["ok"],
            true
        );
        assert_eq!(
            c.action(json!({"type":"language","value":"zh-Hans"}))["ok"],
            true
        );
        let saved = Profiles::load(&path).unwrap();
        assert_eq!(saved.profiles.len(), 2);
        assert!(!saved.profiles[0].config.automatic);
        assert!(saved.profiles[1].config.automatic);
        assert!(saved
            .profiles
            .iter()
            .all(|p| p.config.language == "zh-Hans"));
        assert_eq!(saved.profiles[0].name, "Home");
        assert_eq!(saved.profiles[1].name, "Work");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn saved_automatic_profile_dispatches_owned_then_absent() {
        let mut profiles = two_desks().profiles;
        profiles.profiles[0].config.automatic = true;
        let mut c = Controller::from_store(Err("no persistence".into()), Ok(profiles));
        c.lifecycle_ok = true;
        c.lifecycle = || (0, false);
        c.lifecycle_epoch = 0;
        discover(&mut c, &[0], true);
        assert!(c.policy.automatic());
        assert_eq!(c.policy.ownership(), Ownership::Owned);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (_, local) = c
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .take()
            .expect("owned must dispatch");
        assert_eq!(local.target, Target::Local);
        discover(&mut c, &[0], false);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (_, remote) = c
            .worker
            .shared
            .0
            .lock()
            .unwrap()
            .jobs
            .take()
            .expect("established departure must dispatch");
        assert_eq!(remote.target, Target::Remote);
    }

    #[test]
    fn display_reconfiguration_reasserts_owned_once_without_usb_transition() {
        let mut c = enrolled();
        c.config = None;
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (_, first) = c.worker.shared.0.lock().unwrap().jobs.take().unwrap();
        assert_eq!(first.target, Target::Local);
        let before = c.worker.shared.0.lock().unwrap().inventory_revision;

        // Ordinary successful scans do not repeat the command.
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());

        // A display reconfiguration loses selection even while USB remains owned.
        discover(&mut c, &[], true);
        assert!(c.config.is_none());
        assert_eq!(c.policy.ownership(), Ownership::Unknown);
        assert_eq!(
            c.worker.shared.0.lock().unwrap().inventory_revision,
            before + 1
        );
        discover(&mut c, &[0], true);
        assert_eq!(
            c.worker.shared.0.lock().unwrap().inventory_revision,
            before + 2
        );
        assert_eq!(c.policy.ownership(), Ownership::Owned);
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        c.started -= Duration::from_secs(1);
        c.snapshot();
        let (_, reassert) = c.worker.shared.0.lock().unwrap().jobs.take().unwrap();
        assert_eq!(reassert.target, Target::Local);
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
    }

    #[test]
    fn inventory_failure_then_absence_does_not_replay_previous_owned_departure() {
        let mut c = enrolled();
        c.config = None;
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_some());

        c.worker.shared.0.lock().unwrap().inventory =
            Some((0, Err("USB inventory changed during enumeration".into())));
        c.snapshot();
        assert!(c.config.is_none());
        assert!(c.inventory.is_none());
        assert_eq!(c.policy.ownership(), Ownership::Unknown);
        discover(&mut c, &[0], false);
        assert_eq!(c.policy.ownership(), Ownership::Absent);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
    }

    #[test]
    fn startup_absent_explains_wait_without_hiding_transport_result() {
        let mut c = two_desks();
        c.config = None;
        c.profiles.profiles[0].config.automatic = true;
        discover(&mut c, &[0], false);
        assert!(c.snapshot()["status"]
            .as_str()
            .unwrap()
            .contains("both enrolled USB hubs"));
        c.status = "Command sent; visual switching is unverified.".into();
        let status = c.snapshot()["status"].as_str().unwrap().to_owned();
        assert!(status.contains("Command sent"));
        assert!(status.contains("both enrolled USB hubs"));
        assert!(c.worker.shared.0.lock().unwrap().jobs.take().is_none());
        discover(&mut c, &[0], true);
        c.started -= Duration::from_secs(1);
        c.snapshot();
        c.worker.shared.0.lock().unwrap().jobs.take();
        discover(&mut c, &[0], false);
        assert!(!c.snapshot()["status"]
            .as_str()
            .unwrap()
            .contains("both enrolled USB hubs"));
    }
}
