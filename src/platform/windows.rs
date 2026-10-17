#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
use super::*;
#[path = "windows_ddc.rs"]
mod windows_ddc;
#[path = "windows_displays.rs"]
mod windows_displays;
const BRIDGE: &str = include_str!("windows.ps1");
fn powershell(mode: &'static str, target: Option<(&str, Option<u8>)>) -> Result<String, String> {
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
    let enabled = crate::timing::enabled();
    // Explicitly clear inherited child tracing when this process has no active logger.
    command.env("PORTKEY_TIMING", if enabled { "1" } else { "0" });
    if !enabled {
        return run(&mut command, Duration::from_secs(15));
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let invocation = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let start = Instant::now();
    crate::timing::event("helper_start", mode, None, Some(invocation));
    let result = run_with_stderr(&mut command, Duration::from_secs(15), |stderr| {
        collect_timing(stderr, |stage, micros| {
            crate::timing::event(
                stage,
                mode,
                Some(Duration::from_micros(micros)),
                Some(invocation),
            );
        })
    });
    crate::timing::event(
        if result.is_ok() {
            "helper_end"
        } else {
            "helper_failed"
        },
        mode,
        Some(start.elapsed()),
        Some(invocation),
    );
    result
}
#[cfg(target_os = "windows")]
pub fn ddc_helper() -> Result<String, String> {
    windows_ddc::run()
}
// A DLL-hosted controller must never relaunch the WinUI executable as a helper.
fn native_helper_for_module(module: &std::path::Path) -> std::path::PathBuf {
    if module
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
    {
        module.with_file_name("PortKeyHelper.exe")
    } else {
        module.to_owned()
    }
}
#[cfg(target_os = "windows")]
fn native_helper_path() -> Result<std::path::PathBuf, String> {
    use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleExW};
    unsafe {
        let mut module = std::ptr::null_mut();
        // FROM_ADDRESS | UNCHANGED_REFCOUNT locates this Rust module, not its UI host.
        if GetModuleHandleExW(
            6,
            native_helper_path as *const () as *const u16,
            &mut module,
        ) == 0
        {
            return Err("Cannot locate the native controller module".into());
        }
        let mut name = vec![0u16; 32768];
        let length = GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) as usize;
        if length == 0 || length >= name.len() {
            return Err("Cannot locate the native controller module path".into());
        }
        use std::os::windows::ffi::OsStringExt;
        Ok(native_helper_for_module(std::path::Path::new(
            &std::ffi::OsString::from_wide(&name[..length]),
        )))
    }
}
#[cfg(not(target_os = "windows"))]
fn native_helper_path() -> Result<std::path::PathBuf, String> {
    std::env::current_exe()
        .map(|p| native_helper_for_module(&p))
        .map_err(|_| "Cannot locate native helper".into())
}
fn native_ddc(mode: &'static str, display_id: &str, input: Option<u8>) -> Result<String, String> {
    let mut command = Command::new(native_helper_path()?);
    command
        .arg("--ddc-helper")
        .env("PORTKEY_MODE", mode)
        .env("PORTKEY_DISPLAY", display_id)
        .env_remove("PORTKEY_INPUT")
        .env_remove("PORTKEY_TIMING_LOG");
    if let Some(input) = input {
        command.env("PORTKEY_INPUT", input.to_string());
    }
    let enabled = crate::timing::enabled();
    // Explicitly clear inherited child tracing when this process has no active logger.
    command.env("PORTKEY_TIMING", if enabled { "1" } else { "0" });
    if !enabled {
        return run(&mut command, Duration::from_secs(15));
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let invocation = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let start = Instant::now();
    crate::timing::event("helper_start", mode, None, Some(invocation));
    let result = run_with_stderr(&mut command, Duration::from_secs(15), |stderr| {
        collect_timing(stderr, |stage, micros| {
            crate::timing::event(
                stage,
                mode,
                Some(Duration::from_micros(micros)),
                Some(invocation),
            );
        })
    });
    crate::timing::event(
        if result.is_ok() {
            "helper_end"
        } else {
            "helper_failed"
        },
        mode,
        Some(start.elapsed()),
        Some(invocation),
    );
    result
}
// Only fixed labels and bounded numeric durations may cross into the timing log.
fn parse_timing(line: &str) -> Option<(&'static str, u64)> {
    let mut parts = line.split_ascii_whitespace();
    if parts.next()? != "PORTKEY_TIMING" {
        return None;
    }
    let stage = match parts.next()? {
        "add_type" => "add_type",
        "script_total" => "script_total",
        "native_helper_total" => "native_helper_total",
        "usb_enumerate" => "usb_enumerate",
        "names_display_enumerate" => "names_display_enumerate",
        "native_display_enumerate" => "native_display_enumerate",
        "exact_display_open" => "exact_display_open",
        "set_vcp" => "set_vcp",
        "postwrite_wait" => "postwrite_wait",
        "postwrite_readback" => "postwrite_readback",
        "read_input" => "read_input",
        _ => return None,
    };
    let number = parts.next()?;
    if !number.bytes().all(|b| b.is_ascii_digit()) || parts.next().is_some() {
        return None;
    }
    let micros = number.parse::<u64>().ok()?;
    (micros <= 60_000_000).then_some((stage, micros))
}
fn collect_timing(stderr: &str, mut record: impl FnMut(&'static str, u64)) -> String {
    let mut errors = String::new();
    for line in stderr.split_inclusive('\n') {
        if let Some((stage, micros)) = parse_timing(line) {
            record(stage, micros);
        } else {
            errors.push_str(line);
        }
    }
    errors
}
#[cfg(target_os = "windows")]
pub fn inventory() -> Result<Inventory, String> {
    // Display discovery stays out of the independent native USB observation loop.
    let displays = parse_displays(&powershell("displays", None)?)?;
    let hubs = super::windows_usb::hubs()?;
    Ok(Inventory { displays, hubs })
}
#[cfg(target_os = "windows")]
pub fn prewrite_inventory() -> Result<Inventory, String> {
    Ok(Inventory {
        displays: windows_displays::displays()?,
        hubs: super::windows_usb::hubs()?,
    })
}
fn parse_displays(text: &str) -> Result<Vec<Display>, String> {
    #[derive(serde::Deserialize)]
    struct Snapshot {
        displays: Vec<Display>,
    }
    let parsed: Snapshot = serde_json::from_str(text)
        .map_err(|e| format!("Invalid Windows display inventory: {e}"))?;
    let mut seen = std::collections::HashSet::new();
    for display in &parsed.displays {
        if !display.id.starts_with(r"\\?\") || !seen.insert(&display.id) {
            return Err("Display lacks a unique stable interface identity".into());
        }
    }
    Ok(parsed.displays)
}
pub fn read_input(display_id: &str) -> Result<u8, String> {
    if !display_id.starts_with(r"\\?\") {
        return Err("Display lacks stable interface identity".into());
    }
    parse_input(&native_ddc("read", display_id, None)?)
}
pub fn set_input(config: &Config, input: u8) -> Result<WriteOutcome, String> {
    config.validate()?;
    if config.platform != "windows" || ![config.local_input, config.remote_input].contains(&input) {
        return Err("Invalid platform or unenrolled input".into());
    }
    // Child re-enumerates immediately before opening the unique physical monitor.
    let response = native_ddc("write", &config.display_id, Some(input))?;
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
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "Read-only display parity probe; requires explicit output path"]
    fn native_display_readonly_parity() {
        assert_eq!(
            std::env::var("PORTKEY_TEST_DISPLAY_READONLY").as_deref(),
            Ok("1")
        );
        let path = std::path::PathBuf::from(
            std::env::var_os("PORTKEY_TEST_DISPLAY_OUTPUT").expect("private output path"),
        );
        assert!(path.is_absolute());
        let start = Instant::now();
        let native = windows_displays::displays();
        let native_us = start.elapsed().as_micros();
        let start = Instant::now();
        let bridge = powershell("displays", None).and_then(|s| parse_displays(&s));
        let bridge_us = start.elapsed().as_micros();
        let ids = |result: &Result<Vec<Display>, String>| {
            result.as_ref().ok().map(|rows| {
                let mut ids: Vec<_> = rows.iter().map(|d| d.id.clone()).collect();
                ids.sort();
                ids
            })
        };
        let native_ids = ids(&native);
        let bridge_ids = ids(&bridge);
        let report = serde_json::json!({"native_us": native_us, "bridge_us": bridge_us,
            "native_ids": native_ids, "bridge_ids": bridge_ids,
            "native_error": native.as_ref().err(), "bridge_error": bridge.as_ref().err()});
        std::fs::write(path, report.to_string()).unwrap();
        match (native_ids, bridge_ids) {
            (Some(a), Some(b)) => assert_eq!(a, b),
            _ => panic!("Desktop display inventory unavailable; parity not established (see private report)"),
        }
    }
    #[test]
    fn dll_transport_uses_dedicated_helper_and_legacy_exe_remains_supported() {
        use std::path::Path;
        assert_eq!(
            native_helper_for_module(Path::new("bundle/portkey.dll")),
            Path::new("bundle/PortKeyHelper.exe")
        );
        assert_eq!(
            native_helper_for_module(Path::new("bundle/PORTKEY.DLL")),
            Path::new("bundle/PortKeyHelper.exe")
        );
        assert_eq!(
            native_helper_for_module(Path::new("bundle/PortKey-NativeDDC.exe")),
            Path::new("bundle/PortKey-NativeDDC.exe")
        );
    }
    #[test]
    fn timing_markers_are_bounded_allowlisted_and_identity_free() {
        assert_eq!(
            parse_timing("PORTKEY_TIMING set_vcp 125"),
            Some(("set_vcp", 125))
        );
        assert_eq!(
            parse_timing("PORTKEY_TIMING add_type 0"),
            Some(("add_type", 0))
        );
        for line in [
            "PORTKEY_TIMING id 123",
            "PORTKEY_TIMING set_vcp -1",
            "PORTKEY_TIMING set_vcp 60000001",
            "PORTKEY_TIMING set_vcp 5 extra",
            "error 123",
            "PORTKEY_TIMING set_vcp +5",
        ] {
            assert!(parse_timing(line).is_none(), "{line}");
        }
    }
    #[test]
    fn timing_does_not_discard_helper_errors_or_malformed_markers() {
        let mut records = Vec::new();
        let errors = collect_timing(
            "failure\r\nPORTKEY_TIMING set_vcp 25\r\nPORTKEY_TIMING private-id 1\n",
            |s, n| records.push((s, n)),
        );
        assert_eq!(records, vec![("set_vcp", 25)]);
        assert_eq!(errors, "failure\r\nPORTKEY_TIMING private-id 1\n");
    }
    #[test]
    fn display_only_snapshot_requires_stable_unique_interfaces() {
        assert!(parse_displays("{}").is_err());
        assert!(parse_displays(r#"{"displays":null}"#).is_err());
        assert!(parse_displays(r#"{"displays":[{"id":"model","label":"monitor"}]}"#).is_err());
        assert!(parse_displays(r#"{"displays":[]}"#).unwrap().is_empty());
        let display = Display {
            id: r"\\?\display#unique".into(),
            label: "Monitor".into(),
        };
        let text = serde_json::json!({"displays":[display.clone(), display]}).to_string();
        assert!(parse_displays(&text).is_err());
    }
}
