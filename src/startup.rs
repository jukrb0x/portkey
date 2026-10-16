/// Surface launch errors even when Finder/Windows do not attach a console.
pub fn show_error(message: &str) {
    eprintln!("PortKey: {message}");
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                flags: u32,
            ) -> i32;
        }
        let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<_> = "PortKey".encode_utf16().chain(Some(0)).collect();
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), 0x10);
    }
    #[cfg(target_os = "macos")]
    unsafe {
        use std::ffi::{c_void, CString};
        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            fn CFStringCreateWithCString(
                allocator: *const c_void,
                text: *const std::ffi::c_char,
                encoding: u32,
            ) -> *const c_void;
            fn CFUserNotificationDisplayAlert(
                timeout: f64,
                flags: usize,
                icon: *const c_void,
                sound: *const c_void,
                localization: *const c_void,
                title: *const c_void,
                message: *const c_void,
                default: *const c_void,
                alternate: *const c_void,
                other: *const c_void,
                response: *mut usize,
            ) -> i32;
            fn CFRelease(value: *const c_void);
        }
        let null = std::ptr::null();
        let message = CString::new(message.replace('\0', " ")).unwrap();
        let title = CFStringCreateWithCString(null, c"PortKey".as_ptr(), 0x08000100);
        let text = CFStringCreateWithCString(null, message.as_ptr(), 0x08000100);
        if !title.is_null() && !text.is_null() {
            let mut response = 0;
            CFUserNotificationDisplayAlert(
                0.0,
                0,
                null,
                null,
                null,
                title,
                text,
                null,
                null,
                null,
                &mut response,
            );
        }
        if !title.is_null() {
            CFRelease(title);
        }
        if !text.is_null() {
            CFRelease(text);
        }
    }
}
