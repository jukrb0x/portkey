//! Guided enrollment records topology changes and measured inputs, never guesses them.
use crate::core::{Config, Hub};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Setup {
    pub phase: &'static str,
    pub message: String,
    pub error: bool,
    pub display_id: String,
    pub name: String,
    pub local_input: Option<u8>,
    pub remote_input: Option<u8>,
    pub code: Option<String>,
    pub hub_count: usize,
    pub ready: bool,
    #[serde(skip)]
    baseline: Vec<Hub>,
    #[serde(skip)]
    hubs: Vec<Hub>,
    #[serde(skip)]
    stable_since: Option<u64>,
}
impl Setup {
    pub fn new(display_id: String, name: String) -> Self {
        Self {
            phase: "display",
            message: "Keep USB on this computer, then start USB detection.".into(),
            error: false,
            display_id,
            name,
            local_input: None,
            remote_input: None,
            code: None,
            hub_count: 0,
            ready: false,
            baseline: vec![],
            hubs: vec![],
            stable_since: None,
        }
    }
    pub fn start_usb(&mut self, hubs: &[Hub]) -> Result<(), String> {
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
        self.ready = false;
        self.phase = "usb_wait_departure";
        self.message =
            "Press the physical USB switch to the other computer, wait, then switch back.".into();
        self.error = false;
        Ok(())
    }
    pub fn observe(&mut self, observed: &[Hub], now: u64) {
        if !matches!(self.phase, "usb_wait_departure" | "usb_wait_return") {
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
            self.phase = "display";
            return;
        }
        if changed {
            self.stable_since = None;
            self.phase = "usb_wait_departure";
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
        let stable = match self.phase {
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
            self.phase = "usb_wait_return";
            self.message =
                "USB departure detected. Switch USB back to this computer and wait.".into();
        } else {
            self.phase = "usb_ready";
            self.message = "USB switch detected. Show this computer on the selected monitor, then read its input.".into();
        }
    }
    pub fn input_result(&mut self, result: Result<u8, String>) {
        match result {
            Ok(input) if input != 0 => {
                self.local_input = Some(input);
                self.code = Some(input_code(input));
                self.phase = "exchange";
                self.message =
                    "Copy this code to the other computer. Enter the code shown there to finish."
                        .into();
                self.error = false;
                self.update_ready();
            }
            Ok(_) => {
                self.phase = "usb_ready";
                self.fail("The monitor returned an invalid input. Show this computer on the monitor and retry.");
            }
            Err(error) => {
                self.phase = "usb_ready";
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
            self.phase = "ready";
            self.message =
                "Ready to save. Automatic switching will stay paused until you enable it.".into();
        }
    }
    pub fn fail(&mut self, message: &str) {
        self.error = true;
        self.message = message.into();
    }
    pub fn interrupted(&mut self) {
        self.phase = "display";
        self.ready = false;
        self.hubs.clear();
        self.hub_count = 0;
        self.local_input = None;
        self.remote_input = None;
        self.code = None;
        self.fail("Discovery or sleep interrupted setup. Restart USB detection.");
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
