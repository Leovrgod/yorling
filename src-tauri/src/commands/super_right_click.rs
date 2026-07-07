use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::ffi::{OsString, c_void};
#[cfg(target_os = "macos")]
use std::fs::OpenOptions;
#[cfg(target_os = "macos")]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tauri::{AppHandle, State};

const FINDER_SYNC_BUNDLE_ID: &str = "com.yorling.app.FinderSync";
const FINDER_SYNC_APPEX_NAME: &str = "YorlingFinderSync.appex";
#[cfg(target_os = "macos")]
const FINDER_SYNC_PROCESS_NAME: &str = "YorlingFinderSync";
const SHARED_STATE_FILE_NAME: &str = "super-right-click.json";
#[cfg(target_os = "macos")]
const SHARED_RUNTIME_DIR_NAME: &str = "Yorling";
#[cfg(target_os = "macos")]
const FINDER_ACTION_REQUEST_FILE_NAME: &str = "finder-action-request.json";
#[cfg(target_os = "macos")]
const FINDER_ACTION_LOG_FILE_NAME: &str = "finder-sync-debug.log";
#[cfg(target_os = "macos")]
const FINDER_ACTION_POLL_INTERVAL_MS: u64 = 500;
#[cfg(target_os = "macos")]
const FINDER_ACTION_REQUEST_MAX_AGE_MS: u64 = 30_000;
#[cfg(target_os = "macos")]
const FINDER_ACTION_BACKGROUND_LAUNCH_SUPPRESS_MS: u64 = 5_000;
#[cfg(target_os = "macos")]
const SUPER_RIGHT_CLICK_RUNTIME_HEARTBEAT_INTERVAL_MS: u64 = 10_000;
#[cfg(target_os = "macos")]
const SUPER_RIGHT_CLICK_RUNTIME_LEASE_MAX_AGE_MS: u64 = 45_000;
#[cfg(target_os = "macos")]
const FINDER_SYNC_BOOTSTRAP_RETRY_DELAYS_MS: [u64; 5] = [0, 2_000, 6_000, 15_000, 30_000];
#[cfg(target_os = "macos")]
const FINDER_AUTOMATION_SETTINGS_URLS: [&str; 2] = [
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
    "x-apple.systempreferences:com.apple.PrivacySecurity.extension?Privacy_Automation",
];
#[cfg(target_os = "macos")]
const FINDER_SYNC_EXTENSION_SETTINGS_URLS: [&str; 2] = [
    "x-apple.systempreferences:com.apple.ExtensionsPreferences",
    "x-apple.systempreferences:com.apple.preferences.extensions",
];
#[cfg(target_os = "macos")]
const ACCESSIBILITY_SETTINGS_URLS: [&str; 2] = [
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
    "x-apple.systempreferences:com.apple.PrivacySecurity.extension?Privacy_Accessibility",
];
#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FLAG_MASK_SHIFT: u64 = 0x0002_0000;
#[cfg(target_os = "macos")]
const K_CG_EVENT_FLAG_MASK_COMMAND: u64 = 0x0010_0000;
#[cfg(target_os = "macos")]
const KEY_CODE_PERIOD: u16 = 47;
#[cfg(target_os = "macos")]
const SYNTHETIC_EVENT_TAG: i64 = 0x594F524C; // "YORL"
#[cfg(target_os = "macos")]
static SUPER_RIGHT_CLICK_RUNTIME_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static SHARED_STATE_WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "macos")]
static LAST_FINDER_ACTION_ACTIVITY_AT_MS: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
type CGEventRef = *mut c_void;

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventPost(tap_location: u32, event: CGEventRef);
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

#[derive(Debug, Clone, Serialize)]
pub struct SuperRightClickStatus {
    pub running: bool,
    pub enabled: bool,
    pub mode: &'static str,
    pub native_reference: &'static str,
    pub native_bundle_id: &'static str,
    pub native_installed: bool,
    pub native_enabled: bool,
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SuperRightClickSharedState {
    enabled: bool,
    updated_at: u64,
    terminal_id: Option<String>,
    runtime_active: bool,
    runtime_updated_at: u64,
}

#[cfg(target_os = "macos")]
impl Default for SuperRightClickSharedState {
    fn default() -> Self {
        Self {
            enabled: false,
            updated_at: 0,
            terminal_id: Some("terminal".into()),
            runtime_active: false,
            runtime_updated_at: 0,
        }
    }
}

#[cfg(target_os = "macos")]
impl SuperRightClickSharedState {
    fn runtime_lease_is_current(&self) -> bool {
        if !self.runtime_active || self.runtime_updated_at == 0 {
            return false;
        }

        let now = current_unix_millis();
        if self.runtime_updated_at > now.saturating_add(5_000) {
            return false;
        }

        now.saturating_sub(self.runtime_updated_at) <= SUPER_RIGHT_CLICK_RUNTIME_LEASE_MAX_AGE_MS
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Deserialize)]
struct FinderActionRequest {
    id: String,
    action: String,
    directory_path: Option<PathBuf>,
    targeted_path: Option<PathBuf>,
    #[serde(default)]
    selected_paths: Vec<PathBuf>,
    created_at: Option<u64>,
}

pub struct SuperRightClickState {
    #[cfg(target_os = "macos")]
    service: SuperRightClickService,
}

impl SuperRightClickState {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            service: SuperRightClickService::new(),
        }
    }
}

impl Default for SuperRightClickState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn start_finder_action_request_worker(app_handle: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn(async move {
            log_finder_action("main action worker started");
            loop {
                if let Err(error) = consume_pending_finder_action_request(&app_handle) {
                    log_finder_action(&format!("main action worker error: {error}"));
                }

                tokio::time::sleep(std::time::Duration::from_millis(
                    FINDER_ACTION_POLL_INTERVAL_MS,
                ))
                .await;
            }
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = app_handle;
    }
}

pub fn bootstrap_super_right_click_if_enabled() {
    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn(async move {
            for (attempt_index, delay_ms) in
                FINDER_SYNC_BOOTSTRAP_RETRY_DELAYS_MS.iter().enumerate()
            {
                if *delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                }

                let shared_state = read_shared_state();
                if !shared_state.enabled {
                    let _ = clear_super_right_click_runtime_lease();
                    log_finder_action(
                        "startup native bootstrap skipped because Super Right Click is disabled",
                    );
                    return;
                }

                if let Err(error) = mark_super_right_click_runtime_active() {
                    log_finder_action(&format!("startup runtime lease refresh failed: {error}"));
                }

                match ensure_finder_sync_extension_ready() {
                    Ok(()) => log_finder_action(&format!(
                        "startup native bootstrap attempt {}/{} completed",
                        attempt_index + 1,
                        FINDER_SYNC_BOOTSTRAP_RETRY_DELAYS_MS.len()
                    )),
                    Err(error) => {
                        log_finder_action(&format!(
                            "startup native bootstrap attempt {}/{} failed: {error}",
                            attempt_index + 1,
                            FINDER_SYNC_BOOTSTRAP_RETRY_DELAYS_MS.len()
                        ));
                    }
                }
            }
        });
    }
}

pub fn start_super_right_click_runtime_heartbeat() {
    #[cfg(target_os = "macos")]
    {
        SUPER_RIGHT_CLICK_RUNTIME_SHUTTING_DOWN.store(false, Ordering::SeqCst);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(
                    SUPER_RIGHT_CLICK_RUNTIME_HEARTBEAT_INTERVAL_MS,
                ))
                .await;

                if SUPER_RIGHT_CLICK_RUNTIME_SHUTTING_DOWN.load(Ordering::SeqCst) {
                    return;
                }

                let shared_state = read_shared_state();
                if shared_state.enabled {
                    if SUPER_RIGHT_CLICK_RUNTIME_SHUTTING_DOWN.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Err(error) = mark_super_right_click_runtime_active() {
                        log_finder_action(&format!("runtime heartbeat failed: {error}"));
                    }
                } else if shared_state.runtime_active {
                    if let Err(error) = clear_super_right_click_runtime_lease() {
                        log_finder_action(&format!("runtime lease clear failed: {error}"));
                    }
                }
            }
        });
    }
}

pub fn shutdown_super_right_click_runtime() {
    #[cfg(target_os = "macos")]
    {
        SUPER_RIGHT_CLICK_RUNTIME_SHUTTING_DOWN.store(true, Ordering::SeqCst);
        match clear_super_right_click_runtime_lease() {
            Ok(()) => log_finder_action("runtime lease cleared during app shutdown"),
            Err(error) => log_finder_action(&format!(
                "runtime lease clear during app shutdown failed: {error}"
            )),
        }
        restart_finder_sync_extension_process();
    }
}

pub fn has_recent_pending_finder_action_request() -> bool {
    #[cfg(target_os = "macos")]
    {
        pending_finder_action_request_is_recent(&finder_action_request_path())
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn should_suppress_main_window_for_finder_action_launch() -> bool {
    #[cfg(target_os = "macos")]
    {
        has_recent_pending_finder_action_request() || recent_finder_action_activity_is_current()
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(target_os = "macos")]
fn pending_finder_action_request_is_recent(path: &Path) -> bool {
    let modified_at = path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    let Some(modified_at) = modified_at else {
        return false;
    };

    if !system_time_is_within_age(modified_at, FINDER_ACTION_REQUEST_MAX_AGE_MS) {
        return false;
    }

    match std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<FinderActionRequest>(&contents).ok())
    {
        Some(request) => finder_action_request_is_current(&request, Some(modified_at)),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn finder_action_request_is_current(
    request: &FinderActionRequest,
    modified_at: Option<std::time::SystemTime>,
) -> bool {
    if let Some(created_at) = request.created_at {
        let now = current_unix_millis();
        if created_at > now.saturating_add(5_000) {
            return false;
        }
        return now.saturating_sub(created_at) <= FINDER_ACTION_REQUEST_MAX_AGE_MS;
    }

    modified_at
        .map(|modified_at| system_time_is_within_age(modified_at, FINDER_ACTION_REQUEST_MAX_AGE_MS))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn system_time_is_within_age(time: std::time::SystemTime, max_age_ms: u64) -> bool {
    time.elapsed()
        .map(|age| age <= std::time::Duration::from_millis(max_age_ms))
        .unwrap_or(false)
}

#[tauri::command]
pub async fn get_super_right_click_status(
    state: State<'_, Arc<SuperRightClickState>>,
) -> Result<SuperRightClickStatus, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(state.service.status())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        Ok(SuperRightClickStatus {
            running: false,
            enabled: false,
            mode: "unsupported",
            native_reference: "Finder Sync is macOS-only.",
            native_bundle_id: FINDER_SYNC_BUNDLE_ID,
            native_installed: false,
            native_enabled: false,
        })
    }
}

#[tauri::command]
pub async fn start_super_right_click(
    state: State<'_, Arc<SuperRightClickState>>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        state.service.set_enabled(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        Err("Super Right Click is only supported on macOS for now".into())
    }
}

#[tauri::command]
pub async fn stop_super_right_click(
    state: State<'_, Arc<SuperRightClickState>>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        state.service.set_enabled(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = state;
        Err("Super Right Click is only supported on macOS for now".into())
    }
}

#[tauri::command]
pub async fn set_super_right_click_enabled(
    state: State<'_, Arc<SuperRightClickState>>,
    enabled: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        state.service.set_enabled(enabled)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, enabled);
        Err("Super Right Click is only supported on macOS for now".into())
    }
}

#[tauri::command]
pub async fn set_super_right_click_terminal(
    state: State<'_, Arc<SuperRightClickState>>,
    terminal_id: String,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        state.service.set_terminal_id(terminal_id)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, terminal_id);
        Err("Super Right Click is only supported on macOS for now".into())
    }
}

#[tauri::command]
pub async fn open_finder_sync_extension_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        open_first_system_settings_url(
            &FINDER_SYNC_EXTENSION_SETTINGS_URLS,
            "Finder extension settings",
        )
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Finder Sync extension settings are only available on macOS".into())
    }
}

#[tauri::command]
pub async fn open_finder_automation_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        open_finder_automation_settings_from_main_app()
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("Finder automation settings are only available on macOS".into())
    }
}

#[cfg(target_os = "macos")]
struct SuperRightClickService;

#[cfg(target_os = "macos")]
impl SuperRightClickService {
    fn new() -> Self {
        Self
    }

    fn status(&self) -> SuperRightClickStatus {
        let shared_state = read_shared_state();
        let native_installed = finder_sync_appex_path()
            .map(|path| path.exists())
            .unwrap_or(false);
        let native_enabled = native_installed && finder_sync_extension_enabled();
        let runtime_ready = shared_state.runtime_lease_is_current();

        SuperRightClickStatus {
            running: shared_state.enabled && runtime_ready && native_enabled,
            enabled: shared_state.enabled,
            mode: if native_enabled {
                "finder-sync"
            } else {
                "finder-sync-unavailable"
            },
            native_reference: "Yorling only uses its Finder Sync extension for native Finder context menus.",
            native_bundle_id: FINDER_SYNC_BUNDLE_ID,
            native_installed,
            native_enabled,
        }
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        update_shared_state(|state| {
            state.enabled = enabled;
        })?;

        if enabled {
            mark_super_right_click_runtime_active()?;
            ensure_finder_sync_extension_ready()?;
        } else {
            clear_super_right_click_runtime_lease()?;
            restart_finder_sync_extension_process();
        }

        Ok(())
    }

    fn set_terminal_id(&self, terminal_id: String) -> Result<(), String> {
        let normalized = normalize_terminal_id(&terminal_id);
        update_shared_state(|state| {
            state.terminal_id = Some(normalized);
        })
    }
}

#[cfg(target_os = "macos")]
fn ensure_finder_sync_extension_ready() -> Result<(), String> {
    let appex_path = finder_sync_appex_path().ok_or_else(|| {
        "Yorling is not running from an app bundle, so Finder Sync cannot be registered."
            .to_string()
    })?;

    if !appex_path.exists() {
        return Err(format!(
            "Finder Sync extension is missing at {}",
            appex_path.display()
        ));
    }

    log_finder_action(&format!(
        "ensuring Finder Sync extension path={}",
        appex_path.display()
    ));

    if let Err(error) = run_pluginkit_command(vec![
        OsString::from("-r"),
        appex_path.as_os_str().to_os_string(),
    ]) {
        log_finder_action(&format!(
            "pluginkit remove before registration was non-fatal: {error}"
        ));
    }

    run_pluginkit_command(vec![
        OsString::from("-a"),
        appex_path.as_os_str().to_os_string(),
    ])?;
    run_pluginkit_command(vec![
        OsString::from("-e"),
        OsString::from("use"),
        OsString::from("-p"),
        OsString::from("com.apple.FinderSync"),
        OsString::from("-i"),
        OsString::from(FINDER_SYNC_BUNDLE_ID),
    ])?;

    restart_finder_sync_extension_process();

    if wait_for_finder_sync_extension_enabled() {
        Ok(())
    } else {
        Err(format!(
            "Finder Sync extension {} was registered but macOS does not report it as enabled.",
            FINDER_SYNC_BUNDLE_ID
        ))
    }
}

#[cfg(target_os = "macos")]
fn wait_for_finder_sync_extension_enabled() -> bool {
    for attempt in 0..10 {
        if finder_sync_extension_enabled() {
            return true;
        }

        if attempt < 9 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }

    false
}

#[cfg(target_os = "macos")]
fn normalize_terminal_id(terminal_id: &str) -> String {
    match terminal_id {
        "terminal" | "iterm2" | "ghostty" | "wezterm" | "alacritty" | "kitty" | "warp" => {
            terminal_id.to_string()
        }
        _ => "terminal".into(),
    }
}

#[cfg(target_os = "macos")]
fn read_shared_state() -> SuperRightClickSharedState {
    let Ok(path) = shared_state_path() else {
        return SuperRightClickSharedState::default();
    };

    std::fs::read(path)
        .ok()
        .and_then(|contents| serde_json::from_slice::<SuperRightClickSharedState>(&contents).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn update_shared_state(update: impl FnOnce(&mut SuperRightClickSharedState)) -> Result<(), String> {
    update_shared_state_with_timestamp(update, true)
}

#[cfg(target_os = "macos")]
fn update_runtime_state(
    update: impl FnOnce(&mut SuperRightClickSharedState),
) -> Result<(), String> {
    update_shared_state_with_timestamp(update, false)
}

#[cfg(target_os = "macos")]
fn update_shared_state_with_timestamp(
    update: impl FnOnce(&mut SuperRightClickSharedState),
    touch_updated_at: bool,
) -> Result<(), String> {
    let mut state = read_shared_state();
    update(&mut state);
    if touch_updated_at {
        state.updated_at = current_unix_millis();
    }
    write_shared_state(&state)
}

#[cfg(target_os = "macos")]
fn mark_super_right_click_runtime_active() -> Result<(), String> {
    let now = current_unix_millis();
    update_runtime_state(|state| {
        state.runtime_active = true;
        state.runtime_updated_at = now;
    })
}

#[cfg(target_os = "macos")]
fn clear_super_right_click_runtime_lease() -> Result<(), String> {
    update_runtime_state(|state| {
        state.runtime_active = false;
        state.runtime_updated_at = current_unix_millis();
    })
}

#[cfg(target_os = "macos")]
fn write_shared_state(state: &SuperRightClickSharedState) -> Result<(), String> {
    let path = shared_state_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!("Failed to create Super Right Click state directory: {error}")
        })?;
    }

    let contents = serde_json::to_vec_pretty(state)
        .map_err(|error| format!("Failed to encode Super Right Click state: {error}"))?;
    let sequence = SHARED_STATE_WRITE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let temporary_path = path.with_file_name(format!(
        "{SHARED_STATE_FILE_NAME}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));

    std::fs::write(&temporary_path, contents)
        .map_err(|error| format!("Failed to write Super Right Click state: {error}"))?;
    std::fs::rename(&temporary_path, &path)
        .map_err(|error| format!("Failed to save Super Right Click state: {error}"))?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn consume_pending_finder_action_request(app_handle: &AppHandle) -> Result<(), String> {
    let request_path = finder_action_request_path();
    if !request_path.exists() {
        return Ok(());
    }

    let request_modified_at = request_path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    if !pending_finder_action_request_is_recent(&request_path) {
        let _ = std::fs::remove_file(&request_path);
        log_finder_action(&format!(
            "discarded stale Finder action request before claim path={}",
            request_path.display()
        ));
        return Ok(());
    }

    let processing_path = request_path.with_extension("json.processing");
    match std::fs::rename(&request_path, &processing_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "Failed to claim Finder action request {}: {error}",
                request_path.display()
            ));
        }
    }

    let contents = std::fs::read_to_string(&processing_path).map_err(|error| {
        format!(
            "Failed to read Finder action request {}: {error}",
            processing_path.display()
        )
    });
    let _ = std::fs::remove_file(&processing_path);

    let request: FinderActionRequest = serde_json::from_str(&contents?).map_err(|error| {
        format!(
            "Failed to decode Finder action request {}: {error}",
            processing_path.display()
        )
    })?;

    if !finder_action_request_is_current(&request, request_modified_at) {
        log_finder_action(&format!(
            "discarded stale Finder action request id={} action={} created_at={:?}",
            request.id, request.action, request.created_at
        ));
        return Ok(());
    }

    record_recent_finder_action_activity();
    execute_finder_action_request(app_handle, &request)
}

#[cfg(target_os = "macos")]
fn record_recent_finder_action_activity() {
    LAST_FINDER_ACTION_ACTIVITY_AT_MS.store(current_unix_millis(), Ordering::SeqCst);
}

#[cfg(target_os = "macos")]
fn recent_finder_action_activity_is_current() -> bool {
    let recorded_at = LAST_FINDER_ACTION_ACTIVITY_AT_MS.load(Ordering::SeqCst);
    if recorded_at == 0 {
        return false;
    }

    let now = current_unix_millis();
    if recorded_at > now.saturating_add(5_000) {
        return false;
    }

    now.saturating_sub(recorded_at) <= FINDER_ACTION_BACKGROUND_LAUNCH_SUPPRESS_MS
}

#[cfg(target_os = "macos")]
fn execute_finder_action_request(
    _app_handle: &AppHandle,
    request: &FinderActionRequest,
) -> Result<(), String> {
    log_finder_action(&format!(
        "main action received id={} action={} directory={:?} targeted={:?} selected={:?} created_at={:?}",
        request.id,
        request.action,
        request.directory_path,
        request.targeted_path,
        request.selected_paths,
        request.created_at,
    ));

    match request.action.as_str() {
        "toggleHiddenFiles" => toggle_hidden_files_from_main_app(),
        "snapToGrid" => snap_to_grid_from_main_app(request),
        "moveTo" => move_to_folder_from_main_app(request),
        unknown => Err(format!("Unsupported Finder action request: {unknown}")),
    }
}

#[cfg(target_os = "macos")]
fn toggle_hidden_files_from_main_app() -> Result<(), String> {
    ensure_accessibility_permission_for_finder_shortcut()?;
    activate_finder_for_finder_shortcut()?;
    post_finder_hidden_files_shortcut()?;
    log_finder_action("toggleHiddenFiles completed via Finder keyboard shortcut");
    Ok(())
}

#[cfg(target_os = "macos")]
fn activate_finder_for_finder_shortcut() -> Result<(), String> {
    let status = Command::new("/usr/bin/open")
        .args(["-b", "com.apple.finder"])
        .status()
        .map_err(|error| format!("Failed to activate Finder before shortcut: {error}"))?;

    if !status.success() {
        return Err(format!(
            "Failed to activate Finder before shortcut; open exited with {status}"
        ));
    }

    std::thread::sleep(std::time::Duration::from_millis(120));
    Ok(())
}

#[cfg(target_os = "macos")]
fn snap_to_grid_from_main_app(request: &FinderActionRequest) -> Result<(), String> {
    let directory = finder_action_directory(request)
        .ok_or_else(|| "Snap to grid request did not include a usable directory".to_string())?;
    let directory = directory.canonicalize().unwrap_or(directory);
    let script = snap_to_grid_script_for_request(&directory);

    run_finder_automation_script(&script, true)?;
    log_finder_action(&format!(
        "snapToGrid completed folder_arrangement_directory={} selected_count={} mode=snap-to-grid rearrange=false",
        directory.display(),
        request.selected_paths.len()
    ));
    Ok(())
}

#[cfg(target_os = "macos")]
fn move_to_folder_from_main_app(request: &FinderActionRequest) -> Result<(), String> {
    let source_paths = existing_paths(&request.selected_paths);
    if source_paths.is_empty() {
        return Err("Move To request did not include any existing selected files.".into());
    }

    let Some(destination) = pick_move_to_destination_folder(finder_action_directory(request))?
    else {
        log_finder_action("moveTo cancelled");
        return Ok(());
    };
    let destination = destination.canonicalize().unwrap_or(destination);

    let outcome = move_paths_to_destination(&source_paths, &destination);
    if !outcome.moved_paths.is_empty() {
        reveal_moved_paths(&outcome.moved_paths);
    }

    if !outcome.failures.is_empty() {
        show_move_to_failures(&outcome);
    }

    log_finder_action(&format!(
        "moveTo completed destination={} moved_count={} failed_count={}",
        destination.display(),
        outcome.moved_paths.len(),
        outcome.failures.len()
    ));
    Ok(())
}

#[cfg(target_os = "macos")]
fn pick_move_to_destination_folder(
    default_directory: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    let script = choose_folder_script(default_directory.as_deref());
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|error| format!("Failed to open independent Move To folder picker: {error}"))?;

    if output.status.success() {
        let selected_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if selected_path.is_empty() {
            return Err("Move To folder picker returned an empty destination".into());
        }
        return Ok(Some(PathBuf::from(selected_path)));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let message = format!("{stderr}\n{stdout}");
    if looks_like_user_cancelled_apple_script(&message) {
        return Ok(None);
    }

    Err(format!(
        "Move To folder picker failed status={} stdout={} stderr={}",
        output.status, stdout, stderr
    ))
}

#[cfg(target_os = "macos")]
fn choose_folder_script(default_directory: Option<&Path>) -> String {
    let prompt = apple_script_escaped("移动到... / Move To...");
    let Some(directory) = default_directory.filter(|directory| directory.exists()) else {
        return format!(
            r#"set destinationFolder to choose folder with prompt "{}"
return POSIX path of destinationFolder"#,
            prompt
        );
    };

    format!(
        r#"set destinationFolder to choose folder with prompt "{}" default location (POSIX file "{}" as alias)
return POSIX path of destinationFolder"#,
        prompt,
        apple_script_escaped(&directory.to_string_lossy())
    )
}

#[cfg(target_os = "macos")]
fn run_independent_apple_script(script: &str) -> Result<String, String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("Failed to run independent AppleScript dialog: {error}"))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "Independent AppleScript dialog failed status={} stdout={} stderr={}",
        output.status, stdout, stderr
    ))
}

#[cfg(target_os = "macos")]
#[derive(Debug, Default)]
struct MoveToOutcome {
    moved_paths: Vec<PathBuf>,
    failures: Vec<String>,
    total_count: usize,
}

#[cfg(target_os = "macos")]
fn move_paths_to_destination(source_paths: &[PathBuf], destination: &Path) -> MoveToOutcome {
    let mut outcome = MoveToOutcome {
        total_count: source_paths.len(),
        ..MoveToOutcome::default()
    };

    for source in source_paths {
        let display_name = source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| source.display().to_string());

        if !source.exists() {
            outcome
                .failures
                .push(format!("{display_name}: 文件不存在 / file does not exist"));
            continue;
        }

        if source.is_dir() && is_same_or_descendant(destination, source) {
            outcome.failures.push(format!(
                "{display_name}: 不能移动到自身内部 / cannot move into itself"
            ));
            continue;
        }

        if let Some(parent) = source.parent() {
            if same_path(parent, destination) {
                outcome.failures.push(format!(
                    "{display_name}: 目标位置相同 / destination is the same"
                ));
                continue;
            }
        }

        let target = unique_move_destination_path(source, destination);
        match move_path(source, &target) {
            Ok(()) => {
                log_finder_action(&format!(
                    "moveTo moved {} -> {}",
                    source.display(),
                    target.display()
                ));
                outcome.moved_paths.push(target);
            }
            Err(error) => {
                log_finder_action(&format!(
                    "moveTo failed {} -> {}: {}",
                    source.display(),
                    target.display(),
                    error
                ));
                outcome.failures.push(format!("{display_name}: {error}"));
            }
        }
    }

    outcome
}

#[cfg(target_os = "macos")]
fn move_path(source: &Path, target: &Path) -> Result<(), String> {
    match std::fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            let output = Command::new("/bin/mv")
                .arg(source)
                .arg(target)
                .output()
                .map_err(|error| {
                    format!("Failed to run /bin/mv after rename failed ({rename_error}): {error}")
                })?;

            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                Err(format!(
                    "rename failed ({rename_error}); /bin/mv exited with {}",
                    output.status
                ))
            } else {
                Err(stderr)
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn unique_move_destination_path(source: &Path, destination: &Path) -> PathBuf {
    let preferred = source
        .file_name()
        .map(|name| destination.join(Path::new(name)))
        .unwrap_or_else(|| destination.join("Untitled"));
    if !preferred.exists() {
        return preferred;
    }

    let is_directory = source.is_dir();
    let stem = if is_directory {
        source
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into())
    } else {
        source
            .file_stem()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into())
    };
    let extension = if is_directory {
        None
    } else {
        source.extension().map(|extension| extension.to_os_string())
    };

    for index in 1..1000 {
        let mut candidate = destination.join(format!("{stem} {index}"));
        if let Some(extension) = &extension {
            candidate.set_extension(extension);
        }
        if !candidate.exists() {
            return candidate;
        }
    }

    let mut fallback = destination.join(format!("{stem}-{}", current_unix_millis()));
    if let Some(extension) = &extension {
        fallback.set_extension(extension);
    }
    fallback
}

#[cfg(target_os = "macos")]
fn is_same_or_descendant(candidate: &Path, parent: &Path) -> bool {
    let candidate = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    let parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    candidate == parent || candidate.starts_with(&parent)
}

#[cfg(target_os = "macos")]
fn reveal_moved_paths(paths: &[PathBuf]) {
    if let Some(path) = paths.first() {
        let _ = Command::new("/usr/bin/open").arg("-R").arg(path).status();
    }
}

#[cfg(target_os = "macos")]
fn show_move_to_failures(outcome: &MoveToOutcome) {
    let message = format!(
        "已移动 {}/{} 个项目。\n\n{}\n\nMoved {} of {} item(s).",
        outcome.moved_paths.len(),
        outcome.total_count,
        outcome.failures.join("\n"),
        outcome.moved_paths.len(),
        outcome.total_count
    );
    let icon = if outcome.moved_paths.is_empty() {
        "stop"
    } else {
        "caution"
    };
    let script = format!(
        r#"display dialog "{}" with title "{}" buttons {{"OK"}} default button "OK" with icon {}"#,
        apple_script_escaped(&message),
        apple_script_escaped("部分项目未能移动 / Some items could not be moved"),
        icon
    );

    if let Err(error) = run_independent_apple_script(&script) {
        log_finder_action(&format!(
            "moveTo failed to show independent failure dialog: {error}"
        ));
    }
}

#[cfg(target_os = "macos")]
fn snap_to_grid_script_for_request(directory: &Path) -> String {
    let desktop = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Desktop");
    let directory_path = directory.to_string_lossy();
    let menu_handler = snap_to_grid_menu_handler_script();

    if same_path(directory, &desktop) {
        format!(
            r#"{}
tell application id "com.apple.finder"
  activate
  set workspaceWindow to container window of desktop
  set current view of workspaceWindow to icon view
  set index of workspaceWindow to 1
  delay 0.1
end tell

my clickFinderSnapToGridSortMode()

tell application id "com.apple.finder"
  set workspaceWindow to container window of desktop
  if arrangement of icon view options of workspaceWindow is not snap to grid then
    error "Finder desktop did not switch to snap to grid"
  end if
  return "snapToGrid verified desktop arrangement=" & (arrangement of icon view options of workspaceWindow as text) & " mode=snap-to-grid rearrange=false"
end tell
"#,
            menu_handler
        )
    } else {
        format!(
            r#"{}
tell application id "com.apple.finder"
  activate
  set requestedPath to "{}"
  set targetFolder to (POSIX file requestedPath) as alias
  set targetWindow to missing value

  try
    set frontWindow to front Finder window
    set frontWindowPath to POSIX path of (target of frontWindow as alias)
    if frontWindowPath is requestedPath or frontWindowPath is requestedPath & "/" then
      set targetWindow to frontWindow
    end if
  end try

  if targetWindow is missing value then
    repeat with finderWindowRef in (get Finder windows)
      try
        set finderWindow to contents of finderWindowRef
        set windowPath to POSIX path of (target of finderWindow as alias)
        if windowPath is requestedPath or windowPath is requestedPath & "/" then
          set targetWindow to finderWindow
          exit repeat
        end if
      end try
    end repeat
  end if

  if targetWindow is missing value then
    open targetFolder
  end if

  repeat with attemptIndex from 1 to 20
    if targetWindow is not missing value then
      exit repeat
    end if

    repeat with finderWindowRef in (get Finder windows)
      try
        set finderWindow to contents of finderWindowRef
        set windowPath to POSIX path of (target of finderWindow as alias)
        if windowPath is requestedPath or windowPath is requestedPath & "/" then
          set targetWindow to finderWindow
          exit repeat
        end if
      end try
    end repeat

    delay 0.1
  end repeat

  if targetWindow is missing value then
    error "Finder did not expose an icon view window for " & requestedPath
  end if

  set index of targetWindow to 1
  delay 0.1
  set current view of targetWindow to icon view
  delay 0.1
end tell

my clickFinderSnapToGridSortMode()

tell application id "com.apple.finder"
  set requestedPath to "{}"
  set targetFolder to (POSIX file requestedPath) as alias
  set targetWindow to missing value

  try
    set frontWindow to front Finder window
    set frontWindowPath to POSIX path of (target of frontWindow as alias)
    if frontWindowPath is requestedPath or frontWindowPath is requestedPath & "/" then
      set targetWindow to frontWindow
    end if
  end try

  if targetWindow is missing value then
    repeat with finderWindowRef in (get Finder windows)
      try
        set finderWindow to contents of finderWindowRef
        set windowPath to POSIX path of (target of finderWindow as alias)
        if windowPath is requestedPath or windowPath is requestedPath & "/" then
          set targetWindow to finderWindow
          exit repeat
        end if
      end try
    end repeat
  end if

  if targetWindow is missing value then
    error "Finder did not expose an icon view window for " & requestedPath
  end if
  if arrangement of icon view options of targetWindow is not snap to grid then
    error "Finder window did not switch to snap to grid for " & requestedPath
  end if
  return "snapToGrid verified path=" & requestedPath & " arrangement=" & (arrangement of icon view options of targetWindow as text) & " mode=snap-to-grid rearrange=false"
end tell"#,
            menu_handler,
            apple_script_escaped(&directory_path),
            apple_script_escaped(&directory_path)
        )
    }
}

#[cfg(target_os = "macos")]
fn snap_to_grid_menu_handler_script() -> &'static str {
    r#"on clickFinderSnapToGridSortMode()
  tell application "System Events"
    tell process "Finder"
      if frontmost is false then
        set frontmost to true
        delay 0.1
      end if

      set finderViewMenu to missing value
      repeat with viewMenuName in {"显示", "View"}
        try
          set finderViewMenu to menu 1 of menu bar item (contents of viewMenuName) of menu bar 1
          exit repeat
        end try
      end repeat
      if finderViewMenu is missing value then
        error "Finder View menu was not found"
      end if

      set finderSortMenu to missing value
      repeat with sortMenuName in {"排序方式", "Sort By"}
        try
          set finderSortMenu to menu 1 of menu item (contents of sortMenuName) of finderViewMenu
          exit repeat
        end try
      end repeat
      if finderSortMenu is missing value then
        error "Finder Sort By menu was not found"
      end if

      set snapMenuItem to missing value
      repeat with snapMenuItemName in {"吸附到网格", "Snap to Grid"}
        try
          set snapMenuItem to menu item (contents of snapMenuItemName) of finderSortMenu
          exit repeat
        end try
      end repeat
      if snapMenuItem is missing value then
        error "Finder Snap to Grid menu item was not found"
      end if

      click snapMenuItem
      delay 0.1

      set snapMenuMark to missing value
      try
        set snapMenuMark to value of attribute "AXMenuItemMarkChar" of snapMenuItem
      end try
      if snapMenuMark is missing value or snapMenuMark is "" then
        error "Finder Snap to Grid sort mode did not become checked"
      end if
    end tell
  end tell
end clickFinderSnapToGridSortMode
"#
}

#[cfg(target_os = "macos")]
fn run_finder_automation_script(
    script: &str,
    open_settings_on_permission_error: bool,
) -> Result<(), String> {
    let result = run_finder_action_command("/usr/bin/osascript", &["-e", script]);
    if let Ok(output) = &result {
        if !output.trim().is_empty() {
            log_finder_action(&format!("Finder automation output: {}", output.trim()));
        }
    }
    if open_settings_on_permission_error {
        if let Err(error) = &result {
            if looks_like_accessibility_permission_error(error) {
                log_finder_action(&format!(
                    "Finder UI scripting permission appears blocked; opening settings: {error}"
                ));
                let _ = open_first_system_settings_url(
                    &ACCESSIBILITY_SETTINGS_URLS,
                    "Accessibility settings",
                );
            } else if looks_like_automation_permission_error(error) {
                log_finder_action(&format!(
                    "Finder automation permission appears blocked; opening settings: {error}"
                ));
                let _ = open_finder_automation_settings_from_main_app();
            }
        }
    }
    result.map(|_| ())
}

#[cfg(target_os = "macos")]
fn ensure_accessibility_permission_for_finder_shortcut() -> Result<(), String> {
    if unsafe { AXIsProcessTrusted() } != 0 {
        return Ok(());
    }

    let _ = open_first_system_settings_url(&ACCESSIBILITY_SETTINGS_URLS, "Accessibility settings");
    Err("Yorling needs Accessibility permission to toggle Finder hidden files live.".into())
}

#[cfg(target_os = "macos")]
fn post_finder_hidden_files_shortcut() -> Result<(), String> {
    let flags = K_CG_EVENT_FLAG_MASK_COMMAND | K_CG_EVENT_FLAG_MASK_SHIFT;
    post_keyboard_event(KEY_CODE_PERIOD, true, flags)?;
    std::thread::sleep(std::time::Duration::from_millis(24));
    post_keyboard_event(KEY_CODE_PERIOD, false, flags)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn post_keyboard_event(key_code: u16, key_down: bool, flags: u64) -> Result<(), String> {
    let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null(), key_code, key_down) };
    if event.is_null() {
        return Err("Failed to create Finder shortcut keyboard event".into());
    }

    unsafe {
        CGEventSetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE, key_code as i64);
        CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, SYNTHETIC_EVENT_TAG);
        CGEventSetFlags(event, flags);
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event as *const c_void);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn finder_action_directory(request: &FinderActionRequest) -> Option<PathBuf> {
    request
        .directory_path
        .as_ref()
        .or(request.targeted_path.as_ref())
        .or_else(|| request.selected_paths.first())
        .map(|path| {
            if std::fs::metadata(path)
                .map(|metadata| metadata.is_dir())
                .unwrap_or(false)
            {
                path.clone()
            } else {
                path.parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| path.clone())
            }
        })
}

#[cfg(target_os = "macos")]
fn looks_like_automation_permission_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("-1743")
        || normalized.contains("not authorized to send apple events")
        || normalized.contains("not authorised to send apple events")
        || normalized.contains("is not allowed to send keystrokes")
        || normalized.contains("not permitted to send apple events")
        || normalized.contains("privacy")
        || normalized.contains("automation")
}

#[cfg(target_os = "macos")]
fn looks_like_accessibility_permission_error(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("assistive access")
        || normalized.contains("accessibility")
        || normalized.contains("not allowed to control")
        || normalized.contains("not authorized for accessibility")
        || normalized.contains("not authorised for accessibility")
}

#[cfg(target_os = "macos")]
fn looks_like_user_cancelled_apple_script(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("-128")
        || normalized.contains("user canceled")
        || normalized.contains("user cancelled")
        || normalized.contains("用户已取消")
        || normalized.contains("使用者已取消")
}

#[cfg(target_os = "macos")]
fn open_finder_automation_settings_from_main_app() -> Result<(), String> {
    open_first_system_settings_url(
        &FINDER_AUTOMATION_SETTINGS_URLS,
        "Finder automation settings",
    )
}

#[cfg(target_os = "macos")]
fn open_first_system_settings_url(urls: &[&str], label: &str) -> Result<(), String> {
    for url in urls {
        match Command::new("/usr/bin/open").arg(url).status() {
            Ok(status) if status.success() => {
                log_finder_action(&format!("opened {label} url={url}"));
                return Ok(());
            }
            Ok(status) => {
                log_finder_action(&format!("failed to open {label} url={url} status={status}"));
            }
            Err(error) => {
                log_finder_action(&format!(
                    "failed to launch System Settings for {label} url={url}: {error}"
                ));
            }
        }
    }

    Err(format!("Failed to open {label}"))
}

#[cfg(target_os = "macos")]
fn run_finder_action_command(launch_path: &str, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new(launch_path)
        .args(arguments)
        .output()
        .map_err(|error| {
            format!("Failed to run Finder action command {launch_path} {arguments:?}: {error}")
        })?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "Finder action command failed status={} command={} args={:?} stdout={} stderr={}",
        output.status, launch_path, arguments, stdout, stderr
    ))
}

#[cfg(target_os = "macos")]
fn run_pluginkit_command(arguments: Vec<OsString>) -> Result<String, String> {
    let display_arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ");

    let output = Command::new("/usr/bin/pluginkit")
        .args(&arguments)
        .output()
        .map_err(|error| format!("Failed to run pluginkit {display_arguments}: {error}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            log_finder_action(&format!("pluginkit {display_arguments}: {stdout}"));
        }
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Err(format!(
        "pluginkit failed status={} args={} stdout={} stderr={}",
        output.status, display_arguments, stdout, stderr
    ))
}

#[cfg(target_os = "macos")]
fn restart_finder_sync_extension_process() {
    match Command::new("/usr/bin/pkill")
        .args(["-x", FINDER_SYNC_PROCESS_NAME])
        .output()
    {
        Ok(output) if output.status.success() => {
            log_finder_action("restarted Finder Sync extension process after registration");
        }
        Ok(output) if output.status.code() == Some(1) => {
            log_finder_action("Finder Sync extension process was not running during registration");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            log_finder_action(&format!(
                "Finder Sync extension process refresh exited with status={} stderr={}",
                output.status, stderr
            ));
        }
        Err(error) => {
            log_finder_action(&format!(
                "failed to refresh Finder Sync extension process after registration: {error}"
            ));
        }
    }
}

#[cfg(target_os = "macos")]
fn same_path(left: &std::path::Path, right: &std::path::Path) -> bool {
    let normalized_left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let normalized_right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    normalized_left == normalized_right
}

#[cfg(target_os = "macos")]
fn apple_script_escaped(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(target_os = "macos")]
fn existing_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths.iter().filter(|path| path.exists()).cloned().collect()
}

#[cfg(target_os = "macos")]
fn shared_runtime_dir() -> PathBuf {
    PathBuf::from("/Users")
        .join("Shared")
        .join(SHARED_RUNTIME_DIR_NAME)
}

#[cfg(target_os = "macos")]
fn finder_action_request_path() -> PathBuf {
    shared_runtime_dir().join(FINDER_ACTION_REQUEST_FILE_NAME)
}

#[cfg(target_os = "macos")]
fn log_finder_action(message: &str) {
    log::info!("[YorlingFinderAction] {message}");

    let directory = shared_runtime_dir();
    if let Err(error) = std::fs::create_dir_all(&directory) {
        log::warn!("Failed to create Finder action log directory: {error}");
        return;
    }

    let log_path = directory.join(FINDER_ACTION_LOG_FILE_NAME);
    match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(mut file) => {
            let _ = writeln!(file, "[{}] [main] {}", current_unix_millis(), message);
        }
        Err(error) => log::warn!("Failed to open Finder action log: {error}"),
    }
}

#[cfg(target_os = "macos")]
fn shared_state_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Cannot resolve the home directory".to_string())?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("Yorling")
        .join(SHARED_STATE_FILE_NAME))
}

#[cfg(target_os = "macos")]
fn current_unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn finder_sync_appex_path() -> Option<PathBuf> {
    let app_bundle = current_app_bundle_path()?;
    Some(
        app_bundle
            .join("Contents")
            .join("PlugIns")
            .join(FINDER_SYNC_APPEX_NAME),
    )
}

#[cfg(target_os = "macos")]
fn current_app_bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.ancestors()
        .find(|path| path.extension().is_some_and(|extension| extension == "app"))
        .map(|path| path.to_path_buf())
}

#[cfg(target_os = "macos")]
fn finder_sync_extension_enabled() -> bool {
    let output = Command::new("/usr/bin/pluginkit")
        .args([
            "-m",
            "-A",
            "-D",
            "-v",
            "-p",
            "com.apple.FinderSync",
            "-i",
            FINDER_SYNC_BUNDLE_ID,
        ])
        .output();

    let Ok(output) = output else {
        return false;
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().any(|line| {
        line.contains(FINDER_SYNC_BUNDLE_ID) && line.split_whitespace().any(|part| part == "+")
    })
}
