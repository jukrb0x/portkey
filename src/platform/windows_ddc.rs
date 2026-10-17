//! Isolated native DDC transport. Parent process bounds potentially blocking driver calls.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[cfg(target_os = "windows")]
use std::time::Duration;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Operation {
    Read,
    Write(u8),
}
fn request(mode: &str, display: &str, input: Option<&str>) -> Result<Operation, String> {
    if !display.starts_with(r"\\?\") || display.chars().any(char::is_control) {
        return Err("Invalid display identity".into());
    }
    match (mode, input) {
        ("read", None) => Ok(Operation::Read),
        ("write", Some(input)) => super::parse_input(input).map(Operation::Write),
        _ => Err("Invalid native DDC request".into()),
    }
}
trait Monitor {
    fn read(&mut self) -> Result<u8, String>;
    fn write(&mut self, input: u8) -> Result<(), String>;
}
fn timed<T>(stage: &'static str, enabled: bool, action: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = action();
    if enabled {
        eprintln!("PORTKEY_TIMING {stage} {}", start.elapsed().as_micros());
    }
    value
}
fn access(
    monitor: &mut impl Monitor,
    operation: Operation,
    trace: bool,
    wait: impl FnOnce(),
) -> Result<String, String> {
    match operation {
        Operation::Read => timed("read_input", trace, || monitor.read()).map(|v| v.to_string()),
        Operation::Write(input) => {
            timed("set_vcp", trace, || monitor.write(input))?;
            timed("postwrite_wait", trace, wait);
            let readback = timed("postwrite_readback", trace, || monitor.read());
            Ok(if readback == Ok(input) {
                "confirmed"
            } else {
                "sent"
            }
            .into())
        }
    }
}

fn exact_monitor<T>(rows: Vec<(crate::core::Display, T)>, id: &str) -> Result<T, String> {
    let mut matches = rows.into_iter().filter(|(display, _)| display.id == id);
    let (_, handle) = matches.next().ok_or("Enrolled display unavailable")?;
    if matches.next().is_some() {
        return Err("Ambiguous enrolled display".into());
    }
    Ok(handle)
}
fn single_physical(count: u32) -> Result<(), String> {
    if count == 1 {
        Ok(())
    } else {
        Err("Display does not map to exactly one physical monitor".into())
    }
}
fn execute<M: Monitor>(
    open: impl FnOnce() -> Result<M, String>,
    operation: Operation,
    trace: bool,
    wait: impl FnOnce(),
) -> Result<String, String> {
    let mut monitor = timed("exact_display_open", trace, open)?;
    access(&mut monitor, operation, trace, wait)
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use std::ffi::c_void;
    #[repr(C)]
    struct Physical {
        handle: *mut c_void,
        description: [u16; 128],
    }
    #[link(name = "dxva2")]
    unsafe extern "system" {
        fn GetNumberOfPhysicalMonitorsFromHMONITOR(monitor: *mut c_void, count: *mut u32) -> i32;
        fn GetPhysicalMonitorsFromHMONITOR(
            monitor: *mut c_void,
            count: u32,
            physical: *mut Physical,
        ) -> i32;
        fn DestroyPhysicalMonitors(count: u32, physical: *mut Physical) -> i32;
        fn SetVCPFeature(monitor: *mut c_void, code: u8, value: u32) -> i32;
        fn GetVCPFeatureAndVCPFeatureReply(
            monitor: *mut c_void,
            code: u8,
            kind: *mut u32,
            current: *mut u32,
            maximum: *mut u32,
        ) -> i32;
    }
    struct OpenMonitor(Physical);
    impl Drop for OpenMonitor {
        fn drop(&mut self) {
            unsafe {
                DestroyPhysicalMonitors(1, &mut self.0);
            }
        }
    }
    impl Monitor for OpenMonitor {
        fn read(&mut self) -> Result<u8, String> {
            let (mut kind, mut current, mut maximum) = (0, 0, 0);
            let ok = unsafe {
                GetVCPFeatureAndVCPFeatureReply(
                    self.0.handle,
                    0x60,
                    &mut kind,
                    &mut current,
                    &mut maximum,
                )
            };
            if ok == 0 || current == 0 || current > 255 {
                return Err("DDC input readback unavailable or invalid".into());
            }
            Ok(current as u8)
        }
        fn write(&mut self, input: u8) -> Result<(), String> {
            if unsafe { SetVCPFeature(self.0.handle, 0x60, u32::from(input)) } == 0 {
                Err("DDC write failed; monitor outcome unknown".into())
            } else {
                Ok(())
            }
        }
    }
    fn open(id: &str, trace: bool) -> Result<OpenMonitor, String> {
        let rows = timed(
            "native_display_enumerate",
            trace,
            super::super::windows_displays::monitors,
        )?;
        let handle = exact_monitor(rows, id)?;
        let mut count = 0;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(handle, &mut count) } == 0 || count != 1
        {
            return Err("Display does not map to exactly one physical monitor".into());
        }
        single_physical(count)?;
        let mut physical: Physical = unsafe { std::mem::zeroed() };
        if unsafe { GetPhysicalMonitorsFromHMONITOR(handle, 1, &mut physical) } == 0 {
            return Err("Cannot open enrolled physical monitor".into());
        }
        Ok(OpenMonitor(physical))
    }
    pub fn run() -> Result<String, String> {
        let mode = std::env::var("PORTKEY_MODE").map_err(|_| "Missing helper mode")?;
        let id = std::env::var("PORTKEY_DISPLAY").map_err(|_| "Missing helper display")?;
        let input = std::env::var("PORTKEY_INPUT").ok();
        let operation = request(&mode, &id, input.as_deref())?;
        let trace = std::env::var("PORTKEY_TIMING").as_deref() == Ok("1");
        timed("native_helper_total", trace, || {
            execute(
                || open(&id, trace),
                operation,
                trace,
                || std::thread::sleep(Duration::from_millis(1500)),
            )
        })
    }
}
#[cfg(target_os = "windows")]
pub use native::run;

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        read: Result<u8, String>,
        writes: Vec<u8>,
        fail: bool,
    }
    impl Monitor for Fake {
        fn read(&mut self) -> Result<u8, String> {
            self.read.clone()
        }
        fn write(&mut self, input: u8) -> Result<(), String> {
            self.writes.push(input);
            if self.fail {
                Err("failed".into())
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn exact_target_and_single_physical_are_required() {
        let row = |id: &str| {
            (
                crate::core::Display {
                    id: id.into(),
                    label: "test".into(),
                },
                42,
            )
        };
        assert_eq!(
            exact_monitor(vec![row("other"), row("target")], "target").unwrap(),
            42
        );
        assert!(exact_monitor(vec![row("other")], "target").is_err());
        assert!(exact_monitor(vec![row("target"), row("target")], "target").is_err());
        for count in [0, 2, u32::MAX] {
            assert!(single_physical(count).is_err());
        }
        assert!(single_physical(1).is_ok());
    }
    #[test]
    fn opened_monitor_is_released_on_every_transport_result() {
        use std::{cell::Cell, rc::Rc};
        struct Tracked(Rc<Cell<u32>>, bool);
        impl Drop for Tracked {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        impl Monitor for Tracked {
            fn read(&mut self) -> Result<u8, String> {
                Err("unavailable".into())
            }
            fn write(&mut self, _: u8) -> Result<(), String> {
                if self.1 {
                    Err("failed".into())
                } else {
                    Ok(())
                }
            }
        }
        for (operation, fail) in [
            (Operation::Read, false),
            (Operation::Write(7), false),
            (Operation::Write(7), true),
        ] {
            let drops = Rc::new(Cell::new(0));
            let _ = execute(|| Ok(Tracked(drops.clone(), fail)), operation, false, || {});
            assert_eq!(drops.get(), 1);
        }
        assert!(execute::<Tracked>(
            || Err("open failed".into()),
            Operation::Read,
            false,
            || panic!("must not wait")
        )
        .is_err());
    }

    #[test]
    fn native_protocol_is_strict() {
        let id = r"\\?\display#test";
        for (mode, input) in [
            ("read", Some("7")),
            ("write", None),
            ("write", Some("0")),
            ("write", Some("256")),
            ("other", None),
        ] {
            assert!(request(mode, id, input).is_err());
        }
        assert!(request("read", "generic", None).is_err());
        assert!(request("write", id, Some("7")).is_ok());
    }
    #[test]
    fn native_outcomes_preserve_transport_and_readback_semantics() {
        for (read, expected) in [
            (Ok(7), "confirmed"),
            (Ok(5), "sent"),
            (Err("unavailable".into()), "sent"),
        ] {
            let mut monitor = Fake {
                read,
                writes: vec![],
                fail: false,
            };
            assert_eq!(
                access(&mut monitor, Operation::Write(7), false, || {}).unwrap(),
                expected
            );
            assert_eq!(monitor.writes, vec![7]);
        }
        let mut monitor = Fake {
            read: Ok(7),
            writes: vec![],
            fail: true,
        };
        assert!(access(&mut monitor, Operation::Write(7), false, || panic!(
            "failed write must not wait"
        ))
        .is_err());
        assert_eq!(
            access(&mut monitor, Operation::Read, false, || panic!(
                "read must not wait"
            ))
            .unwrap(),
            "7"
        );
    }
}
