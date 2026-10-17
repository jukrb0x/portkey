//! Fresh display identities for pre-command validation; friendly WMI names stay
//! in the UI inventory. No physical monitor is opened and no DDC is performed.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
use crate::core::Display;

fn utf16_field(value: &[u16]) -> Result<String, String> {
    let end = value
        .iter()
        .position(|v| *v == 0)
        .ok_or("Unterminated display identity field")?;
    String::from_utf16(&value[..end]).map_err(|_| "Malformed display identity field".into())
}

fn select_display(devices: Vec<(u32, String, String)>) -> Result<Display, String> {
    let mut active = devices
        .into_iter()
        .filter(|(flags, id, _)| flags & 1 != 0 && !id.is_empty());
    let (_, id, label) = active.next().ok_or("Monitor interface unavailable")?;
    if active.next().is_some() || !id.starts_with(r"\\?\") {
        return Err("Ambiguous or invalid monitor interface".into());
    }
    Ok(Display {
        id: id.to_lowercase(),
        label,
    })
}
fn add_display(displays: &mut Vec<Display>, display: Display) -> Result<(), String> {
    if displays.iter().any(|d| d.id == display.id) {
        return Err("Duplicate monitor interface".into());
    }
    displays.push(display);
    Ok(())
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use std::{ffi::c_void, mem::size_of, ptr};
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Info {
        cb: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
        device: [u16; 32],
    }
    #[repr(C)]
    struct Device {
        cb: u32,
        name: [u16; 32],
        description: [u16; 128],
        flags: u32,
        id: [u16; 128],
        key: [u16; 128],
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(
            dc: *mut c_void,
            clip: *const Rect,
            callback: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut Rect, isize) -> i32,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(monitor: *mut c_void, info: *mut Info) -> i32;
        fn EnumDisplayDevicesW(
            device: *const u16,
            index: u32,
            output: *mut Device,
            flags: u32,
        ) -> i32;
    }
    struct State {
        displays: Vec<Display>,
        handles: Vec<*mut c_void>,
        error: Option<String>,
    }
    unsafe fn monitor_display(monitor: *mut c_void) -> Result<Display, String> {
        let mut info: Info = unsafe { std::mem::zeroed() };
        info.cb = size_of::<Info>() as u32;
        if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
            return Err("Monitor identity read failed".into());
        }
        if utf16_field(&info.device)?.is_empty() {
            return Err("Monitor device name missing".into());
        }
        let mut devices = Vec::new();
        for index in 0..64 {
            let mut device: Device = unsafe { std::mem::zeroed() };
            device.cb = size_of::<Device>() as u32;
            // EDD_GET_DEVICE_INTERFACE_NAME preserves the enrolled full interface ID.
            if unsafe { EnumDisplayDevicesW(info.device.as_ptr(), index, &mut device, 1) } == 0 {
                return select_display(devices);
            }
            devices.push((
                device.flags,
                utf16_field(&device.id)?,
                utf16_field(&device.description)?,
            ));
        }
        Err("Monitor interface enumeration exceeded bound".into())
    }
    unsafe extern "system" fn callback(
        monitor: *mut c_void,
        _: *mut c_void,
        _: *mut Rect,
        data: isize,
    ) -> i32 {
        let state = unsafe { &mut *(data as *mut State) };
        // Never unwind across the Windows callback boundary.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if state.displays.len() >= 64 {
                return Err("Monitor enumeration exceeded bound".into());
            }
            add_display(&mut state.displays, unsafe { monitor_display(monitor) }?)?;
            state.handles.push(monitor);
            Ok(())
        }));
        match result {
            Ok(Ok(())) => 1,
            Ok(Err(error)) => {
                state.error = Some(error);
                0
            }
            Err(_) => {
                state.error = Some("Monitor enumeration failed".into());
                0
            }
        }
    }
    pub fn monitors() -> Result<Vec<(Display, *mut c_void)>, String> {
        let mut state = State {
            displays: Vec::new(),
            handles: Vec::new(),
            error: None,
        };
        let ok = unsafe {
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                callback,
                &mut state as *mut State as isize,
            )
        };
        if let Some(error) = state.error {
            return Err(error);
        }
        if ok == 0 {
            return Err("Monitor enumeration failed".into());
        }
        Ok(state.displays.into_iter().zip(state.handles).collect())
    }
    pub fn displays() -> Result<Vec<Display>, String> {
        monitors().map(|rows| rows.into_iter().map(|(display, _)| display).collect())
    }
}
#[cfg(target_os = "windows")]
pub use native::{displays, monitors};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_interface_and_active_selection_preserve_enrollment() {
        let a = r"\\?\DISPLAY#ABC#instance-a#{guid}";
        let b = r"\\?\DISPLAY#ABC#instance-b#{guid}";
        let selected = select_display(vec![
            (0, b.into(), "inactive".into()),
            (1, a.into(), "monitor".into()),
        ])
        .unwrap();
        assert_eq!(selected.id, a.to_lowercase());
        assert_ne!(selected.id, b.to_lowercase());
        assert_eq!(selected.label, "monitor");
    }
    #[test]
    fn missing_ambiguous_and_duplicate_interfaces_fail_closed() {
        let a = r"\\?\DISPLAY#ABC#instance-a#{guid}";
        assert!(select_display(vec![]).is_err());
        assert!(select_display(vec![(1, String::new(), String::new())]).is_err());
        assert!(select_display(vec![(1, "generic-name".into(), String::new())]).is_err());
        assert!(select_display(vec![
            (1, a.into(), String::new()),
            (1, a.into(), String::new())
        ])
        .is_err());
        let mut displays = Vec::new();
        add_display(
            &mut displays,
            select_display(vec![(1, a.into(), String::new())]).unwrap(),
        )
        .unwrap();
        assert!(add_display(
            &mut displays,
            select_display(vec![(1, a.to_lowercase(), String::new())]).unwrap()
        )
        .is_err());
    }
    #[test]
    fn native_fields_require_valid_terminated_utf16() {
        assert_eq!(utf16_field(&[65, 0, 66]).unwrap(), "A");
        assert!(utf16_field(&[65; 128]).is_err());
        assert!(utf16_field(&[0xd800, 0]).is_err());
    }
}
