//! Versioned per-display enrollment. Legacy single-display data is never overwritten.
use crate::core::{Config, Display};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub profile_id: String,
    pub name: String,
    pub config: Config,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<crate::product::Workspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_draft_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub version: u32,
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub preferred_profiles: std::collections::BTreeMap<String, String>,
}
impl Default for Profiles {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
            preferred_profiles: Default::default(),
        }
    }
}
impl Profiles {
    pub fn from_legacy(config: Option<Config>) -> Self {
        Self {
            version: 1,
            preferred_profiles: Default::default(),
            profiles: config
                .into_iter()
                .map(|config| Profile {
                    profile_id: legacy_profile_id(&config),
                    name: config.display_id.clone(),
                    config,
                    workspace: None,
                    completed_draft_id: None,
                })
                .collect(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported profile store version".into());
        }
        let mut ids = std::collections::HashSet::new();
        for profile in &self.profiles {
            profile.config.validate()?;
            if let Some(workspace) = &profile.workspace {
                workspace.validate()?;
            }
            if profile.config.platform != std::env::consts::OS {
                return Err("Profile belongs to another platform; enroll this computer.".into());
            }
            if profile.name.trim().is_empty() || profile.name.chars().any(char::is_control) {
                return Err("Profile name must contain printable characters".into());
            }
            if !valid_id(&profile.profile_id) || !ids.insert(&profile.profile_id) {
                return Err("Duplicate or invalid connection profile identity".into());
            }
        }
        for (display, id) in &self.preferred_profiles {
            if !self
                .profiles
                .iter()
                .any(|p| &p.profile_id == id && &p.config.display_id == display)
            {
                return Err(
                    "Remembered profile choice is missing or belongs to another display.".into(),
                );
            }
        }
        Ok(())
    }
    pub fn normalize_ids(&mut self) {
        for profile in &mut self.profiles {
            if profile.profile_id.is_empty() {
                profile.profile_id = legacy_profile_id(&profile.config);
            }
        }
    }
    pub fn unique_for_display(&self, display_id: &str) -> Result<Option<&Profile>, String> {
        let mut profiles = self
            .profiles
            .iter()
            .filter(|p| p.config.display_id == display_id);
        let first = profiles.next();
        if profiles.next().is_some() {
            Err(
                "Multiple connection profiles match this display. Choose a profile explicitly."
                    .into(),
            )
        } else {
            Ok(first)
        }
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut store: Self = match fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("Invalid profile store: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Self::from_legacy(Config::load(&path.with_file_name("config-rust.json"))?)
            }
            Err(e) => return Err(format!("Cannot read profile store: {e}")),
        };
        store.normalize_ids();
        store.validate()?;
        Ok(store)
    }
    pub fn upsert(&mut self, name: Option<String>, config: Config) -> Result<(), String> {
        let id = self
            .unique_for_display(&config.display_id)?
            .map(|p| p.profile_id.clone())
            .unwrap_or_else(new_profile_id);
        self.upsert_id(id, name, config)
    }
    pub fn upsert_id(
        &mut self,
        profile_id: String,
        name: Option<String>,
        config: Config,
    ) -> Result<(), String> {
        let previous = self
            .profiles
            .iter()
            .position(|p| p.profile_id == profile_id);
        let name = name
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.trim().to_owned())
            .or_else(|| previous.map(|i| self.profiles[i].name.clone()))
            .unwrap_or_else(|| config.display_id.clone());
        let profile = Profile {
            profile_id,
            name,
            config,
            workspace: previous.and_then(|i| self.profiles[i].workspace.clone()),
            completed_draft_id: previous.and_then(|i| self.profiles[i].completed_draft_id.clone()),
        };
        let mut next = self.clone();
        // A re-enrollment to another display cannot retain a remembered choice for the old display.
        next.preferred_profiles.retain(|display, id| {
            id != &profile.profile_id || display == &profile.config.display_id
        });
        if let Some(index) = previous {
            next.profiles[index] = profile;
        } else {
            next.profiles.push(profile);
        }
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub fn selected<'a>(&'a self, displays: &[Display]) -> Result<Option<&'a Profile>, String> {
        let matches: Vec<_> = displays
            .iter()
            .filter(|d| self.profiles.iter().any(|p| p.config.display_id == d.id))
            .collect();
        let display=match matches.as_slice() { []=>return Ok(None), [display]=>*display, _=>return Err("Multiple enrolled displays are connected; disconnect all but one before switching.".into()) };
        if let Some(id) = self.preferred_profiles.get(&display.id) {
            return Ok(self.profiles.iter().find(|p| &p.profile_id == id));
        }
        self.unique_for_display(&display.id)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if path.exists() {
            let backup = path.with_file_name(format!(
                "{}.pre-product.bak",
                path.file_name()
                    .ok_or("Profile path needs a file name")?
                    .to_string_lossy()
            ));
            if !backup.exists() {
                let bytes = fs::read(path)
                    .map_err(|e| format!("Cannot preserve original profiles: {e}"))?;
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut original = options
                    .open(&backup)
                    .map_err(|e| format!("Cannot preserve original profiles: {e}"))?;
                if let Err(error) = original.write_all(&bytes).and_then(|_| original.sync_all()) {
                    drop(original);
                    let _ = fs::remove_file(&backup);
                    return Err(format!("Cannot preserve original profiles: {error}"));
                }
            }
        }
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("Profile path needs a parent directory")?;
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create profile directory: {e}"))?;
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let temp = parent.join(format!(
            ".profiles-{}-{}.tmp",
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
            .map_err(|e| format!("Cannot create profile file: {e}"))?;
        let result = (|| {
            file.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("Cannot write profiles: {e}"))?;
            drop(file);
            fs::rename(&temp, path).map_err(|e| format!("Cannot replace profiles: {e}"))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }
}

pub fn new_profile_id() -> String {
    format!(
        "profile-{}",
        crate::product::Workspace::default().workspace_id
    )
}
fn legacy_profile_id(config: &Config) -> String {
    // Fixed FNV-1a keeps unsaved legacy identities stable across Rust/toolchain upgrades.
    let hash = config
        .platform
        .bytes()
        .chain([0])
        .chain(config.display_id.bytes())
        .fold(0xcbf29ce484222325u64, |value, byte| {
            (value ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("profile-legacy1-{hash:016x}")
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 120 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
