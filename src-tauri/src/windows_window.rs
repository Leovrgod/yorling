#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::mem::size_of_val;
use tauri::{Runtime, WebviewWindow};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
};

pub fn configure_main_window<R: Runtime>(window: &WebviewWindow<R>) {
    if let Err(error) = window.set_shadow(true) {
        log::warn!("Failed to enable Windows main window shadow: {error}");
    }

    let Ok(hwnd) = window.hwnd() else {
        log::warn!("Failed to access the Windows main window handle");
        return;
    };

    let corner_preference = DWMWCP_ROUND;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner_preference as *const _ as *const c_void,
            size_of_val(&corner_preference) as u32,
        )
    };

    if result < 0 {
        log::warn!(
            "Failed to request rounded Windows main window corners: HRESULT 0x{:08X}",
            result as u32
        );
    }
}
