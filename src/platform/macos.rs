use super::*;
use plist::Value;
use std::{io::Cursor, path::PathBuf};

fn helper() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("Cannot locate application directory")?;
    [
        parent.join("m1ddc"),
        parent.join("../Resources/m1ddc"),
        parent.join("../Helpers/m1ddc"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or_else(|| {
        "Bundled m1ddc helper missing. Use the packaged PortKey application or build-rust-app.sh."
            .into()
    })
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGGetOnlineDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> *const std::ffi::c_void;
    fn CGDisplayIsBuiltin(display: u32) -> u32;
    fn CGDisplayVendorNumber(display: u32) -> u32;
    fn CGDisplayModelNumber(display: u32) -> u32;
}
fn displays() -> Result<Vec<Display>, String> {
    unsafe {
        let mut ids = [0u32; 64];
        let mut count = 0;
        if CGGetOnlineDisplayList(64, ids.as_mut_ptr(), &mut count) != 0 || count > 64 {
            return Err("Display enumeration failed".into());
        }
        let mut displays = Vec::new();
        for display in &ids[..count as usize] {
            if CGDisplayIsBuiltin(*display) != 0 {
                continue;
            }
            let uuid = CGDisplayCreateUUIDFromDisplayID(*display);
            if uuid.is_null() {
                return Err("Display UUID unavailable".into());
            }
            let value = CFUUIDCreateString(std::ptr::null(), uuid);
            CFRelease(uuid);
            if value.is_null() {
                return Err("Display UUID could not be decoded".into());
            }
            let mut buffer = [0 as std::ffi::c_char; 128];
            let valid = CFStringGetCString(value, buffer.as_mut_ptr(), 128, 0x08000100);
            CFRelease(value);
            if valid == 0 {
                return Err("Invalid display UUID".into());
            }
            let id = std::ffi::CStr::from_ptr(buffer.as_ptr())
                .to_str()
                .map_err(|_| "Invalid display UUID")?
                .to_ascii_uppercase();
            let label = format!(
                "Display {:04X}:{:04X} · {}",
                CGDisplayVendorNumber(*display),
                CGDisplayModelNumber(*display),
                id
            );
            displays.push(Display { id, label });
        }
        Ok(displays)
    }
}
fn write_args(id: &str, input: u8) -> [String; 5] {
    [
        "display".into(),
        id.into(),
        "set".into(),
        "input".into(),
        input.to_string(),
    ]
}
pub fn inventory() -> Result<Inventory, String> {
    let displays = displays()?;
    Ok(Inventory {
        displays,
        hubs: usb_inventory()?,
    })
}
pub fn set_input(config: &Config, input: u8) -> Result<WriteOutcome, String> {
    config.validate()?;
    if config.platform != "macos" || ![config.local_input, config.remote_input].contains(&input) {
        return Err("Invalid platform or unenrolled input".into());
    }
    require_display(&displays()?, &config.display_id)?;
    run(
        Command::new(helper()?).args(write_args(&config.display_id, input)),
        Duration::from_secs(5),
    )?;
    // The pinned helper does not fully validate DDC replies. Never promote its numeric output to confirmation.
    Ok(WriteOutcome::SentUnverified(
        "DDC command sent; confirm the display changed inputs".into(),
    ))
}
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceMatching(name: *const std::ffi::c_char) -> *const std::ffi::c_void;
    fn IOServiceGetMatchingServices(
        port: u32,
        matching: *const std::ffi::c_void,
        iterator: *mut u32,
    ) -> i32;
    fn IORegistryGetRootEntry(port: u32) -> u32;
    fn IOObjectIsEqualTo(first: u32, second: u32) -> u32;
    fn IORegistryEntryGetPath(
        entry: u32,
        plane: *const std::ffi::c_char,
        path: *mut std::ffi::c_char,
    ) -> i32;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOIteratorIsValid(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> i32;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        props: *mut *const std::ffi::c_void,
        allocator: *const std::ffi::c_void,
        options: u32,
    ) -> i32;
    fn IORegistryEntryGetParentEntry(
        entry: u32,
        plane: *const std::ffi::c_char,
        parent: *mut u32,
    ) -> i32;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFUUIDCreateString(
        allocator: *const std::ffi::c_void,
        uuid: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFStringGetCString(
        value: *const std::ffi::c_void,
        buffer: *mut std::ffi::c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    fn CFPropertyListCreateXMLData(
        allocator: *const std::ffi::c_void,
        property: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFDataGetLength(data: *const std::ffi::c_void) -> isize;
    fn CFDataGetBytePtr(data: *const std::ffi::c_void) -> *const u8;
    fn CFRelease(object: *const std::ffi::c_void);
}
struct Object(u32);
impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            IOObjectRelease(self.0);
        }
    }
}
fn properties(entry: u32) -> Result<Value, String> {
    unsafe {
        let mut props = std::ptr::null();
        if IORegistryEntryCreateCFProperties(entry, &mut props, std::ptr::null(), 0) != 0
            || props.is_null()
        {
            return Err("USB property read failed".into());
        }
        let data = CFPropertyListCreateXMLData(std::ptr::null(), props);
        CFRelease(props);
        if data.is_null() {
            return Err("USB properties could not be decoded".into());
        }
        let bytes =
            std::slice::from_raw_parts(CFDataGetBytePtr(data), CFDataGetLength(data) as usize);
        let result = Value::from_reader_xml(Cursor::new(bytes)).map_err(|e| e.to_string());
        CFRelease(data);
        result
    }
}
fn matching_empty(status: i32, iterator: u32) -> Result<bool, String> {
    if status != 0 {
        return Err("USB enumeration failed".into());
    }
    // IOKitLib.h explicitly defines success + NULL as no matching services.
    Ok(iterator == 0)
}
fn usb_inventory() -> Result<Vec<Hub>, String> {
    unsafe {
        let matching = IOServiceMatching(c"IOUSBHostDevice".as_ptr());
        if matching.is_null() {
            return Err("USB matching unavailable".into());
        }
        let mut iterator = 0;
        let status = IOServiceGetMatchingServices(0, matching, &mut iterator);
        if matching_empty(status, iterator)? {
            return Ok(Vec::new());
        }
        let iterator = Object(iterator);
        let root = Object(IORegistryGetRootEntry(0));
        if root.0 == 0 {
            return Err("Registry root unavailable".into());
        }
        let mut hubs = Vec::new();
        loop {
            let entry = IOIteratorNext(iterator.0);
            if entry == 0 {
                break;
            }
            let entry = Object(entry);
            let props = properties(entry.0)?;
            let mut parent = None;
            let mut current = entry.0;
            let mut retained = None;
            let mut root_path = None;
            loop {
                if IOObjectIsEqualTo(current, root.0) != 0 {
                    break;
                }
                let mut next = 0;
                if IORegistryEntryGetParentEntry(current, c"IOService".as_ptr(), &mut next) != 0 {
                    return Err("USB parent topology read failed".into());
                }
                if root_path.is_none() {
                    let mut buffer = [0 as std::ffi::c_char; 4096];
                    if IORegistryEntryGetPath(next, c"IOService".as_ptr(), buffer.as_mut_ptr()) != 0
                    {
                        return Err("USB controller path unavailable".into());
                    }
                    root_path = Some(
                        std::ffi::CStr::from_ptr(buffer.as_ptr())
                            .to_str()
                            .map_err(|_| "Invalid controller path")?
                            .to_owned(),
                    );
                }
                retained = Some(Object(next));
                current = next;
                if IOObjectIsEqualTo(current, root.0) != 0 {
                    break;
                }
                let p = properties(current)?;
                let d = p.as_dictionary().ok_or("Invalid parent properties")?;
                if let (Some(v), Some(p), Some(l)) = (
                    d.get("idVendor").and_then(Value::as_unsigned_integer),
                    d.get("idProduct").and_then(Value::as_unsigned_integer),
                    d.get("locationID").and_then(Value::as_unsigned_integer),
                ) {
                    parent = Some((v, p, l));
                    break;
                }
                if d.contains_key("idVendor") || d.contains_key("idProduct") {
                    return Err("Incomplete USB ancestor identity".into());
                }
            }
            drop(retained);
            let before = hubs.len();
            walk(&props, parent, &mut hubs)?;
            if parent.is_none() && hubs.len() > before {
                let path = root_path.ok_or("USB root topology missing")?;
                hubs[before].id = root_identity(&hubs[before].id, &path)?;
            }
        }
        if IOIteratorIsValid(iterator.0) == 0 {
            return Err("USB inventory changed during enumeration".into());
        }
        hubs.sort_by(|a, b| a.id.cmp(&b.id));
        if hubs.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("Duplicate USB topology identity".into());
        }
        Ok(hubs)
    }
}
fn root_identity(id: &str, path: &str) -> Result<String, String> {
    if !path.starts_with("IOService:/") || path.len() <= 11 {
        return Err("Incomplete USB controller topology".into());
    }
    let encoded = path.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    Ok(format!(
        "{}:root:{encoded}",
        id.split(':').take(4).collect::<Vec<_>>().join(":")
    ))
}
#[cfg(test)]
fn parse_hubs(text: &str) -> Result<Vec<Hub>, String> {
    let root = Value::from_reader_xml(Cursor::new(text))
        .map_err(|e| format!("Invalid USB inventory: {e}"))?;
    let roots = root
        .as_array()
        .ok_or("USB inventory root must be an array")?;
    if roots.is_empty() {
        return Err("USB registry root missing".into());
    }
    let mut hubs = Vec::new();
    for node in roots {
        walk(node, None, &mut hubs)?;
    }
    let mut ids = std::collections::HashSet::new();
    if hubs.iter().any(|h| !ids.insert(h.id.clone())) {
        return Err("Duplicate USB topology identity".into());
    }
    hubs.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(hubs)
}
fn walk(node: &Value, parent: Option<(u64, u64, u64)>, hubs: &mut Vec<Hub>) -> Result<(), String> {
    let props = node.as_dictionary().ok_or("Invalid USB registry node")?;
    let number = |key: &str| props.get(key).and_then(Value::as_unsigned_integer);
    let identity = match (
        number("idVendor"),
        number("idProduct"),
        number("locationID"),
    ) {
        (Some(v), Some(p), Some(l)) => Some((v, p, l)),
        (None, None, None) if number("bDeviceClass").is_none() => None,
        _ => return Err("Incomplete USB identity; automation paused".into()),
    };
    if number("bDeviceClass") == Some(9) {
        let (v, p, l) = identity.ok_or("Hub lacks topology identity")?;
        let (pv, pp, pl) = parent.unwrap_or((0, 0, 0));
        hubs.push(Hub {
            id: format!("macos:{v:04x}:{p:04x}:{l:08x}:{pv:04x}:{pp:04x}:{pl:08x}"),
            label: format!(
                "{} · {v:04x}:{p:04x} @ {l:08x}",
                props
                    .get("USB Product Name")
                    .and_then(Value::as_string)
                    .unwrap_or("USB Hub")
            ),
        });
    }
    if let Some(children) = props.get("IORegistryEntryChildren") {
        for child in children.as_array().ok_or("Invalid USB registry children")? {
            walk(child, identity.or(parent), hubs)?;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_iterator_is_empty_only_after_success() {
        assert_eq!(matching_empty(0, 0), Ok(true));
        assert_eq!(matching_empty(0, 42), Ok(false));
        assert!(matching_empty(-1, 0).is_err());
    }
    #[test]
    fn direct_hub_requires_explicit_controller_path() {
        let base = "macos:0001:0002:01000000:0000:0000:00000000";
        assert!(root_identity(base, "").is_err());
        assert!(root_identity(base, "IOService:/").is_err());
        let a = root_identity(base, "IOService:/controller@1/port@2").unwrap();
        let b = root_identity(base, "IOService:/controller@1/port@3").unwrap();
        assert!(a.contains(":root:494f536572766963653a2f"));
        assert_ne!(a, b);
    }
    #[test]
    fn write_arguments_use_exact_uuid_and_input() {
        let id = "01234567-89AB-CDEF-0123-456789ABCDEF";
        assert_eq!(
            write_args(id, 17),
            ["display", id, "set", "input", "17"].map(String::from)
        );
    }
    #[test]
    fn identical_models_keep_topology_and_parent() {
        let xml = r#"<?xml version="1.0"?><plist version="1.0"><array><dict><key>idVendor</key><integer>1</integer><key>idProduct</key><integer>2</integer><key>locationID</key><integer>16</integer><key>bDeviceClass</key><integer>9</integer><key>IORegistryEntryChildren</key><array><dict><key>idVendor</key><integer>1</integer><key>idProduct</key><integer>2</integer><key>locationID</key><integer>17</integer><key>bDeviceClass</key><integer>9</integer></dict></array></dict></array></plist>"#;
        let hubs = parse_hubs(xml).unwrap();
        assert_eq!(hubs.len(), 2);
        assert_ne!(hubs[0].id, hubs[1].id);
        assert!(hubs[1].id.ends_with(":0001:0002:00000010"));
        assert!(parse_hubs(&xml.replace("<key>locationID</key>", "<key>missing</key>")).is_err());
        assert!(parse_hubs("<plist><array/></plist>").is_err());
    }
}
