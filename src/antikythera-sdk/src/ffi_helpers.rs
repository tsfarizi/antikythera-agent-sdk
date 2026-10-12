//! Shared FFI Helper Functions
//!
//! Common utilities used across all FFI modules (security, session, servers).
//! Eliminates duplicate helper implementations previously scattered in
//! `security_ffi/helpers.rs`, `session/ffi/helpers.rs`, and `servers/mod.rs`.
//!
//! Functions below are used by the ffi_handler! macro and external FFI callers.
//! Suppress dead_code warnings since macro-driven usage isn't detected.
#![allow(dead_code)]

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Convert C string to Rust string
pub(crate) fn from_c_string(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("Null pointer provided".to_string());
    }

    unsafe {
        CStr::from_ptr(ptr)
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| format!("Invalid UTF-8: {}", e))
    }
}

/// Convert Rust string to C string (caller must free)
pub(crate) fn to_c_string(s: &str) -> *mut c_char {
    match CString::new(s) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Create error response
pub(crate) fn error_response(message: &str) -> *mut c_char {
    let json = serde_json::json!({
        "success": false,
        "error": message
    });
    serialize_result(&json)
}

/// Serialize result to C string
pub(crate) fn serialize_result<T: serde::Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => to_c_string(&json),
        Err(_) => error_response("Failed to serialize result"),
    }
}

/// Free C string allocated by Rust
///
/// # Safety
///
/// The pointer must be a valid pointer to a C string allocated by Rust using `into_raw`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mcp_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

/// Eliminates boilerplate for `extern "C"` FFI functions that follow the
/// canonical pattern: parse C string → log → lock global → match guard.
///
/// ## Variants
///
/// ### With input (single C string parameter):
/// ```ignore
/// ffi_handler!("fn_name", logger, input_ptr, GLOBAL, |instance| {
///     // use `input_str` and `instance`
/// })
/// ```
///
/// ### Without input (guard check only):
/// ```ignore
/// ffi_handler!("fn_name", logger, GLOBAL, |instance| {
///     // use `instance`
/// })
/// ```
#[macro_export]
/// With input parsing + logger.
///
/// Without input (just guard check).
///
macro_rules! ffi_handler {
    ($fn_name:expr, $logger:expr, $input:expr, $global:expr, |$instance:ident| $($body:tt)*) => {{
        let input_str = match $crate::ffi_helpers::from_c_string($input) {
            Ok(s) => s,
            Err(e) => {
                $logger.ffi_error($fn_name, &e);
                return $crate::ffi_helpers::error_response(&e);
            }
        };
        $logger.ffi_call($fn_name, &format!("input={}", input_str));
        let guard = $global.lock().unwrap();
        match guard.as_ref() {
            Some($instance) => $($body)*,
            None => {
                $logger.ffi_error($fn_name, "Not initialized");
                $crate::ffi_helpers::error_response("Not initialized")
            }
        }
    }};
    ($fn_name:expr, $logger:expr, $global:expr, |$instance:ident| $($body:tt)*) => {{
        $logger.ffi_call($fn_name, "");
        let guard = $global.lock().unwrap();
        match guard.as_ref() {
            Some($instance) => $($body)*,
            None => {
                $logger.ffi_error($fn_name, "Not initialized");
                $crate::ffi_helpers::error_response("Not initialized")
            }
        }
    }};
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-sdk/ffi_helpers.tests.rs"]
mod tests;
