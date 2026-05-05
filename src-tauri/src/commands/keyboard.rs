use crate::alt_tab::{AltTabManager, AltTabOverlayState};
use crate::bracket_overlay::{BracketOverlayManager, BracketOverlayState};
use serde::Serialize;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, State};
use yorling_engine::engine::SystemAction;

#[cfg(target_os = "windows")]
use crate::windows_keyboard::WindowsKeyboardInterceptor;
#[cfg(target_os = "macos")]
use core_graphics::access::ScreenCaptureAccess;
#[cfg(target_os = "macos")]
use yorling_platform_macos::KeyboardInterceptor;

#[derive(Debug, Clone, Serialize)]
pub struct EngineStatus {
    pub running: bool,
    pub enabled: bool,
    pub event_count: u64,
    pub has_accessibility: bool,
    pub has_screen_recording: bool,
    pub platform: &'static str,
    pub interception_supported: bool,
    pub requires_accessibility: bool,
    pub requires_screen_recording: bool,
    pub elevation_limited: bool,
}

#[derive(Debug, Clone, Serialize)]
struct MusicNativeKeyEvent {
    code: String,
    key_down: bool,
}

pub struct InterceptorState {
    alt_tab: AltTabManager,
    bracket_overlay: BracketOverlayManager,
    #[cfg(target_os = "macos")]
    interceptor: KeyboardInterceptor,
    #[cfg(target_os = "windows")]
    interceptor: WindowsKeyboardInterceptor,
    #[cfg(target_os = "macos")]
    music_suppression_owns_interceptor: AtomicBool,
}

impl InterceptorState {
    pub fn new() -> Self {
        Self {
            alt_tab: AltTabManager::new(),
            bracket_overlay: BracketOverlayManager::new(),
            #[cfg(target_os = "macos")]
            interceptor: KeyboardInterceptor::new(),
            #[cfg(target_os = "windows")]
            interceptor: WindowsKeyboardInterceptor::new(),
            #[cfg(target_os = "macos")]
            music_suppression_owns_interceptor: AtomicBool::new(false),
        }
    }

    pub fn bind_app_handle(&self, app_handle: tauri::AppHandle) {
        let app_handle_for_music = app_handle.clone();
        self.alt_tab.bind_app_handle(app_handle.clone());
        self.bracket_overlay.bind_app_handle(app_handle);

        #[cfg(target_os = "macos")]
        {
            let alt_tab = self.alt_tab.clone();
            let bracket_overlay = self.bracket_overlay.clone();
            self.interceptor
                .set_system_action_handler(move |action| match action {
                    SystemAction::AltTabCycle { .. }
                    | SystemAction::AltTabCommit
                    | SystemAction::AltTabCancel => alt_tab.dispatch(action),
                    SystemAction::BracketModeChanged { .. } => bracket_overlay.dispatch(action),
                    // Mouse actions are handled directly in the interceptor callback;
                    // MouseModeChanged could be used for UI feedback in the future.
                    SystemAction::MouseMove { .. }
                    | SystemAction::MouseClick { .. }
                    | SystemAction::MouseScroll { .. }
                    | SystemAction::MouseModeChanged { .. } => {}
                });

            self.interceptor
                .set_music_key_handler(move |code, key_down| {
                    let payload = MusicNativeKeyEvent {
                        code: code.to_string(),
                        key_down,
                    };
                    let _ = app_handle_for_music.emit("music-native-key", payload);
                });
        }

        #[cfg(target_os = "windows")]
        {
            let bracket_overlay = self.bracket_overlay.clone();
            self.interceptor
                .set_system_action_handler(move |action| match action {
                    SystemAction::BracketModeChanged { .. } => bracket_overlay.dispatch(action),
                    // Windows handles mouse actions in the hook thread. The custom macOS
                    // Alt+Tab overlay is intentionally disabled on Windows.
                    SystemAction::MouseMove { .. }
                    | SystemAction::MouseClick { .. }
                    | SystemAction::MouseScroll { .. }
                    | SystemAction::MouseModeChanged { .. }
                    | SystemAction::AltTabCycle { .. }
                    | SystemAction::AltTabCommit
                    | SystemAction::AltTabCancel => {}
                });
        }
    }

    pub fn alt_tab_overlay_state(&self) -> AltTabOverlayState {
        self.alt_tab.overlay_state()
    }

    pub fn bracket_overlay_state(&self) -> BracketOverlayState {
        self.bracket_overlay.overlay_state()
    }

    pub fn select_alt_tab_window(&self, window_id: u32) {
        self.alt_tab.select_window(window_id);
    }

    pub fn set_alt_tab_hovered_window(&self, window_id: Option<u32>) {
        self.alt_tab.set_hovered_window(window_id);
    }

    pub fn activate_alt_tab_window(&self, window_id: u32) {
        #[cfg(target_os = "macos")]
        self.interceptor.cancel_alt_tab_session();

        self.alt_tab.activate_window(window_id);
    }

    pub fn alt_tab_thumbnail_data_url(&self, window_id: u32) -> Option<String> {
        self.alt_tab.thumbnail_data_url(window_id)
    }

    pub fn alt_tab_app_icon_data_url(&self, owner_pid: i32) -> Option<String> {
        self.alt_tab.app_icon_data_url(owner_pid)
    }
}

#[tauri::command]
pub async fn start_interceptor(state: State<'_, Arc<InterceptorState>>) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        state
            .music_suppression_owns_interceptor
            .store(false, Ordering::SeqCst);
        state.interceptor.start()?;
        Ok("Interceptor started".into())
    }
    #[cfg(target_os = "windows")]
    {
        state.interceptor.start()?;
        Ok("Interceptor started".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = state;
        Err("Keyboard interception not supported on this platform yet".into())
    }
}

#[tauri::command]
pub async fn stop_interceptor(state: State<'_, Arc<InterceptorState>>) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        state
            .music_suppression_owns_interceptor
            .store(false, Ordering::SeqCst);
        state.interceptor.stop();
        Ok("Interceptor stopped".into())
    }
    #[cfg(target_os = "windows")]
    {
        state.interceptor.stop();
        Ok("Interceptor stopped".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = state;
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
pub async fn set_enabled(
    state: State<'_, Arc<InterceptorState>>,
    enabled: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if enabled {
            state
                .music_suppression_owns_interceptor
                .store(false, Ordering::SeqCst);
        }
        state.interceptor.set_enabled(enabled);
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        state.interceptor.set_enabled(enabled);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (state, enabled);
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
pub async fn set_music_native_keys_suppressed(
    state: State<'_, Arc<InterceptorState>>,
    suppressed: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if suppressed {
            if !state.interceptor.is_running() {
                state.interceptor.start_disabled()?;
                state
                    .music_suppression_owns_interceptor
                    .store(true, Ordering::SeqCst);
            }
            state.interceptor.set_music_native_keys_suppressed(true);
            return Ok(());
        }

        state.interceptor.set_music_native_keys_suppressed(false);
        if state
            .music_suppression_owns_interceptor
            .swap(false, Ordering::SeqCst)
        {
            state.interceptor.stop();
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (state, suppressed);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (state, suppressed);
        Err("Not supported on this platform".into())
    }
}

#[tauri::command]
pub async fn get_status(state: State<'_, Arc<InterceptorState>>) -> Result<EngineStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(EngineStatus {
            running: state.interceptor.is_running(),
            enabled: state.interceptor.is_enabled(),
            event_count: state.interceptor.event_count(),
            has_accessibility: KeyboardInterceptor::has_accessibility_permission(),
            has_screen_recording: ScreenCaptureAccess::default().preflight(),
            platform: "macos",
            interception_supported: true,
            requires_accessibility: true,
            requires_screen_recording: true,
            elevation_limited: false,
        })
    }
    #[cfg(target_os = "windows")]
    {
        Ok(EngineStatus {
            running: state.interceptor.is_running(),
            enabled: state.interceptor.is_enabled(),
            event_count: state.interceptor.event_count(),
            has_accessibility: true,
            has_screen_recording: false,
            platform: "windows",
            interception_supported: true,
            requires_accessibility: false,
            requires_screen_recording: false,
            elevation_limited: !WindowsKeyboardInterceptor::is_running_elevated(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = state;
        Ok(EngineStatus {
            running: false,
            enabled: false,
            event_count: 0,
            has_accessibility: cfg!(windows),
            has_screen_recording: false,
            platform: if cfg!(target_os = "linux") {
                "linux"
            } else {
                "unknown"
            },
            interception_supported: false,
            requires_accessibility: false,
            requires_screen_recording: false,
            elevation_limited: false,
        })
    }
}

#[tauri::command]
pub async fn check_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(KeyboardInterceptor::has_accessibility_permission())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(cfg!(windows))
    }
}

#[tauri::command]
pub async fn request_accessibility() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(KeyboardInterceptor::request_accessibility_permission())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(cfg!(windows))
    }
}

#[tauri::command]
pub async fn check_screen_recording() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(ScreenCaptureAccess::default().preflight())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub async fn request_screen_recording() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(ScreenCaptureAccess::default().request())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub async fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn()
            .map_err(|e| format!("Failed to open Accessibility settings: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Opening accessibility settings is only supported on macOS".into())
    }
}

#[tauri::command]
pub async fn open_screen_recording_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn()
            .map_err(|e| format!("Failed to open Screen Recording settings: {}", e))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Opening screen recording settings is only supported on macOS".into())
    }
}

#[tauri::command]
pub async fn get_alt_tab_overlay_state(
    state: State<'_, Arc<InterceptorState>>,
) -> Result<AltTabOverlayState, String> {
    Ok(state.alt_tab_overlay_state())
}

#[tauri::command]
pub async fn get_bracket_overlay_state(
    state: State<'_, Arc<InterceptorState>>,
) -> Result<BracketOverlayState, String> {
    Ok(state.bracket_overlay_state())
}

#[tauri::command]
pub async fn select_alt_tab_window(
    state: State<'_, Arc<InterceptorState>>,
    window_id: u32,
) -> Result<(), String> {
    state.select_alt_tab_window(window_id);
    Ok(())
}

#[tauri::command]
pub async fn set_alt_tab_hovered_window(
    state: State<'_, Arc<InterceptorState>>,
    window_id: Option<u32>,
) -> Result<(), String> {
    state.set_alt_tab_hovered_window(window_id);
    Ok(())
}

#[tauri::command]
pub async fn activate_alt_tab_window(
    state: State<'_, Arc<InterceptorState>>,
    window_id: u32,
) -> Result<(), String> {
    state.activate_alt_tab_window(window_id);
    Ok(())
}

#[tauri::command]
pub async fn get_alt_tab_thumbnail(
    state: State<'_, Arc<InterceptorState>>,
    window_id: u32,
) -> Result<Option<String>, String> {
    Ok(state.alt_tab_thumbnail_data_url(window_id))
}

#[tauri::command]
pub async fn get_alt_tab_app_icon(
    state: State<'_, Arc<InterceptorState>>,
    owner_pid: i32,
) -> Result<Option<String>, String> {
    Ok(state.alt_tab_app_icon_data_url(owner_pid))
}
