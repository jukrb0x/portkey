#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
use super::*;
const BRIDGE: &str = include_str!("windows.ps1");
fn powershell(mode: &str, target: Option<(&str, Option<u8>)>) -> Result<String, String> {
    let system = std::env::var_os("SystemRoot").ok_or("Windows system directory missing")?;
    let exe =
        std::path::PathBuf::from(system).join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut command = Command::new(exe);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            BRIDGE,
        ])
        .env("PORTKEY_MODE", mode);
    if let Some((display_id, input)) = target {
        command.env("PORTKEY_DISPLAY", display_id);
        if let Some(input) = input {
            command.env("PORTKEY_INPUT", input.to_string());
        }
    }
    run(&mut command, Duration::from_secs(15))
}
pub fn inventory() -> Result<Inventory, String> {
    parse_inventory(&powershell("inventory", None)?)
}
fn parse_inventory(text: &str) -> Result<Inventory, String> {
    #[derive(serde::Deserialize)]
    struct Snapshot {
        displays: Vec<Display>,
        hubs: Vec<Hub>,
    }
    let parsed: Snapshot = serde_json::from_str(text)
        .map_err(|e| format!("Invalid Windows hardware inventory: {e}"))?;
    if parsed.hubs.is_empty() {
        return Err("USB root hubs missing; automation paused".into());
    }
    let mut seen = std::collections::HashSet::new();
    for hub in &parsed.hubs {
        if !hub.id.starts_with("windows:") || hub.id.len() <= 8 || !seen.insert(&hub.id) {
            return Err("Incomplete or duplicate Windows USB topology".into());
        }
    }
    for display in &parsed.displays {
        if !display.id.starts_with(r"\\?\") {
            return Err("Display lacks stable interface identity".into());
        }
    }
    Ok(Inventory {
        displays: parsed.displays,
        hubs: parsed.hubs,
    })
}
pub fn read_input(display_id: &str) -> Result<u8, String> {
    if !display_id.starts_with(r"\\?\") {
        return Err("Display lacks stable interface identity".into());
    }
    parse_input(&powershell("read", Some((display_id, None)))?)
}
pub fn set_input(config: &Config, input: u8) -> Result<WriteOutcome, String> {
    config.validate()?;
    if config.platform != "windows" || ![config.local_input, config.remote_input].contains(&input) {
        return Err("Invalid platform or unenrolled input".into());
    }
    // Child re-enumerates immediately before opening the unique physical monitor.
    let response = powershell("write", Some((&config.display_id, Some(input))))?;
    match response.trim() {
        "confirmed" => Ok(WriteOutcome::Confirmed),
        "sent" => Ok(WriteOutcome::SentUnverified(
            "DDC command sent; readback unavailable or different".into(),
        )),
        _ => Err("Unexpected Windows DDC helper response".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_windows_snapshot_is_never_absence() {
        assert!(parse_inventory("{}").is_err());
        assert!(parse_inventory(r#"{"displays":[],"hubs":null}"#).is_err());
        assert!(
            parse_inventory(r#"{"displays":[],"hubs":[{"id":"VID_1234","label":"hub"}]}"#).is_err()
        );
        assert!(parse_inventory(r#"{"displays":[],"hubs":[]}"#).is_err());
    }
}
