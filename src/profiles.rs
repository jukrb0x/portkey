//! Versioned per-display enrollment. Legacy single-display data is never overwritten.
use crate::core::{Config, Display};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub config: Config,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub version: u32,
    pub profiles: Vec<Profile>,
}
impl Default for Profiles {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![],
        }
    }
}
impl Profiles {
    pub fn from_legacy(config: Option<Config>) -> Self {
        Self {
            version: 1,
            profiles: config
                .into_iter()
                .map(|config| Profile {
                    name: config.display_id.clone(),
                    config,
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
            if profile.config.platform != std::env::consts::OS {
                return Err("Profile belongs to another platform; enroll this computer.".into());
            }
            if profile.name.trim().is_empty() || profile.name.chars().any(char::is_control) {
                return Err("Profile name must contain printable characters".into());
            }
            if !ids.insert(&profile.config.display_id) {
                return Err("Duplicate display profile identity".into());
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let store = match fs::read(path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("Invalid profile store: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Self::from_legacy(Config::load(&path.with_file_name("config-rust.json"))?)
            }
            Err(e) => return Err(format!("Cannot read profile store: {e}")),
        };
        store.validate()?;
        Ok(store)
    }
    pub fn upsert(&mut self, name: Option<String>, config: Config) -> Result<(), String> {
        let previous = self
            .profiles
            .iter()
            .position(|p| p.config.display_id == config.display_id);
        let name = name
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.trim().to_owned())
            .or_else(|| previous.map(|i| self.profiles[i].name.clone()))
            .unwrap_or_else(|| config.display_id.clone());
        let profile = Profile { name, config };
        let mut next = self.clone();
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
            .filter_map(|d| self.profiles.iter().find(|p| p.config.display_id == d.id))
            .collect();
        match matches.as_slice() { [] => Ok(None), [profile] => Ok(Some(*profile)), _ => Err("Multiple enrolled displays are connected; disconnect all but one before switching.".into()) }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
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
