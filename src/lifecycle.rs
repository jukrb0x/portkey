//! Process-lifetime native power subscription; no polling can miss a short suspend.
use std::sync::atomic::{AtomicU64, Ordering};
// Generation and suspend bit are one snapshot, including across a full short sleep.
static STATE: AtomicU64 = AtomicU64::new(0);

pub fn snapshot() -> (u64, bool) {
    let state = STATE.load(Ordering::SeqCst);
    (state >> 1, state & 1 != 0)
}
fn transition(sleeping: bool) {
    let mut old = STATE.load(Ordering::SeqCst);
    loop {
        let new = ((old >> 1).wrapping_add(1) << 1) | u64::from(sleeping);
        match STATE.compare_exchange(old, new, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(current) => old = current,
        }
    }
}

#[cfg(target_os = "macos")]
pub fn start() -> Result<(), String> {
    use std::{ffi::c_void, ptr};
    static CONNECTION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IORegisterForSystemPower(
            context: *mut c_void,
            port: *mut *mut c_void,
            callback: unsafe extern "C" fn(*mut c_void, u32, u32, *mut c_void),
            notifier: *mut u32,
        ) -> u32;
        fn IOAllowPowerChange(connection: u32, notification: isize) -> i32;
        fn IONotificationPortGetRunLoopSource(port: *mut c_void) -> *mut c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopAddSource(runloop: *mut c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
    }
    unsafe extern "C" fn callback(_: *mut c_void, _: u32, message: u32, argument: *mut c_void) {
        match message {
            0xe0000280 => transition(true),
            0xe0000300 => transition(false),
            _ => {}
        }
        if matches!(message, 0xe0000270 | 0xe0000280) {
            IOAllowPowerChange(CONNECTION.load(Ordering::SeqCst), argument as isize);
        }
    }
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || unsafe {
        let mut port = ptr::null_mut();
        let mut notifier = 0;
        let connection =
            IORegisterForSystemPower(ptr::null_mut(), &mut port, callback, &mut notifier);
        if connection == 0 || port.is_null() {
            let _ = send.send(false);
            return;
        }
        CONNECTION.store(connection, Ordering::SeqCst);
        CFRunLoopAddSource(
            CFRunLoopGetCurrent(),
            IONotificationPortGetRunLoopSource(port),
            kCFRunLoopDefaultMode,
        );
        let _ = send.send(true);
        CFRunLoopRun();
    });
    if receive.recv_timeout(std::time::Duration::from_secs(2)) == Ok(true) {
        Ok(())
    } else {
        Err("Cannot subscribe to system sleep events; switching is disabled".into())
    }
}

#[cfg(target_os = "windows")]
pub fn start() -> Result<(), String> {
    use std::{ffi::c_void, ptr};
    #[repr(C)]
    struct Subscription {
        callback: unsafe extern "system" fn(*const c_void, u32, *const c_void) -> u32,
        context: *const c_void,
    }
    #[link(name = "powrprof")]
    extern "system" {
        fn PowerRegisterSuspendResumeNotification(
            flags: u32,
            recipient: *const c_void,
            handle: *mut *mut c_void,
        ) -> u32;
    }
    unsafe extern "system" fn callback(_: *const c_void, event: u32, _: *const c_void) -> u32 {
        match event {
            4 => transition(true),
            7 | 18 => transition(false),
            _ => {}
        }
        0
    }
    // Registration and callback storage intentionally live until process termination.
    let subscription = Box::new(Subscription {
        callback,
        context: ptr::null(),
    });
    let mut handle = ptr::null_mut();
    let code = unsafe {
        PowerRegisterSuspendResumeNotification(
            2,
            (&*subscription as *const Subscription).cast(),
            &mut handle,
        )
    };
    if code != 0 {
        return Err(format!(
            "Cannot subscribe to system power events ({code}); switching is disabled"
        ));
    }
    Box::leak(subscription);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start() -> Result<(), String> {
    Err("Unsupported operating system".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspend_and_resume_invalidate_even_without_elapsed_time() {
        let before = snapshot().0;
        transition(true);
        assert_eq!(snapshot(), (before + 1, true));
        transition(false);
        assert_eq!(snapshot(), (before + 2, false));
    }
}
