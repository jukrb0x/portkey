use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hub {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub platform: String,
    pub display_id: String,
    pub local_input: u8,
    pub remote_input: u8,
    pub hubs: Vec<Hub>,
    pub automatic: bool,
    pub language: String,
}
impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(
                "Unsupported configuration version; re-enroll with this version of PortKey".into(),
            );
        }
        if !matches!(self.platform.as_str(), "macos" | "windows") {
            return Err("Configuration requires macos or windows platform".into());
        }
        let valid_display = if self.platform == "macos" {
            let bytes = self.display_id.as_bytes();
            bytes.len() == 36
                && bytes.iter().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        *b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                })
        } else {
            self.display_id.starts_with(r"\\?\display#") && self.display_id.len() > 20
        };
        if !valid_display {
            return Err("Choose a display with a stable platform identity".into());
        }
        if self.local_input == 0 || self.remote_input == 0 || self.local_input == self.remote_input
        {
            return Err("Local and remote inputs must be distinct numbers from 1 to 255".into());
        }
        if !matches!(self.language.as_str(), "en" | "zh-Hans") {
            return Err("Unsupported language".into());
        }
        if self.hubs.len() != 2 || self.hubs[0].id == self.hubs[1].id {
            return Err("Enroll two different shared hubs".into());
        }
        for hub in &self.hubs {
            if !valid_hub_id(&self.platform, &hub.id) {
                return Err("Each hub requires a complete stable USB topology; re-enroll after moving ports".into());
            }
        }
        if self.platform == "macos"
            && self.hubs[0].id.split(':').nth(3) == self.hubs[1].id.split(':').nth(3)
        {
            return Err("Choose hubs at two different USB locations".into());
        }
        Ok(())
    }
    pub fn observe(&self, hubs: &[Hub]) -> Ownership {
        if self.validate().is_err() {
            return Ownership::Unknown;
        }
        let counts: Vec<_> = self
            .hubs
            .iter()
            .map(|enrolled| hubs.iter().filter(|h| h.id == enrolled.id).count())
            .collect();
        match counts.as_slice() {
            [1, 1] => Ownership::Owned,
            [0, 0] => Ownership::Absent,
            [1, 0] | [0, 1] => Ownership::Partial,
            _ => Ownership::Unknown,
        }
    }
    pub fn input(&self, target: Target) -> u8 {
        if target == Target::Local {
            self.local_input
        } else {
            self.remote_input
        }
    }
    pub fn path() -> Result<PathBuf, String> {
        dirs::config_dir()
            .map(|p| p.join("PortKey/config-rust.json"))
            .ok_or_else(|| "Cannot locate the per-user configuration directory".into())
    }
    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Cannot read configuration: {e}")),
        };
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid configuration JSON: {e}"))?;
        config.validate()?;
        Ok(Some(config))
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("Configuration path needs a parent directory")?;
        fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create configuration directory: {e}"))?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let temp = parent.join(format!(
            ".portkey-{}-{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("Cannot create temporary configuration: {e}"))?;
        let result = (|| {
            let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
            file.write_all(&data)
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("Cannot write configuration: {e}"))?;
            drop(file);
            fs::rename(&temp, path).map_err(|e| format!("Cannot replace configuration: {e}"))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
    /// Import only on explicit request; callers choose whether to save into the separate Rust file.
    pub fn import_legacy(path: &Path) -> Result<Self, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LegacyHub {
            vendor: u32,
            product: u32,
            location: u32,
            parent_vendor: u32,
            parent_product: u32,
            parent_location: u32,
        }
        impl LegacyHub {
            fn hub(self, label: &str) -> Hub {
                Hub {
                    id: format!(
                        "macos:{:04x}:{:04x}:{:08x}:{:04x}:{:04x}:{:08x}",
                        self.vendor,
                        self.product,
                        self.location,
                        self.parent_vendor,
                        self.parent_product,
                        self.parent_location
                    ),
                    label: label.into(),
                }
            }
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Legacy {
            monitor: String,
            mac_input: u8,
            windows_input: u8,
            usb2: LegacyHub,
            usb3: LegacyHub,
        }
        let bytes = fs::read(path).map_err(|e| format!("Cannot read legacy configuration: {e}"))?;
        let old: Legacy = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Invalid legacy configuration: {e}"))?;
        let config = Self {
            version: 1,
            platform: "macos".into(),
            display_id: old.monitor.to_ascii_uppercase(),
            local_input: old.mac_input,
            remote_input: old.windows_input,
            hubs: vec![
                old.usb2.hub("Imported USB2 hub"),
                old.usb3.hub("Imported USB3 hub"),
            ],
            automatic: false,
            language: "en".into(),
        };
        config.validate()?;
        Ok(config)
    }
}
fn valid_hub_id(platform: &str, id: &str) -> bool {
    if platform == "windows" {
        return id.starts_with("windows:")
            && id.contains("#usb(")
            && id.ends_with(')')
            && id == id.to_ascii_lowercase()
            && !id.chars().any(char::is_whitespace);
    }
    let fields: Vec<_> = id.split(':').collect();
    let root = fields.len() == 6 && fields[4] == "root";
    if (!root && fields.len() != 7) || fields[0] != "macos" {
        return false;
    }
    let numeric = if root { &fields[1..4] } else { &fields[1..] };
    let mut values = Vec::new();
    for (part, width) in numeric.iter().zip([4, 4, 8, 4, 4, 8]) {
        if part.len() != width
            || part
                .chars()
                .any(|c| !c.is_ascii_hexdigit() || c.is_ascii_uppercase())
        {
            return false;
        }
        match u32::from_str_radix(part, 16) {
            Ok(value) if value > 0 => values.push(value),
            _ => return false,
        }
    }
    if root {
        let hex = fields[5];
        if hex.len() % 2 != 0
            || !hex
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return false;
        }
        let decoded: Vec<u8> = hex
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        return std::str::from_utf8(&decoded).is_ok_and(|path| {
            path.starts_with("IOService:/")
                && path.len() > "IOService:/".len()
                && !path.chars().any(char::is_control)
        });
    }
    values[2] != values[5]
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ownership {
    Unknown,
    Owned,
    Absent,
    Partial,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Local,
    Remote,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub target: Target,
    pub generation: u64,
}
pub struct Policy {
    ownership: Ownership,
    automatic: bool,
    sleeping: bool,
    established: bool,
    generation: u64,
    request: Option<Request>,
    deadline: Option<u64>,
    manual_anchor: Option<Ownership>,
    stable: Ownership,
}
impl Default for Policy {
    fn default() -> Self {
        Self::new()
    }
}
impl Policy {
    pub fn new() -> Self {
        Self {
            ownership: Ownership::Unknown,
            automatic: true,
            sleeping: false,
            established: false,
            generation: 0,
            request: None,
            deadline: None,
            manual_anchor: None,
            stable: Ownership::Unknown,
        }
    }
    pub fn observe(&mut self, value: Ownership, now_ms: u64) {
        if self.sleeping || value == self.ownership {
            return;
        }
        self.ownership = value;
        if value == Ownership::Unknown {
            self.established = false;
            self.invalidate();
            return;
        }
        if matches!(value, Ownership::Owned | Ownership::Absent) {
            if self.manual_anchor.is_some_and(|anchor| anchor != value) {
                self.manual_anchor = None;
                self.invalidate();
            }
            self.stable = value;
        }
        if self.manual_anchor.is_some() {
            return;
        }
        self.invalidate();
        if !self.automatic {
            return;
        }
        if value == Ownership::Owned {
            self.deadline = Some(now_ms.saturating_add(300));
        }
        if value == Ownership::Absent && self.established {
            self.deadline = Some(now_ms.saturating_add(700));
        }
    }
    pub fn tick(&mut self, now_ms: u64) -> Option<Request> {
        if self.deadline.is_some_and(|due| now_ms >= due) && self.automatic && !self.sleeping {
            self.deadline = None;
            if self.ownership == Ownership::Owned {
                self.established = true;
                self.issue(Target::Local);
            } else if self.ownership == Ownership::Absent && self.established {
                self.issue(Target::Remote);
            }
        }
        self.request
    }
    pub fn request(&self) -> Option<Request> {
        self.request
    }
    pub fn ownership(&self) -> Ownership {
        self.ownership
    }
    pub fn automatic(&self) -> bool {
        self.automatic
    }
    pub fn sleeping(&self) -> bool {
        self.sleeping
    }
    pub fn manual(&mut self, target: Target) {
        if self.sleeping {
            return;
        }
        self.invalidate();
        self.manual_anchor = Some(self.stable);
        self.issue(target);
    }
    pub fn set_automatic(&mut self, enabled: bool, now_ms: u64) {
        self.automatic = enabled;
        self.reset();
        if matches!(self.ownership, Ownership::Owned | Ownership::Absent) {
            self.stable = self.ownership;
        }
        if enabled && self.ownership == Ownership::Owned {
            self.deadline = Some(now_ms.saturating_add(300));
        }
    }
    pub fn sleep(&mut self) {
        self.sleeping = true;
        self.reset();
        self.ownership = Ownership::Unknown;
    }
    pub fn wake(&mut self, _now_ms: u64) {
        self.sleeping = false;
        self.reset();
        self.ownership = Ownership::Unknown;
    }
    pub fn monitor_reconnected(&mut self, now_ms: u64) {
        self.invalidate();
        if self.automatic
            && !self.sleeping
            && self.manual_anchor.is_none()
            && self.ownership == Ownership::Owned
        {
            self.deadline = Some(now_ms.saturating_add(300));
        }
    }
    fn reset(&mut self) {
        self.invalidate();
        self.established = false;
        self.manual_anchor = None;
        self.stable = Ownership::Unknown;
    }
    fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.request = None;
        self.deadline = None;
    }
    fn issue(&mut self, target: Target) {
        self.generation = self.generation.wrapping_add(1);
        self.request = Some(Request {
            target,
            generation: self.generation,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn owned() -> Policy {
        let mut p = Policy::new();
        p.observe(Ownership::Owned, 0);
        assert_eq!(p.tick(300).unwrap().target, Target::Local);
        p
    }
    #[test]
    fn startup_absent_and_partial_never_handoff() {
        let mut p = Policy::new();
        for (i, value) in [Ownership::Absent, Ownership::Partial, Ownership::Absent]
            .into_iter()
            .enumerate()
        {
            p.observe(value, i as u64 * 1000);
            assert!(p.tick(i as u64 * 1000 + 999).is_none());
        }
    }
    #[test]
    fn arrival_settles_without_snapshot_starvation() {
        let mut p = Policy::new();
        p.observe(Ownership::Owned, 0);
        assert!(p.tick(299).is_none());
        p.observe(Ownership::Owned, 299);
        assert_eq!(p.tick(300).unwrap().target, Target::Local);
    }
    #[test]
    fn departure_waits_and_partial_invalidates_requests() {
        let mut p = owned();
        let previous = p.request().unwrap();
        p.observe(Ownership::Absent, 400);
        assert!(p.tick(1099).is_none());
        let next = p.tick(1100).unwrap();
        assert_eq!(next.target, Target::Remote);
        assert!(next.generation > previous.generation);
        p.observe(Ownership::Partial, 1200);
        assert!(p.request().is_none());
    }
    #[test]
    fn unstable_arrival_does_not_establish_ownership() {
        let mut p = Policy::new();
        p.observe(Ownership::Owned, 0);
        p.observe(Ownership::Absent, 200);
        assert!(p.tick(1000).is_none());
    }
    #[test]
    fn unknown_discards_prior_ownership() {
        let mut p = owned();
        p.observe(Ownership::Unknown, 400);
        p.observe(Ownership::Absent, 500);
        assert!(p.tick(2000).is_none());
    }
    #[test]
    fn manual_survives_partial_bounce_but_next_transition_resumes() {
        let mut p = owned();
        p.manual(Target::Remote);
        let manual = p.request().unwrap();
        p.observe(Ownership::Partial, 400);
        p.observe(Ownership::Owned, 500);
        assert_eq!(p.tick(1000), Some(manual));
        p.observe(Ownership::Absent, 1100);
        assert!(p.request().is_none());
        assert_eq!(p.tick(1800).unwrap().target, Target::Remote);
    }
    #[test]
    fn sleep_cancels_and_wake_absence_is_unknown() {
        let mut p = owned();
        p.observe(Ownership::Absent, 400);
        p.sleep();
        p.manual(Target::Remote);
        p.observe(Ownership::Owned, 2000);
        assert!(p.tick(3000).is_none());
        p.wake(4000);
        p.observe(Ownership::Absent, 4000);
        assert!(p.tick(5000).is_none());
        p.observe(Ownership::Owned, 6000);
        assert_eq!(p.tick(6300).unwrap().target, Target::Local);
    }
    #[test]
    fn pause_resume_absent_cannot_handoff() {
        let mut p = owned();
        p.set_automatic(false, 400);
        p.observe(Ownership::Absent, 500);
        p.set_automatic(true, 600);
        assert!(p.tick(2000).is_none());
        p.manual(Target::Local);
        assert_eq!(p.request().unwrap().target, Target::Local);
    }
    #[test]
    fn monitor_reconnect_reclaims_only_owned_without_manual_override() {
        let mut p = owned();
        let generation = p.request().unwrap().generation;
        p.monitor_reconnected(500);
        assert!(p.tick(799).is_none());
        assert!(p.tick(800).unwrap().generation > generation);
        p.manual(Target::Remote);
        p.monitor_reconnected(900);
        assert!(p.tick(2000).is_none());
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;
    fn config() -> Config {
        Config {
            version: 1,
            platform: "macos".into(),
            display_id: "12345678-1234-1234-1234-123456789abc".into(),
            local_input: 17,
            remote_input: 15,
            hubs: vec![
                Hub {
                    id: "macos:05e3:0610:02121000:05e3:0610:02120000".into(),
                    label: "USB2".into(),
                },
                Hub {
                    id: "macos:05e3:0626:02210000:05e3:0626:02200000".into(),
                    label: "USB3".into(),
                },
            ],
            automatic: true,
            language: "en".into(),
        }
    }
    fn temp() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::env::temp_dir()
            .join(format!(
                "portkey-core-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ))
            .join("config.json")
    }
    #[test]
    fn refuses_unsupported_version_and_incomplete_enrollment() {
        let c = config();
        assert!(c.validate().is_ok());
        let mut bad = c.clone();
        bad.version = 99;
        assert!(bad.validate().is_err());
        let mut bad = c.clone();
        bad.local_input = bad.remote_input;
        assert!(bad.validate().is_err());
        let mut bad = c.clone();
        bad.display_id.clear();
        assert!(bad.validate().is_err());
        let mut bad = c.clone();
        bad.hubs[1] = bad.hubs[0].clone();
        assert!(bad.validate().is_err());
        let mut bad = c.clone();
        bad.hubs[0].id = "05e3:0610".into();
        assert!(bad.validate().is_err());
        let mut bad = c;
        bad.hubs[0].id = "macos:05e3:0610:02121000:0000:0000:00000000".into();
        assert!(bad.validate().is_err());
    }
    #[test]
    fn topology_and_identity_not_label_determine_ownership() {
        let c = config();
        let mut inventory = c.hubs.clone();
        inventory[0].label = "Renamed".into();
        assert_eq!(c.observe(&inventory), Ownership::Owned);
        inventory[0].id = "macos:05e3:0610:03121000:05e3:0610:03120000".into();
        assert_eq!(c.observe(&inventory), Ownership::Partial);
        assert_eq!(c.observe(&[]), Ownership::Absent);
        assert_eq!(
            c.observe(&[c.hubs[0].clone(), c.hubs[0].clone(), c.hubs[1].clone()]),
            Ownership::Unknown
        );
    }
    #[test]
    fn verified_root_topology_enrolls_but_controller_relocation_is_distinct() {
        let mut c = config();
        c.hubs[0].id = "macos:05e3:0610:02121000:root:494f536572766963653a2f5553424031".into();
        assert!(c.validate().is_ok());
        assert_eq!(c.observe(&c.hubs), Ownership::Owned);
        let mut relocated = c.hubs.clone();
        relocated[0].id = "macos:05e3:0610:02121000:root:494f536572766963653a2f5553424032".into();
        assert_eq!(c.observe(&relocated), Ownership::Partial);
    }
    #[test]
    fn root_topology_rejects_missing_malformed_and_noncanonical_paths() {
        for path in [
            "",
            "0",
            "zz",
            "494f536572766963653a2f",
            "494f536572766963653a2f00",
            "494f536572766963653a2fff",
            "494F536572766963653a2f5553424031",
            "5553424031",
        ] {
            let mut c = config();
            c.hubs[0].id = format!("macos:05e3:0610:02121000:root:{path}");
            assert!(c.validate().is_err(), "unexpectedly accepted {path}");
        }
        let mut c = config();
        c.hubs[0].id = "macos:0000:0610:02121000:root:494f536572766963653a2f5553424031".into();
        assert!(c.validate().is_err());
    }
    #[test]
    fn save_load_replace_and_reject_invalid_without_data_loss() {
        let path = temp();
        assert_eq!(Config::load(&path).unwrap(), None);
        let mut c = config();
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), Some(c.clone()));
        c.language = "zh-Hans".into();
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), Some(c.clone()));
        let bytes = fs::read(&path).unwrap();
        c.version = 2;
        assert!(c.save(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::write(&path, "{}").unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn legacy_import_is_explicit_and_preserves_source() {
        let path = temp();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let raw = r#"{"monitor":"12345678-1234-1234-1234-123456789abc","macInput":17,"windowsInput":15,"usb2":{"vendor":1507,"product":1552,"location":34738176,"parentVendor":1507,"parentProduct":1552,"parentLocation":34734080},"usb3":{"vendor":1507,"product":1574,"location":35717120,"parentVendor":1507,"parentProduct":1574,"parentLocation":35651584}}"#;
        fs::write(&path, raw).unwrap();
        let c = Config::import_legacy(&path).unwrap();
        assert_eq!(c.display_id, "12345678-1234-1234-1234-123456789ABC");
        assert_eq!(c.hubs[0].id, "macos:05e3:0610:02121000:05e3:0610:02120000");
        assert!(!c.automatic);
        assert_eq!(fs::read_to_string(&path).unwrap(), raw);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
