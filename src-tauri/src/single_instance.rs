//! A named per-session event enforces one main window per application channel.
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    use tauri::Manager;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateEventW(attributes: *const c_void, manual: i32, initial: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
        fn SetEvent(event: *mut c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn AllowSetForegroundWindow(process_id: u32) -> i32;
    }

    pub fn claim() -> Option<isize> {
        let name = if cfg!(feature = "internal-preview") { "Local\\YcodePreviewMainWindow" } else { "Local\\YcodeStableMainWindow" };
        claim_named(name)
    }

    fn claim_named(name: &str) -> Option<isize> {
        let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        // CreateEvent is atomic; an auto-reset event preserves a launch received during startup.
        unsafe {
            let event = CreateEventW(std::ptr::null(), 0, 0, name.as_ptr());
            if event.is_null() { return Some(0); }
            if GetLastError() == 183 {
                AllowSetForegroundWindow(u32::MAX);
                SetEvent(event);
                CloseHandle(event);
                return None;
            }
            Some(event as isize)
        }
    }

    pub fn listen(event: isize, app: tauri::AppHandle) {
        if event == 0 { return; }
        std::thread::spawn(move || loop {
            if unsafe { WaitForSingleObject(event as *mut c_void, u32::MAX) } != 0 { break; }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn second_launch_signals_first() {
            let name = format!("Local\\YcodeTest-{}", uuid::Uuid::new_v4());
            let event = claim_named(&name).unwrap();
            assert_ne!(event, 0);
            assert!(claim_named(&name).is_none());
            assert_eq!(unsafe { WaitForSingleObject(event as *mut c_void, 0) }, 0);
            unsafe { CloseHandle(event as *mut c_void); }
        }
    }
}

#[cfg(windows)]
pub use windows::{claim, listen};
