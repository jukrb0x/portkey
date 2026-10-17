//! Read-only SetupAPI USB inventory. Identity remains the complete Windows location path.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]
use crate::core::Hub;
use std::{cmp::Ordering, collections::HashSet};

const MAX_PROPERTY: usize = 64 * 1024;
struct Reply {
    kind: u32,
    size: usize,
    error: Option<u32>,
}
fn read_property(
    mut query: impl FnMut(&mut [u8]) -> Reply,
    optional: bool,
) -> Result<Option<(u32, Vec<u8>)>, String> {
    let size = query(&mut []);
    if size.error == Some(1168) && optional {
        return Ok(None);
    }
    if size.error.is_some_and(|error| error != 122) || size.size > MAX_PROPERTY {
        return Err("USB property size query failed or exceeded limit".into());
    }
    let mut bytes = vec![0u8; size.size];
    let result = query(&mut bytes);
    if result.error.is_some() || result.size != bytes.len() || result.kind != size.kind {
        return Err("USB property changed or could not be read".into());
    }
    bytes.truncate(result.size);
    Ok(Some((result.kind, bytes)))
}
fn wide(bytes: &[u8]) -> Result<Vec<u16>, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("Malformed USB UTF-16 property".into());
    }
    Ok(bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect())
}
fn string_property(kind: u32, bytes: &[u8]) -> Result<String, String> {
    if kind == 0 && bytes.is_empty() {
        return Ok(String::new());
    }
    let units = wide(bytes)?;
    if kind != 18 || units.last() != Some(&0) || units[..units.len() - 1].contains(&0) {
        return Err("Malformed USB string property".into());
    }
    String::from_utf16(&units[..units.len() - 1]).map_err(|_| "Invalid USB UTF-16 string".into())
}
fn paths_property(kind: u32, bytes: &[u8]) -> Result<Vec<String>, String> {
    let units = wide(bytes)?;
    if kind != 0x2012 || units.len() < 3 || !units.ends_with(&[0, 0]) {
        return Err("Malformed USB location path list".into());
    }
    units[..units.len() - 2]
        .split(|unit| *unit == 0)
        .map(|path| {
            if path.is_empty() {
                return Err("Empty USB location path".into());
            }
            String::from_utf16(path).map_err(|_| "Invalid USB location UTF-16".into())
        })
        .collect()
}
fn is_hub(service: &str) -> bool {
    service.eq_ignore_ascii_case("USBHUB") || service.eq_ignore_ascii_case("USBHUB3")
}
struct Device {
    instance: String,
    service: String,
    status: u32,
    problem: u32,
    paths: Vec<String>,
    label: String,
}
fn finish(
    devices: Vec<Device>,
    mut compare: impl FnMut(&str, &str) -> Result<Ordering, String>,
) -> Result<Vec<Hub>, String> {
    let mut instances = HashSet::new();
    let mut identities = HashSet::new();
    let mut hubs = Vec::new();
    for device in devices {
        if device.instance.is_empty() || !instances.insert(device.instance.to_uppercase()) {
            return Err("Missing or duplicate USB device identity".into());
        }
        if !is_hub(&device.service) {
            continue;
        }
        if device.status & 8 == 0 || device.status & (0x400 | 0x8000) != 0 || device.problem != 0 {
            return Err("USB hub status is uncertain; automation paused".into());
        }
        let mut selected: Option<&str> = None;
        for path in &device.paths {
            // Windows location paths are nonlocalized; unsupported text must not acquire a different normalized identity.
            if path.is_empty()
                || !path.is_ascii()
                || path
                    .bytes()
                    .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
            {
                return Err("USB hub lacks a valid location path".into());
            }
            if let Some(previous) = selected {
                if compare(path, previous)? == Ordering::Less {
                    selected = Some(path);
                }
            } else {
                selected = Some(path);
            }
        }
        let path = selected.ok_or("USB hub location paths missing")?;
        let id = format!("windows:{}", path.to_ascii_lowercase());
        if !identities.insert(id.clone()) {
            return Err("Duplicate USB topology identity".into());
        }
        hubs.push(Hub {
            id,
            label: format!("{} @ {path}", device.label),
        });
    }
    if hubs.is_empty() {
        return Err("USB root hubs missing; automation paused".into());
    }
    hubs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(hubs)
}

#[cfg(not(target_os = "windows"))]
pub fn hubs() -> Result<Vec<Hub>, String> {
    Err("Native Windows USB inventory is unavailable on this platform".into())
}
#[cfg(target_os = "windows")]
pub use native::hubs;

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use std::{mem::size_of, ptr};
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        a: u32,
        b: u16,
        c: u16,
        d: [u8; 8],
    }
    #[repr(C)]
    struct Key {
        guid: Guid,
        id: u32,
    }
    #[repr(C)]
    struct Info {
        size: u32,
        class: Guid,
        instance: u32,
        reserved: usize,
    }
    const USB_CLASS: Guid = Guid {
        a: 0x36fc9e60,
        b: 0xc465,
        c: 0x11cf,
        d: [0x80, 0x56, 0x44, 0x45, 0x53, 0x54, 0, 0],
    };
    const PROPERTY_GUID: Guid = Guid {
        a: 0xa45c254e,
        b: 0xdf1c,
        c: 0x4efd,
        d: [0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0],
    };
    #[link(name = "setupapi")]
    extern "system" {
        fn SetupDiGetClassDevsW(
            class: *const Guid,
            enumerator: *const u16,
            parent: isize,
            flags: u32,
        ) -> isize;
        fn SetupDiEnumDeviceInfo(set: isize, index: u32, info: *mut Info) -> i32;
        fn SetupDiGetDeviceInstanceIdW(
            set: isize,
            info: *mut Info,
            buffer: *mut u16,
            capacity: u32,
            required: *mut u32,
        ) -> i32;
        fn SetupDiGetDevicePropertyW(
            set: isize,
            info: *mut Info,
            key: *const Key,
            kind: *mut u32,
            buffer: *mut u8,
            capacity: u32,
            required: *mut u32,
            flags: u32,
        ) -> i32;
        fn SetupDiDestroyDeviceInfoList(set: isize) -> i32;
    }
    #[link(name = "cfgmgr32")]
    extern "system" {
        fn CM_Get_DevNode_Status(
            status: *mut u32,
            problem: *mut u32,
            instance: u32,
            flags: u32,
        ) -> u32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
        fn CompareStringEx(
            locale: *const u16,
            flags: u32,
            a: *const u16,
            a_len: i32,
            b: *const u16,
            b_len: i32,
            version: *const std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
            param: isize,
        ) -> i32;
    }
    struct Set(isize);
    impl Drop for Set {
        fn drop(&mut self) {
            unsafe {
                SetupDiDestroyDeviceInfoList(self.0);
            }
        }
    }
    fn property(
        set: &Set,
        info: &mut Info,
        id: u32,
        optional: bool,
    ) -> Result<Option<(u32, Vec<u8>)>, String> {
        let key = Key {
            guid: PROPERTY_GUID,
            id,
        };
        read_property(
            |bytes| unsafe {
                let mut kind = 0;
                let mut required = 0;
                let ok = SetupDiGetDevicePropertyW(
                    set.0,
                    info,
                    &key,
                    &mut kind,
                    if bytes.is_empty() {
                        ptr::null_mut()
                    } else {
                        bytes.as_mut_ptr()
                    },
                    bytes.len() as u32,
                    &mut required,
                    0,
                );
                let error = if ok == 0 { Some(GetLastError()) } else { None };
                Reply {
                    kind,
                    size: required as usize,
                    error,
                }
            },
            optional,
        )
    }
    fn text(set: &Set, info: &mut Info, id: u32, optional: bool) -> Result<Option<String>, String> {
        property(set, info, id, optional)?
            .map(|(kind, bytes)| string_property(kind, &bytes))
            .transpose()
    }
    fn instance(set: &Set, info: &mut Info) -> Result<String, String> {
        let mut buffer = vec![0u16; 4096];
        let mut required = 0;
        if unsafe {
            SetupDiGetDeviceInstanceIdW(
                set.0,
                info,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut required,
            )
        } == 0
            || required == 0
            || required as usize > buffer.len()
        {
            return Err("USB device instance identity unavailable".into());
        }
        string_property(
            18,
            &buffer[..required as usize]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
    }
    fn status(info: &Info) -> Result<(u32, u32), String> {
        let mut flags = 0;
        let mut problem = 0;
        if unsafe { CM_Get_DevNode_Status(&mut flags, &mut problem, info.instance, 0) } != 0 {
            return Err("USB hub status read failed".into());
        }
        Ok((flags, problem))
    }
    fn compare(a: &str, b: &str) -> Result<Ordering, String> {
        let a: Vec<u16> = a.encode_utf16().collect();
        let b: Vec<u16> = b.encode_utf16().collect();
        // Windows PowerShell Sort-Object uses the user's culture and ignores case.
        match unsafe {
            CompareStringEx(
                ptr::null(),
                1,
                a.as_ptr(),
                a.len() as i32,
                b.as_ptr(),
                b.len() as i32,
                ptr::null(),
                ptr::null_mut(),
                0,
            )
        } {
            1 => Ok(Ordering::Less),
            2 => Ok(Ordering::Equal),
            3 => Ok(Ordering::Greater),
            _ => Err("USB location path comparison failed".into()),
        }
    }
    pub fn hubs() -> Result<Vec<Hub>, String> {
        // Setup class (not a USB interface class) matches Get-PnpDevice -PresentOnly -Class USB.
        let raw = unsafe { SetupDiGetClassDevsW(&USB_CLASS, ptr::null(), 0, 2) };
        if raw == -1 {
            return Err("USB device enumeration could not start".into());
        }
        let set = Set(raw);
        let mut devices = Vec::new();
        for index in 0..4096 {
            let mut info = Info {
                size: size_of::<Info>() as u32,
                class: USB_CLASS,
                instance: 0,
                reserved: 0,
            };
            if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut info) } == 0 {
                if unsafe { GetLastError() } == 259 {
                    return finish(devices, compare);
                }
                return Err("USB device enumeration failed".into());
            }
            let instance = instance(&set, &mut info)?;
            let service = text(&set, &mut info, 6, false)?.ok_or("USB service property missing")?;
            let mut device = Device {
                instance,
                service,
                status: 0,
                problem: 0,
                paths: Vec::new(),
                label: String::new(),
            };
            if is_hub(&device.service) {
                let (kind, bytes) =
                    property(&set, &mut info, 37, false)?.ok_or("USB location paths missing")?;
                device.paths = paths_property(kind, &bytes)?;
                device.label = match text(&set, &mut info, 14, true)?.filter(|s| !s.is_empty()) {
                    Some(label) => label,
                    None => text(&set, &mut info, 2, true)?
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "USB Hub".into()),
                };
                // Check after property reads so a removed devnode cannot silently become an empty inventory.
                (device.status, device.problem) = status(&info)?;
            }
            devices.push(device);
        }
        Err("USB device enumeration exceeded limit".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "read-only hardware probe; requires PORTKEY_TEST_USB_READONLY=1"]
    fn native_readonly_probe() {
        assert_eq!(
            std::env::var("PORTKEY_TEST_USB_READONLY").as_deref(),
            Ok("1")
        );
        let output = std::path::PathBuf::from(
            std::env::var_os("PORTKEY_TEST_USB_OUTPUT").expect("Provide a private output path"),
        );
        assert!(output.is_absolute());
        let start = std::time::Instant::now();
        let hubs = hubs().expect("Native inventory failed");
        let elapsed = start.elapsed().as_micros();
        let result = serde_json::json!({"elapsed_us": elapsed, "hubs": hubs});
        std::fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    }
    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }
    fn device(instance: &str, paths: &[&str]) -> Device {
        Device {
            instance: instance.into(),
            service: "USBHUB3".into(),
            status: 8,
            problem: 0,
            paths: paths.iter().map(|p| (*p).into()).collect(),
            label: "Hub".into(),
        }
    }
    #[test]
    fn string_properties_require_type_termination_and_valid_utf16() {
        assert_eq!(string_property(18, &utf16("USBHUB3\0")).unwrap(), "USBHUB3");
        assert_eq!(string_property(0, &[]).unwrap(), "");
        for (kind, bytes) in [
            (7, utf16("x\0")),
            (18, utf16("x")),
            (18, utf16("x\0y\0")),
            (18, vec![0]),
            (18, vec![0, 0xd8, 0, 0]),
            (0, utf16("x")),
        ] {
            assert!(string_property(kind, &bytes).is_err());
        }
    }
    #[test]
    fn multistring_rejects_truncation_and_embedded_empty_entries() {
        assert_eq!(
            paths_property(0x2012, &utf16("PCIROOT(0)\0ACPI(_SB_)\0\0")).unwrap(),
            vec!["PCIROOT(0)", "ACPI(_SB_)"]
        );
        for text in ["", "\0", "\0\0", "x\0", "x\0\0y\0\0", "x\0\0\0"] {
            assert!(paths_property(0x2012, &utf16(text)).is_err(), "{text:?}");
        }
        assert!(paths_property(18, &utf16("x\0\0")).is_err());
    }
    #[test]
    fn full_topology_and_sort_selection_match_previous_identity() {
        let devices = vec![
            device(
                "USB\\A",
                &[
                    "PCIROOT(0)#USBROOT(0)#USB(2)",
                    "ACPI(_SB_)#ACPI(USB0)#ACPI(RHUB)#ACPI(PRT2)",
                ],
            ),
            device("USB\\B", &["PCIROOT(0)#USBROOT(0)#USB(3)"]),
        ];
        let hubs = finish(devices, |a, b| Ok(a.cmp(b))).unwrap();
        assert_eq!(
            hubs[0].id,
            "windows:acpi(_sb_)#acpi(usb0)#acpi(rhub)#acpi(prt2)"
        );
        assert_eq!(hubs[1].id, "windows:pciroot(0)#usbroot(0)#usb(3)");
    }
    #[test]
    fn incomplete_duplicate_or_problem_snapshots_are_errors() {
        assert!(finish(Vec::new(), |a, b| Ok(a.cmp(b))).is_err());
        for (status, problem) in [(0, 0), (8 | 0x400, 0), (8 | 0x8000, 0), (8, 22)] {
            let mut d = device("USB\\A", &["PCIROOT(0)#USBROOT(0)"]);
            d.status = status;
            d.problem = problem;
            assert!(finish(vec![d], |a, b| Ok(a.cmp(b))).is_err());
        }
        assert!(finish(
            vec![device("USB\\A", &["A"]), device("usb\\a", &["B"])],
            |a, b| Ok(a.cmp(b))
        )
        .is_err());
        assert!(finish(
            vec![device("USB\\A", &["A"]), device("USB\\B", &["a"])],
            |a, b| Ok(a.cmp(b))
        )
        .is_err());
        for paths in [vec![], vec![""], vec!["path with spaces"]] {
            assert!(finish(vec![device("USB\\A", &paths)], |a, b| Ok(a.cmp(b))).is_err());
        }
        assert!(finish(vec![device("USB\\A", &["A", "B"])], |_, _| Err(
            "sort failure".into()
        ))
        .is_err());
    }
    #[test]
    fn property_growth_removal_and_oversize_fail_closed() {
        let mut calls = 0;
        let result = read_property(
            |_| {
                calls += 1;
                if calls == 1 {
                    Reply {
                        kind: 18,
                        size: 4,
                        error: Some(122),
                    }
                } else {
                    Reply {
                        kind: 18,
                        size: 8,
                        error: Some(122),
                    }
                }
            },
            false,
        );
        assert!(result.is_err());
        assert_eq!(calls, 2);
        assert!(read_property(
            |_| Reply {
                kind: 18,
                size: 0,
                error: Some(433)
            },
            false
        )
        .is_err());
        assert!(read_property(
            |_| Reply {
                kind: 18,
                size: MAX_PROPERTY + 1,
                error: Some(122)
            },
            false
        )
        .is_err());
        assert!(read_property(
            |_| Reply {
                kind: 18,
                size: 0,
                error: Some(1168)
            },
            false
        )
        .is_err());
        assert!(read_property(
            |_| Reply {
                kind: 18,
                size: 0,
                error: Some(1168)
            },
            true
        )
        .unwrap()
        .is_none());
    }
    #[test]
    fn disappearance_and_short_success_during_property_read_are_errors() {
        for second in [
            Reply {
                kind: 18,
                size: 0,
                error: Some(433),
            },
            Reply {
                kind: 18,
                size: 2,
                error: None,
            },
            Reply {
                kind: 7,
                size: 4,
                error: None,
            },
        ] {
            let mut calls = 0;
            assert!(read_property(
                |_| {
                    calls += 1;
                    if calls == 1 {
                        Reply {
                            kind: 18,
                            size: 4,
                            error: Some(122),
                        }
                    } else {
                        Reply {
                            kind: second.kind,
                            size: second.size,
                            error: second.error,
                        }
                    }
                },
                false
            )
            .is_err());
            assert_eq!(calls, 2);
        }
    }
    #[test]
    fn explicit_empty_service_is_nonhub_but_no_hubs_is_not_absence() {
        let mut nonhub = device("USB\\OTHER", &[]);
        nonhub.service = String::new();
        let hubs = finish(vec![nonhub, device("USB\\HUB", &["ROOT#PORT"])], |a, b| {
            Ok(a.cmp(b))
        })
        .unwrap();
        assert_eq!(hubs.len(), 1);
        let mut nonhub = device("USB\\OTHER", &[]);
        nonhub.service = "USBXHCI".into();
        assert!(finish(vec![nonhub], |a, b| Ok(a.cmp(b))).is_err());
    }
}
