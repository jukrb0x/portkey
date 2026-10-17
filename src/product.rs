//! Local product metadata. Qualification is scoped evidence, never peer presence.
use crate::core::Config;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Evidence {
    pub local: Direction,
    pub remote: Direction,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Direction {
    pub result: String,
    pub source: String,
    pub note: String,
}
impl Default for Direction {
    fn default() -> Self {
        Self {
            result: "unverified".into(),
            source: "none".into(),
            note: String::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Workspace {
    pub local_name: String,
    pub remote_name: String,
    pub local_os: String,
    pub remote_os: String,
    pub local_role: String,
    pub workspace_id: String,
    pub revision: u64,
    pub mode: String,
    pub single_controller: String,
    pub verification: String,
    pub evidence: Evidence,
    pub peer_confirmed: bool,
    pub peer_paused_confirmed: bool,
    pub(crate) binding: Option<Config>,
}
impl Default for Workspace {
    fn default() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            local_name: "This computer".into(),
            remote_name: "Other computer".into(),
            local_os: std::env::consts::OS.into(),
            remote_os: "unknown".into(),
            local_role: "a".into(),
            workspace_id: format!(
                "pk-{:x}-{:x}-{:x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ),
            revision: 1,
            mode: "unverified".into(),
            single_controller: "local".into(),
            verification: "not_checked".into(),
            evidence: Evidence::default(),
            peer_confirmed: false,
            peer_paused_confirmed: false,
            binding: None,
        }
    }
}
pub fn validate_name(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().count() > 80 || value.chars().any(char::is_control)
    {
        Err("Names must contain 1 to 80 printable characters.".into())
    } else {
        Ok(())
    }
}
impl Workspace {
    pub fn edit(&self, value: Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("Workspace must be an object.")?;
        if object.get("verification").and_then(Value::as_str) == Some("historical_unknown") {
            return Err("Historical provenance is not supported for new qualification.".into());
        }
        if object.contains_key("binding") {
            return Err("Qualification binding cannot be supplied by the frontend.".into());
        }
        let mut merged = serde_json::to_value(self).map_err(|e| e.to_string())?;
        for (key, value) in object {
            merged[key] = value.clone();
        }
        let mut next: Self =
            serde_json::from_value(merged).map_err(|e| format!("Invalid workspace: {e}"))?;
        next.local_name = next.local_name.trim().into();
        next.remote_name = next.remote_name.trim().into();
        if next.mode != self.mode
            || next.single_controller != self.single_controller
            || next.local_role != self.local_role
            || next.workspace_id != self.workspace_id
            || next.local_os != self.local_os
            || next.remote_os != self.remote_os
            || next.evidence != self.evidence
            || next.revision != self.revision
        {
            next.binding = None;
        }
        next.validate()?;
        Ok(next)
    }
    pub fn validate(&self) -> Result<(), String> {
        validate_name(&self.local_name)?;
        validate_name(&self.remote_name)?;
        if self.local_name.trim().to_lowercase() == self.remote_name.trim().to_lowercase() {
            return Err("Give the two computers distinguishable names.".into());
        }
        if self.local_os != std::env::consts::OS
            || !matches!(self.remote_os.as_str(), "macos" | "windows" | "unknown")
            || !matches!(self.local_role.as_str(), "a" | "b")
            || !matches!(self.mode.as_str(), "single" | "dual" | "unverified")
            || !matches!(self.single_controller.as_str(), "local" | "remote")
            || !matches!(
                self.verification.as_str(),
                "not_checked"
                    | "historical_unknown"
                    | "single_observed"
                    | "single_inconclusive"
                    | "dual_observed"
                    | "needs_attention"
            )
        {
            return Err("Invalid computer, installation mode or qualification choice.".into());
        }
        if self.workspace_id.is_empty()
            || self.workspace_id.len() > 120
            || !self
                .workspace_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || self.revision == 0
        {
            return Err("Invalid workspace identity or revision.".into());
        }
        for d in [&self.evidence.local, &self.evidence.remote] {
            if !matches!(
                d.result.as_str(),
                "unverified" | "observed" | "failed" | "inconclusive"
            ) || !matches!(
                d.source.as_str(),
                "none" | "readback" | "user_observed" | "imported"
            ) || (d.result == "observed" && d.source == "none")
                || d.note.chars().count() > 500
                || d.note.chars().any(char::is_control)
            {
                return Err("Invalid directional evidence or provenance.".into());
            }
        }
        if matches!(
            self.verification.as_str(),
            "single_observed" | "dual_observed"
        ) {
            if self.remote_os == "unknown"
                || self.evidence.local.result != "observed"
                || self.evidence.remote.result != "observed"
            {
                return Err(
                    "Both directions require explicit observed evidence and computer OS choices."
                        .into(),
                );
            }
            if self.verification == "single_observed"
                && (self.mode != "single" || !self.peer_paused_confirmed)
            {
                return Err("Single-controller qualification requires that the other instance was paused or quit.".into());
            }
            if self.verification == "dual_observed" && (self.mode != "dual" || !self.peer_confirmed)
            {
                return Err("Dual qualification requires confirmed reciprocal enrollment.".into());
            }
        }
        Ok(())
    }
    pub fn attest(&mut self, config: &Config) -> Result<(), String> {
        self.validate()?;
        self.binding = Some(config.clone());
        Ok(())
    }
    pub fn current(&self, config: &Config) -> bool {
        self.binding
            .as_ref()
            .is_some_and(|b| same_binding(b, config))
    }

    pub fn can_enable(&self, config: &Config) -> bool {
        self.current(config)
            && ((self.mode == "single"
                && self.single_controller == "local"
                && self.verification == "single_observed")
                || (self.mode == "dual" && self.verification == "dual_observed"))
    }
    pub fn public(&self) -> Value {
        let mut v = json!(self);
        v.as_object_mut().unwrap().remove("binding");
        v
    }
}

pub fn same_binding(a: &Config, b: &Config) -> bool {
    a.display_id == b.display_id
        && a.platform == b.platform
        && a.local_input == b.local_input
        && a.remote_input == b.remote_input
        && a.hubs
            .iter()
            .map(|h| &h.id)
            .collect::<std::collections::HashSet<_>>()
            == b.hubs.iter().map(|h| &h.id).collect()
}
