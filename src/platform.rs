use crate::core::{Config, Display, Hub};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Inventory {
    pub displays: Vec<Display>,
    pub hubs: Vec<Hub>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOutcome {
    Confirmed,
    SentUnverified(String),
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "windows", test))]
mod windows;
#[cfg(any(target_os = "windows", test))]
mod windows_usb;
#[cfg(target_os = "windows")]
pub fn usb_inventory() -> Result<Vec<Hub>, String> {
    windows_usb::hubs()
}
#[cfg(target_os = "macos")]
pub use macos::{inventory, read_input, set_input};
#[cfg(target_os = "windows")]
pub use windows::{ddc_helper, inventory, prewrite_inventory, read_input, set_input};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn inventory() -> Result<Inventory, String> {
    Err("PortKey supports macOS and Windows".into())
}
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn set_input(_: &Config, _: u8) -> Result<WriteOutcome, String> {
    Err("Unsupported platform".into())
}

#[cfg(any(target_os = "macos", test))]
fn require_display(displays: &[Display], id: &str) -> Result<(), String> {
    match displays.iter().filter(|d| d.id == id).count() {
        1 => Ok(()),
        0 => Err("Enrolled display is unavailable; no command sent".into()),
        _ => Err("Ambiguous display identity; no command sent".into()),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn read_input(_: &str) -> Result<u8, String> {
    Err("Unsupported platform".into())
}

fn parse_input(text: &str) -> Result<u8, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("Invalid display input readback".into());
    }
    text.parse::<u8>()
        .ok()
        .filter(|input| *input != 0)
        .ok_or_else(|| "Display input readback must be between 1 and 255".into())
}

// Drain both pipes concurrently so large inventory output cannot block the child.
#[cfg(any(target_os = "windows", test))]
fn run(command: &mut Command, timeout: Duration) -> Result<String, String> {
    run_with_stderr(command, timeout, str::to_owned)
}
#[cfg(any(target_os = "windows", test))]
fn run_with_stderr(
    command: &mut Command,
    timeout: Duration,
    observe_stderr: impl FnMut(&str) -> String,
) -> Result<String, String> {
    run_observed_with_stderr(command, timeout, observe_stderr, |_, _, _| {})
}
#[cfg(any(target_os = "macos", all(test, unix)))]
fn run_observed(
    command: &mut Command,
    timeout: Duration,
    observe_process: impl FnMut(&'static str, Option<i32>, Duration),
) -> Result<String, String> {
    run_observed_with_stderr(command, timeout, str::to_owned, observe_process)
}
fn run_observed_with_stderr(
    command: &mut Command,
    timeout: Duration,
    mut observe_stderr: impl FnMut(&str) -> String,
    mut observe_process: impl FnMut(&'static str, Option<i32>, Duration),
) -> Result<String, String> {
    let command_started = Instant::now();
    let mut outcome = "transport_error";
    let mut exit_code = None;
    let result = (|| {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| {
            outcome = "spawn_error";
            format!("Cannot start hardware helper: {e}")
        })?;
        observe_process("spawned", None, command_started.elapsed());
        let out = child.stdout.take().ok_or("Missing helper stdout")?;
        let err = child.stderr.take().ok_or("Missing helper stderr")?;
        let reader = |stream: Box<dyn Read + Send>| {
            let (send, receive) = std::sync::mpsc::channel();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let result = stream
                    .take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        if bytes.len() > 4 * 1024 * 1024 {
                            Err("Hardware helper output exceeded limit".into())
                        } else {
                            Ok(String::from_utf8_lossy(&bytes).into_owned())
                        }
                    });
                let _ = send.send(result);
            });
            receive
        };
        let stdout = reader(Box::new(out));
        let stderr = reader(Box::new(err));
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    outcome = "timed_out";
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err("Hardware helper timed out; command outcome unknown".to_owned());
                }
                Err(e) => {
                    outcome = "wait_error";
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("Cannot wait for hardware helper: {e}"));
                }
            }
        }?;
        exit_code = status.code();
        let mut receive = |receiver: std::sync::mpsc::Receiver<Result<String, String>>| {
            receiver
                .recv_timeout(timeout.saturating_sub(start.elapsed()))
                .map_err(|_| {
                    outcome = "output_timed_out";
                    "Hardware helper output timed out".to_string()
                })?
                .inspect_err(|_| {
                    outcome = "output_error";
                })
        };
        let out = receive(stdout)?;
        let err = receive(stderr)?;
        let err = observe_stderr(&err);
        if !status.success() {
            outcome = "exit_error";
            return Err(format!(
                "Hardware helper failed ({status}): {}",
                (err + &out).chars().take(800).collect::<String>()
            ));
        }
        outcome = "transport_ok";
        Ok(out.trim_start_matches('\u{feff}').to_owned())
    })();
    observe_process(outcome, exit_code, command_started.elapsed());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    static PROCESS_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    #[test]
    fn input_readback_requires_one_nonzero_byte() {
        for (text, value) in [("5\n", 5), (" 7\r\n", 7), ("255", 255)] {
            assert_eq!(parse_input(text), Ok(value));
        }
        for text in [
            "",
            "0",
            "256",
            "-1",
            "+5",
            "5 7",
            "5\n7",
            "input: 5",
            "confirmed",
        ] {
            assert!(parse_input(text).is_err(), "{text:?}");
        }
    }
    #[test]
    fn selection_requires_one_exact_identity() {
        let d = || Display {
            id: "a".into(),
            label: "Monitor".into(),
        };
        assert!(require_display(&[], "a").is_err());
        assert!(require_display(&[d()], "a").is_ok());
        assert!(require_display(&[d()], "b").is_err());
        assert!(require_display(&[d(), d()], "a").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn observed_helper_reports_exact_spawn_terminal_and_numeric_argument() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let private_id = "test-display-identity-not-for-logs";
        let mut events = Vec::new();
        let output = run_observed(
            Command::new("/bin/sh").args([
                "-c",
                "printf %s \"$5\"",
                "fake-helper",
                "display",
                private_id,
                "set",
                "input",
                "7",
            ]),
            Duration::from_secs(1),
            |event, code, _| events.push((event, code)),
        )
        .unwrap();
        assert_eq!(parse_input(&output), Ok(7));
        assert_eq!(events, vec![("spawned", None), ("transport_ok", Some(0))]);
        assert!(!format!("{events:?}").contains(private_id));
        events.clear();
        assert!(run_observed(
            Command::new("/bin/sh").args(["-c", "printf private-error >&2; exit 9"]),
            Duration::from_secs(1),
            |event, code, _| events.push((event, code))
        )
        .is_err());
        assert_eq!(events, vec![("spawned", None), ("exit_error", Some(9))]);
        assert!(!format!("{events:?}").contains("private-error"));
    }
    #[cfg(unix)]
    #[test]
    fn observed_helper_separates_spawn_failure_timeout_and_output_failure() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let mut events = Vec::new();
        assert!(run_observed(
            &mut Command::new("/portkey-test-missing-helper"),
            Duration::from_secs(1),
            |event, code, _| events.push((event, code))
        )
        .is_err());
        assert_eq!(events, vec![("spawn_error", None)]);
        events.clear();
        assert!(run_observed(
            Command::new("/bin/sleep").arg("2"),
            Duration::from_millis(40),
            |event, code, _| events.push((event, code))
        )
        .is_err());
        assert_eq!(events, vec![("spawned", None), ("timed_out", None)]);
        events.clear();
        assert!(run_observed(
            Command::new("/bin/sh").args(["-c", "sleep 2 & exit 3"]),
            Duration::from_millis(80),
            |event, code, _| events.push((event, code))
        )
        .is_err());
        assert_eq!(
            events,
            vec![("spawned", None), ("output_timed_out", Some(3))]
        );
        events.clear();
        assert!(run_observed(
            Command::new("/usr/bin/head").args(["-c", "4194305", "/dev/zero"]),
            Duration::from_secs(2),
            |event, code, _| events.push((event, code))
        )
        .is_err());
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "spawned");
        assert_eq!(events[1].0, "output_error");
    }
    #[cfg(unix)]
    #[test]
    fn stderr_observation_preserves_stdout_and_failure_status() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let output = run_with_stderr(
            Command::new("/bin/sh").args(["-c", "printf result; printf trace >&2"]),
            Duration::from_secs(1),
            |stderr| {
                assert_eq!(stderr, "trace");
                String::new()
            },
        )
        .unwrap();
        assert_eq!(output, "result");
        let error = run_with_stderr(
            Command::new("/bin/sh").args(["-c", "printf failure >&2; exit 3"]),
            Duration::from_secs(1),
            str::to_owned,
        )
        .unwrap_err();
        assert!(error.contains("failure"));
        assert!(error.contains('3'));
    }
    #[cfg(unix)]
    #[test]
    fn successful_child_with_inherited_pipe_cannot_hold_worker() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let start = Instant::now();
        assert!(run(
            Command::new("/bin/sh").args(["-c", "sleep 2 & exit 0"]),
            Duration::from_millis(80)
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[cfg(unix)]
    #[test]
    fn process_timeout_and_failure_are_errors() {
        let _guard = PROCESS_TEST.lock().unwrap();
        let start = Instant::now();
        assert!(run(
            Command::new("/bin/sleep").arg("2"),
            Duration::from_millis(40)
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(run(
            Command::new("/usr/bin/false").args([] as [&str; 0]),
            Duration::from_secs(1)
        )
        .is_err());
    }
}
