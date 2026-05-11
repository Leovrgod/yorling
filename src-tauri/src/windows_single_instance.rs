#![cfg(target_os = "windows")]

use std::ptr;
use std::sync::OnceLock;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

const MAIN_WINDOW_TITLE: &str = "YORLING";
const INSTANCE_MUTEX_NAME: &str = "Local\\Yorling.SingleInstance";

static INSTANCE_MUTEX_HANDLE: OnceLock<usize> = OnceLock::new();

pub fn claim_or_focus_existing(present_existing: bool) -> bool {
    if focus_existing_window(present_existing) {
        return false;
    }

    let mutex_name = wide_null(INSTANCE_MUTEX_NAME);
    let handle = unsafe { CreateMutexW(ptr::null(), 1, mutex_name.as_ptr()) };
    if handle.is_null() {
        log::warn!("Failed to create Yorling single-instance mutex");
        return true;
    }

    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(handle);
        }
        let _ = focus_existing_window(present_existing);
        return false;
    }

    let _ = INSTANCE_MUTEX_HANDLE.set(handle as usize);
    true
}

fn focus_existing_window(present_existing: bool) -> bool {
    let title = wide_null(MAIN_WINDOW_TITLE);
    let hwnd = unsafe { FindWindowW(ptr::null(), title.as_ptr()) };
    if hwnd.is_null() {
        return false;
    }

    if present_existing {
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }

    true
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
