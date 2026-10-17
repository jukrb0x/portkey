//! Guided enrollment records topology changes and measured inputs, never guesses them.
use crate::core::{Config, Hub};
use crate::product::Workspace;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Setup {
    pub phase: String,
    pub message: String,
    pub error: bool,
    pub display_id: String,
    pub name: String,
    pub profile_id: String,
    #[serde(default)]
    pub target_profile_id: Option<String>,
    pub local_input: Option<u8>,
    pub remote_input: Option<u8>,
    pub code: Option<String>,
    pub hub_count: usize,
    pub ready: bool,
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub product_configured: bool,
    #[serde(default)]
    pub draft_saved: bool,
    pub draft_id: String,
    #[serde(default = "local_role")]
    pub input_role: String,
    #[serde(default)]
    pub peer_preview: Option<Exchange>,
    #[serde(skip)]
    baseline: Vec<Hub>,
    hubs: Vec<Hub>,
    #[serde(skip)]
    stable_since: Option<u64>,
    #[serde(skip)]
    wait_started: Option<u64>,
}
impl Setup {
    pub fn new(display_id: String, name: String) -> Self {
        Self {
            phase: "display".into(),
            message: "Keep USB on this computer, then start USB detection.".into(),
            error: false,
            display_id,
            name,
            profile_id: crate::profiles::new_profile_id(),
            target_profile_id: None,
            local_input: None,
            remote_input: None,
            code: None,
            hub_count: 0,
            ready: false,
            workspace: Workspace::default(),
            product_configured: false,
            draft_saved: false,
            draft_id: new_draft_id(),
            input_role: "local".into(),
            peer_preview: None,
            baseline: vec![],
            hubs: vec![],
            stable_since: None,
            wait_started: None,
        }
    }
    pub fn capture_usb(&mut self) {
        self.interrupted();
        self.baseline.clear();
        self.stable_since = None;
        self.wait_started = None;
        self.phase = "usb_capturing".into();
        self.error = false;
        self.message = "Keep USB on this computer while PortKey captures a fresh baseline. Wait for the next instruction before switching.".into();
    }
    pub fn start_usb(&mut self, hubs: &[Hub]) -> Result<(), String> {
        self.peer_preview = None;
        self.workspace.binding = None;
        self.workspace.verification = "not_checked".into();
        self.workspace.peer_confirmed = false;
        if hubs.is_empty() {
            return Err("No USB hubs detected. Connect the switch to this computer first.".into());
        }
        if hubs
            .iter()
            .enumerate()
            .any(|(i, h)| hubs[..i].iter().any(|v| v.id == h.id))
        {
            return Err("USB topology is ambiguous; reconnect and retry.".into());
        }
        self.local_input = None;
        self.remote_input = None;
        self.code = None;
        self.baseline = hubs.to_vec();
        self.hubs.clear();
        self.hub_count = 0;
        self.stable_since = None;
        self.wait_started = None;
        self.ready = false;
        self.phase = "usb_wait_departure".into();
        self.message =
            "Switch USB to the other computer and keep it there until PortKey confirms departure. Detection may take 15 seconds. Keep the monitor input unchanged.".into();
        self.error = false;
        Ok(())
    }
    pub fn observe(&mut self, observed: &[Hub], now: u64) {
        if !matches!(
            self.phase.as_str(),
            "usb_wait_departure" | "usb_wait_return"
        ) {
            return;
        }
        let started = *self.wait_started.get_or_insert(now);
        if now.saturating_sub(started) >= 60_000 {
            self.phase = "usb_retry".into();
            self.fail("USB detection timed out. Return USB to this computer, retry detection, and wait for each on-screen instruction.");
            return;
        }
        let mut changed = false;
        for hub in &self.baseline {
            if !observed.iter().any(|h| h.id == hub.id) && !self.hubs.iter().any(|h| h.id == hub.id)
            {
                self.hubs.push(hub.clone());
                changed = true;
            }
        }
        self.hub_count = self.hubs.len();
        if self.hub_count > 2 {
            self.fail("More than two hubs changed. Disconnect other changing hubs and restart USB detection.");
            self.phase = "display".into();
            return;
        }
        if changed {
            self.stable_since = None;
            self.phase = "usb_wait_departure".into();
        }
        if self.hubs.is_empty() {
            return;
        }
        let absent = self
            .hubs
            .iter()
            .all(|h| !observed.iter().any(|v| v.id == h.id));
        let returned = self
            .hubs
            .iter()
            .all(|h| observed.iter().filter(|v| v.id == h.id).count() == 1);
        if self.phase == "usb_wait_departure" && returned {
            self.phase = "usb_retry".into();
            self.fail("USB returned before departure could be confirmed. Keep USB here, retry detection, then wait for PortKey to ask you to switch back.");
            return;
        }
        let stable = match self.phase.as_str() {
            "usb_wait_departure" => absent,
            _ => returned,
        };
        if !stable {
            self.stable_since = None;
            return;
        }
        let since = *self.stable_since.get_or_insert(now);
        if now.saturating_sub(since) < 2000 {
            return;
        }
        self.stable_since = None;
        if self.phase == "usb_wait_departure" {
            self.phase = "usb_wait_return".into();
            self.wait_started = Some(now);
            self.message =
                "USB departure detected. Switch USB back to this computer and wait.".into();
        } else {
            self.phase = "usb_ready".into();
            self.message = "USB switch detected. Show this computer on the selected monitor, then read its input.".into();
        }
    }
    pub fn input_result(&mut self, result: Result<u8, String>) {
        match result {
            Ok(input) if input != 0 => {
                self.peer_preview = None;
                self.workspace.peer_confirmed = false;
                if self.input_role == "remote" {
                    if self.local_input == Some(input) {
                        self.fail("Both computers use the same input. Select the other picture and retry.");
                        self.phase = "exchange".into();
                        return;
                    }
                    self.remote_input = Some(input);
                } else {
                    self.local_input = Some(input);
                    if self.remote_input == Some(input) {
                        self.remote_input = None;
                    }
                }
                self.workspace.binding = None;
                self.workspace.verification = "not_checked".into();
                self.code = Some(input_code(self.local_input.unwrap_or(input)));
                self.phase = "exchange".into();
                self.message =
                    "Copy this code to the other computer. Enter the code shown there to finish."
                        .into();
                self.error = false;
                self.update_ready();
                if self.product_configured {
                    self.code = self.export().ok();
                }
            }
            Ok(_) => {
                self.phase = "usb_ready".into();
                self.fail("The monitor returned an invalid input. Show this computer on the monitor and retry.");
            }
            Err(error) => {
                self.phase = "usb_ready".into();
                self.fail(&error);
            }
        }
    }
    pub fn peer_code(&mut self, code: &str) -> Result<(), String> {
        let input = parse_code(code)?;
        if self.local_input.is_none() {
            return Err("Read this computer's monitor input first.".into());
        }
        if self.local_input == Some(input) {
            return Err("Both computers reported the same input. Show each computer on its own monitor input and read again.".into());
        }
        self.remote_input = Some(input);
        self.peer_preview = None;
        self.workspace.binding = None;
        self.workspace.verification = "not_checked".into();
        self.workspace.peer_confirmed = false;
        self.error = false;
        self.update_ready();
        Ok(())
    }
    fn update_ready(&mut self) {
        self.ready = !self.hubs.is_empty()
            && self.local_input.is_some()
            && self.remote_input.is_some()
            && self.local_input != self.remote_input;
        if self.ready {
            self.phase = "ready".into();
            self.message =
                "Ready to save. Automatic switching will stay paused until you enable it.".into();
        }
    }
    pub fn fail(&mut self, message: &str) {
        self.error = true;
        self.message = message.into();
    }
    pub fn interrupted(&mut self) {
        self.peer_preview = None;
        self.workspace.binding = None;
        self.workspace.verification = "not_checked".into();
        self.workspace.peer_confirmed = false;
        self.phase = "display".into();
        self.ready = false;
        self.hubs.clear();
        self.hub_count = 0;
        self.local_input = None;
        self.remote_input = None;
        self.code = None;
        if self.display_id.is_empty() {
            self.error = false;
            self.message = "Keep USB on this computer, then start USB detection.".into();
        } else {
            self.fail("Discovery or sleep interrupted setup. Restart USB detection.");
        }
    }
    pub fn config(&self, language: &str) -> Result<Config, String> {
        if !self.ready {
            return Err("Complete USB detection and exchange both computer codes first.".into());
        }
        let config = Config {
            version: 1,
            platform: std::env::consts::OS.into(),
            display_id: self.display_id.clone(),
            local_input: self.local_input.unwrap(),
            remote_input: self.remote_input.unwrap(),
            hubs: self.hubs.clone(),
            automatic: false,
            language: language.into(),
        };
        config.validate()?;
        Ok(config)
    }
}

fn new_draft_id() -> String {
    Workspace::default().workspace_id
}
fn local_role() -> String {
    "local".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    pub version: u32,
    pub workspace_id: String,
    pub revision: u64,
    pub role: String,
    pub local_name: String,
    pub remote_name: String,
    pub local_os: String,
    pub remote_os: String,
    pub local_input: u8,
    pub remote_input: Option<u8>,
}
impl Setup {
    pub fn join(record: &str, name: String) -> Result<Self, String> {
        if record.len() > 4096 {
            return Err("Setup record is too large.".into());
        }
        let peer: Exchange = serde_json::from_str(record).map_err(|_| "Invalid setup record.")?;
        if peer.version != 2
            || !matches!(peer.role.as_str(), "a" | "b")
            || peer.local_input == 0
            || peer.remote_input == Some(0)
            || peer.remote_input == Some(peer.local_input)
        {
            return Err("Invalid or incompatible setup record.".into());
        }
        crate::product::validate_name(&name)?;
        let mut setup = Self::new(String::new(), name.trim().into());
        setup.edit_workspace(serde_json::json!({"workspace_id":peer.workspace_id,"revision":peer.revision,"local_role":if peer.role == "a" { "b" } else { "a" },"local_name":peer.remote_name,"remote_name":peer.local_name,"local_os":peer.remote_os,"remote_os":peer.local_os,"mode":"dual"}))?;
        setup.message = "Workspace record adopted. Select and enroll this computer's monitor and USB switch, then read its input and preview the peer record again.".into();
        Ok(setup)
    }
    pub fn edit_workspace(&mut self, value: serde_json::Value) -> Result<(), String> {
        self.workspace = self.workspace.edit(value)?;
        self.product_configured = true;
        self.peer_preview = None;
        if self.local_input.is_some() {
            self.code = self.export().ok();
        }
        Ok(())
    }
    pub fn export(&self) -> Result<String, String> {
        let input = self
            .local_input
            .ok_or("Read this computer's input first.")?;
        if self.hubs.is_empty() {
            return Err("Complete USB detection first.".into());
        }
        let w = &self.workspace;
        serde_json::to_string(&Exchange {
            version: 2,
            workspace_id: w.workspace_id.clone(),
            revision: w.revision,
            role: w.local_role.clone(),
            local_name: w.local_name.clone(),
            remote_name: w.remote_name.clone(),
            local_os: w.local_os.clone(),
            remote_os: w.remote_os.clone(),
            local_input: input,
            remote_input: self.remote_input,
        })
        .map_err(|e| e.to_string())
    }
    pub fn preview(&mut self, record: &str) -> Result<Exchange, String> {
        self.peer_preview = None;
        if record.len() > 4096 {
            return Err("Setup record is too large.".into());
        }
        let peer: Exchange = serde_json::from_str(record)
            .map_err(|_| "Invalid setup record; copy the complete version 2 record.")?;
        let w = &self.workspace;
        if peer.version != 2 || peer.workspace_id != w.workspace_id || peer.revision != w.revision {
            return Err("Wrong workspace, incompatible or stale setup record. Review workspace identity and revision.".into());
        }
        if !matches!(peer.role.as_str(), "a" | "b") || peer.role == w.local_role {
            return Err("The record claims this computer's role. Choose the other role.".into());
        }
        if peer.local_name != w.remote_name
            || peer.remote_name != w.local_name
            || peer.local_os != w.remote_os
            || peer.remote_os != w.local_os
        {
            return Err(
                "Computer names or OS choices disagree. Review both workspace records.".into(),
            );
        }
        let local = self
            .local_input
            .ok_or("Read this computer's input first.")?;
        if peer.local_input == 0
            || peer.local_input == local
            || peer.remote_input.is_some_and(|i| i != local)
            || self.remote_input.is_some_and(|i| i != peer.local_input)
        {
            return Err("Reciprocal input mappings disagree. Review each computer's picture and replace stale records.".into());
        }
        self.peer_preview = Some(peer.clone());
        Ok(peer)
    }
    pub fn confirm_peer(&mut self) -> Result<(), String> {
        let peer = self
            .peer_preview
            .take()
            .ok_or("Preview the other computer's setup record first.")?;
        self.remote_input = Some(peer.local_input);
        self.error = false;
        self.workspace.peer_confirmed = true;
        self.workspace.binding = None;
        self.workspace.verification = "not_checked".into();
        self.update_ready();
        Ok(())
    }
    pub fn restart_draft(&mut self) {
        self.interrupted();
        self.baseline.clear();
        self.peer_preview = None;
        self.workspace.binding = None;
        self.workspace.verification = "not_checked".into();
    }
    pub fn expected_departure(&self) -> bool {
        matches!(
            self.phase.as_str(),
            "usb_wait_departure" | "usb_wait_return"
        )
    }
    pub fn save_draft(&self, path: &std::path::Path) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("Draft path needs a parent directory")?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create draft directory: {e}"))?;
        let temp = path.with_extension("tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        use std::io::Write;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|e| format!("Cannot save draft: {e}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Cannot save draft: {e}"))?;
        drop(file);
        std::fs::rename(temp, path).map_err(|e| format!("Cannot replace draft: {e}"))
    }
    pub fn load_draft(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Cannot read setup draft: {e}"))?;
        if bytes.len() > 65536 {
            return Err("Setup draft is too large.".into());
        }
        let mut draft: Self =
            serde_json::from_slice(&bytes).map_err(|e| format!("Invalid setup draft: {e}"))?;
        crate::product::validate_name(&draft.name)?;
        draft.workspace.validate()?;
        draft.restart_draft();
        draft.draft_saved = true;
        Ok(draft)
    }
}

pub fn input_code(input: u8) -> String {
    format!("PK1-{input:02X}-{:02X}", input ^ 0xA7)
}
pub fn parse_code(code: &str) -> Result<u8, String> {
    let code = code.trim();
    let invalid =
        || "Invalid setup code. Copy the complete PK1 code from the other computer.".to_string();
    let bytes = code.as_bytes();
    if bytes.len() != 9
        || !code.starts_with("PK1-")
        || bytes[6] != b'-'
        || ![4, 5, 7, 8].iter().all(|i| bytes[*i].is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    let input = u8::from_str_radix(&code[4..6], 16).map_err(|_| invalid())?;
    let checksum = u8::from_str_radix(&code[7..9], 16).map_err(|_| invalid())?;
    if input == 0 || checksum != input ^ 0xA7 {
        return Err(invalid());
    }
    Ok(input)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn hub(id: &str) -> Hub {
        Hub {
            id: id.into(),
            label: "same model".into(),
        }
    }
    #[test]
    fn slow_polling_early_return_is_retryable_and_long_hold_succeeds() {
        let hubs = vec![hub("usb2"), hub("usb3")];
        let mut s = Setup::new("display".into(), "Desk".into());
        s.start_usb(&hubs).unwrap();
        s.observe(&[], 0);
        s.observe(&hubs, 4740);
        assert_eq!(s.phase, "usb_retry");
        assert!(s.error);
        assert!(!s.ready);
        s.capture_usb();
        assert_eq!(s.phase, "usb_capturing");
        s.start_usb(&hubs).unwrap();
        s.observe(&[], 10000);
        s.observe(&[], 14740);
        assert_eq!(s.phase, "usb_wait_return");
        s.observe(&hubs, 19480);
        s.observe(&hubs, 24220);
        assert_eq!(s.phase, "usb_ready");
    }
    #[test]
    fn undetected_switch_and_missing_return_have_bounded_wait() {
        let hubs = vec![hub("usb")];
        let mut s = Setup::new("display".into(), "Desk".into());
        s.start_usb(&hubs).unwrap();
        s.observe(&hubs, 0);
        s.observe(&hubs, 60000);
        assert_eq!(s.phase, "usb_retry");
        s.start_usb(&hubs).unwrap();
        s.observe(&[], 0);
        s.observe(&[], 4740);
        s.observe(&[], 64740);
        assert_eq!(s.phase, "usb_retry");
    }
    #[test]
    fn codes_roundtrip_and_reject_corruption() {
        for input in 1..=255 {
            assert_eq!(parse_code(&input_code(input)), Ok(input));
        }
        for code in [
            "PK1-00-A7",
            "PK2-05-A2",
            "PK1-05-A3",
            "PK1-０5-A2",
            "PK1-05-A2x",
        ] {
            assert!(parse_code(code).is_err());
        }
    }
    #[test]
    fn one_or_two_hubs_require_stable_departure_and_return() {
        for count in 1..=2 {
            let baseline = vec![hub("upstream"), hub("usb2"), hub("usb3")];
            let present = if count == 1 {
                baseline[..2].to_vec()
            } else {
                baseline[..1].to_vec()
            };
            let mut s = Setup::new("display".into(), "Desk".into());
            s.start_usb(&baseline).unwrap();
            s.observe(&present, 0);
            s.observe(&present, 2000);
            assert_eq!(s.phase, "usb_wait_return");
            s.observe(&baseline, 2200);
            assert_ne!(s.phase, "usb_ready");
            s.observe(&baseline, 4200);
            assert_eq!(s.phase, "usb_ready");
            assert_eq!(s.hub_count, count);
        }
    }
    #[test]
    fn staggered_pair_and_ambiguous_changes() {
        let baseline = vec![hub("a"), hub("b"), hub("c")];
        let mut s = Setup::new("d".into(), "n".into());
        s.start_usb(&baseline).unwrap();
        s.observe(&baseline[..2], 0);
        s.observe(&baseline[..1], 600);
        s.observe(&baseline[..1], 2000);
        assert_eq!(s.phase, "usb_wait_departure");
        s.observe(&baseline[..1], 2600);
        assert_eq!(s.phase, "usb_wait_return");
        s.observe(&baseline[..2], 3000);
        s.observe(&baseline, 4000);
        s.observe(&baseline, 6000);
        assert_eq!(s.hub_count, 2);
        assert_eq!(s.phase, "usb_ready");
        s.start_usb(&baseline).unwrap();
        s.observe(&[], 0);
        assert!(s.error);
        assert!(!s.ready);
    }
}
