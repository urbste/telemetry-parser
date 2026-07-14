// SPDX-License-Identifier: MIT OR Apache-2.0
// iOS C FFI bindings for telemetry-parser

#[cfg(any(target_os = "ios", target_os = "macos"))]
mod implementation {
    use std::ffi::{c_char, CStr, CString};
    use std::sync::Mutex;

    use crate::mobile;

    static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

    fn set_last_error(message: String) {
        if let Ok(mut slot) = LAST_ERROR.lock() {
            *slot = Some(message);
        }
    }

    fn clear_last_error() {
        if let Ok(mut slot) = LAST_ERROR.lock() {
            *slot = None;
        }
    }

    fn to_c_string(value: &str) -> *mut c_char {
        CString::new(value)
            .map(|s| s.into_raw())
            .unwrap_or(std::ptr::null_mut())
    }

    fn json_to_c_string<T: serde::Serialize>(value: &T) -> *mut c_char {
        match serde_json::to_string(value) {
            Ok(json) => to_c_string(&json),
            Err(e) => {
                set_last_error(format!("JSON serialization failed: {}", e));
                std::ptr::null_mut()
            }
        }
    }

    /// Open a video file and return an opaque handle (0 on failure).
    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_open(path: *const c_char) -> u64 {
        clear_last_error();
        if path.is_null() {
            set_last_error("Path is null".to_owned());
            return 0;
        }
        let path_str = match unsafe { CStr::from_ptr(path) }.to_str() {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("Invalid UTF-8 path: {}", e));
                return 0;
            }
        };
        match mobile::open(path_str) {
            Ok(handle) => handle,
            Err(e) => {
                set_last_error(e);
                0
            }
        }
    }

    /// Release resources associated with a handle.
    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_close(handle: u64) {
        mobile::close(handle);
    }

    /// Free a string returned by this library.
    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_free_string(value: *mut c_char) {
        if !value.is_null() {
            unsafe {
                drop(CString::from_raw(value));
            }
        }
    }

    /// Last error message from the most recent failing call, or null.
    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_last_error() -> *mut c_char {
        LAST_ERROR
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|s| to_c_string(s)))
            .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_camera_type(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| to_c_string(&data.camera_type))
            .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_duration(handle: u64) -> f32 {
        mobile::with_handle(handle, |data| data.duration_s as f32).unwrap_or(0.0)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_video_info_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            data.video_info_json()
                .map(|info| json_to_c_string(&info))
                .unwrap_or(std::ptr::null_mut())
        })
        .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_lens_metadata_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            data.lens_json
                .as_ref()
                .map(|s| to_c_string(s))
                .unwrap_or(std::ptr::null_mut())
        })
        .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_gps_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| json_to_c_string(&data.gps_points))
            .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_accelerometer_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            json_to_c_string(&data.imu_samples_json(|d| d.accl))
        })
        .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_gyroscope_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| json_to_c_string(&data.imu_samples_json(|d| d.gyro)))
            .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_gravity_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| json_to_c_string(&data.imu_samples_json(|d| d.grav)))
            .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_camera_orientation_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            json_to_c_string(&data.orientation_samples_json(&data.cori))
        })
        .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_image_orientation_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            json_to_c_string(&data.orientation_samples_json(&data.iori))
        })
        .unwrap_or(std::ptr::null_mut())
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn telemetry_parser_get_orientation_combined_json(handle: u64) -> *mut c_char {
        mobile::with_handle(handle, |data| {
            json_to_c_string(&data.orientation_samples_json(&data.orientation_combined))
        })
        .unwrap_or(std::ptr::null_mut())
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
mod implementation {}
