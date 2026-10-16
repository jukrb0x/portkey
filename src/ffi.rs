//! Narrow C bridge. Handles and returned strings are owned by the calling UI thread.
use crate::controller::Controller;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    panic::{catch_unwind, AssertUnwindSafe},
};

struct Handle {
    owner: std::thread::ThreadId,
    controller: Controller,
}
fn response(value: Value) -> *mut c_char {
    // JSON escapes embedded NUL, so this conversion cannot contain an interior NUL.
    CString::new(value.to_string())
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
fn guarded(action: impl FnOnce() -> Value) -> *mut c_char {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(value) => response(value),
        Err(_) => response(
            json!({"ok":false,"error":"Internal controller error; quit and restart PortKey."}),
        ),
    }
}

#[no_mangle]
pub extern "C" fn pk_create() -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        Box::into_raw(Box::new(Handle {
            owner: std::thread::current().id(),
            controller: Controller::new(),
        }))
        .cast()
    }))
    .unwrap_or(std::ptr::null_mut())
}
unsafe fn handle<'a>(pointer: *mut c_void) -> Result<&'a mut Handle, &'static str> {
    let handle = pointer
        .cast::<Handle>()
        .as_mut()
        .ok_or("Null PortKey handle")?;
    if handle.owner != std::thread::current().id() {
        return Err("PortKey must be called on its creating UI thread");
    }
    Ok(handle)
}
/// # Safety
/// `pointer` must be null or a live handle from `pk_create`, used on its creating UI thread.
#[no_mangle]
pub unsafe extern "C" fn pk_snapshot(pointer: *mut c_void) -> *mut c_char {
    guarded(|| match handle(pointer) {
        Ok(handle) => handle.controller.snapshot(),
        Err(error) => json!({"ok":false,"error":error}),
    })
}
/// # Safety
/// `pointer` obeys `pk_snapshot`'s contract. `action` is null or a valid NUL-terminated UTF-8 string.
#[no_mangle]
pub unsafe extern "C" fn pk_action(pointer: *mut c_void, action: *const c_char) -> *mut c_char {
    guarded(|| {
        let result = (|| {
            let handle = handle(pointer).map_err(str::to_owned)?;
            if action.is_null() {
                return Err("Null action string".to_string());
            }
            let bytes = CStr::from_ptr(action).to_bytes();
            let value =
                serde_json::from_slice(bytes).map_err(|e| format!("Invalid action JSON: {e}"))?;
            Ok(handle.controller.action(value))
        })();
        match result {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        }
    })
}
/// # Safety
/// `string` is null or an unfreed string returned by this bridge. Free each string exactly once.
#[no_mangle]
pub unsafe extern "C" fn pk_string_free(string: *mut c_char) {
    if !string.is_null() {
        let _ = catch_unwind(AssertUnwindSafe(|| drop(CString::from_raw(string))));
    }
}
/// # Safety
/// `pointer` is null or an undestroyed handle from `pk_create`, on its creating UI thread.
#[no_mangle]
pub unsafe extern "C" fn pk_destroy(pointer: *mut c_void) {
    if pointer.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(handle) = handle(pointer) {
            handle.controller.action(json!({"type":"quit"}));
            drop(Box::from_raw(pointer.cast::<Handle>()));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_handle_and_invalid_json_return_owned_errors() {
        unsafe {
            let value = pk_snapshot(std::ptr::null_mut());
            assert!(!value.is_null());
            let parsed: serde_json::Value =
                serde_json::from_slice(CStr::from_ptr(value).to_bytes()).unwrap();
            assert_eq!(parsed["ok"], false);
            pk_string_free(value);
            let value = pk_action(std::ptr::null_mut(), std::ptr::null());
            assert!(!value.is_null());
            pk_string_free(value);
            pk_destroy(std::ptr::null_mut());
            pk_string_free(std::ptr::null_mut());
        }
    }
    #[test]
    fn invalid_json_and_wrong_thread_are_rejected_without_controller_actions() {
        // Build a dormant controller so ABI validation never touches hardware or user config.
        let controller = Controller::test_instance();
        let pointer = Box::into_raw(Box::new(Handle {
            owner: std::thread::current().id(),
            controller,
        }))
        .cast();
        unsafe {
            for action in [c"{", c"null", c"{\"type\":\"manual\",\"target\":\"bogus\"}"] {
                let result = pk_action(pointer, action.as_ptr());
                let value: Value =
                    serde_json::from_slice(CStr::from_ptr(result).to_bytes()).unwrap();
                assert_eq!(value["ok"], false);
                pk_string_free(result);
            }
            let address = pointer as usize;
            std::thread::spawn(move || {
                let result = pk_snapshot(address as *mut c_void);
                let value: Value =
                    serde_json::from_slice(CStr::from_ptr(result).to_bytes()).unwrap();
                assert_eq!(value["ok"], false);
                pk_string_free(result);
            })
            .join()
            .unwrap();
            pk_destroy(pointer);
        }
    }
}
