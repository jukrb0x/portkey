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
#[cfg(target_os = "macos")]
pub use macos::{inventory, set_input};
#[cfg(target_os = "windows")]
pub use windows::{inventory, set_input};
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

// Drain both pipes concurrently so large inventory output cannot block the child.
fn run(command: &mut Command, timeout: Duration) -> Result<String, String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start hardware helper: {e}"))?;
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
                let _ = child.kill();
                let _ = child.wait();
                break Err("Hardware helper timed out; command outcome unknown".to_owned());
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!("Cannot wait for hardware helper: {e}"));
            }
        }
    };
    let status = status?;
    let out = stdout
        .recv_timeout(timeout.saturating_sub(start.elapsed()))
        .map_err(|_| "Hardware helper output timed out")??;
    let err = stderr
        .recv_timeout(timeout.saturating_sub(start.elapsed()))
        .map_err(|_| "Hardware helper output timed out")??;
    if !status.success() {
        return Err(format!(
            "Hardware helper failed ({status}): {}",
            (err + &out).chars().take(800).collect::<String>()
        ));
    }
    Ok(out.trim_start_matches('\u{feff}').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn successful_child_with_inherited_pipe_cannot_hold_worker() {
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
