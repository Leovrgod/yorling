use std::collections::HashMap;
use std::io::ErrorKind;
#[cfg(target_os = "macos")]
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{
    Emitter, LogicalPosition, LogicalSize, Manager, Monitor, Position, Runtime, Size,
    WebviewWindow, WebviewWindowBuilder,
};
use yorling_island_core::bridge_server::BridgeServer;
use yorling_island_core::event::Decision;
use yorling_island_core::event::TerminalContext;
use yorling_island_core::plugin::{IslandPluginInfo, load_plugins};
use yorling_island_core::provider::{ProviderInfo, ProviderRegistry, build_health_report};
use yorling_island_core::providers::builtin_providers;
use yorling_island_core::session::{ChatMessage, SessionState};
use yorling_island_core::store::SessionStore;
use yorling_island_core::transcript::TranscriptPreview;
use yorling_island_core::transport::BridgeTransport;

#[cfg(target_os = "macos")]
use block2::RcBlock;
#[cfg(target_os = "macos")]
use core_graphics::display::CGDisplay;
#[cfg(target_os = "macos")]
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, msg_send, rc::Retained,
    runtime::AnyObject, runtime::Bool,
};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSEvent, NSEventMask, NSEventType, NSScreen, NSView, NSWindow,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};
#[cfg(target_os = "macos")]
use std::cell::{Cell, RefCell};
#[cfg(target_os = "macos")]
use std::ptr::NonNull;

// ─── State ───

#[cfg(not(target_os = "windows"))]
const ISLAND_INITIAL_WINDOW_WIDTH: f64 = 1440.0;
const ISLAND_WINDOW_HEIGHT: f64 = 750.0;
const FALLBACK_CLOSED_WIDTH: f64 = 266.0;
const FALLBACK_CLOSED_HEIGHT: f64 = 32.0;
const BRIDGE_TARGET_TRIPLE: &str = env!("YORLING_TARGET_TRIPLE");

#[cfg(not(target_os = "macos"))]
const TOP_BAR_ISLAND_MARGIN: f64 = 0.0;

#[cfg(all(windows, target_arch = "x86_64"))]
const EMBEDDED_WINDOWS_BRIDGE: &[u8] =
    include_bytes!("../binaries/yorling-bridge-x86_64-pc-windows-msvc.exe");

#[cfg(target_os = "macos")]
const COLLAPSED_OUTSIDE_CLICK_THRESHOLD: f64 = FALLBACK_CLOSED_HEIGHT + 4.0;
#[cfg(target_os = "macos")]
const HIT_TEST_PADDING_X: f64 = 10.0;
#[cfg(target_os = "macos")]
const HIT_TEST_PADDING_Y: f64 = 6.0;
#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;
#[cfg(target_os = "macos")]
const ISLAND_REPLAY_TAG: i64 = 0x594F524C;

#[cfg(target_os = "macos")]
static ISLAND_INTERACTION_WIDTH_BITS: AtomicU64 = AtomicU64::new(FALLBACK_CLOSED_WIDTH.to_bits());
#[cfg(target_os = "macos")]
static ISLAND_INTERACTION_HEIGHT_BITS: AtomicU64 = AtomicU64::new(FALLBACK_CLOSED_HEIGHT.to_bits());
#[cfg(target_os = "macos")]
static ISLAND_MOUSE_PASSTHROUGH: AtomicBool = AtomicBool::new(true);

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct IslandInteractionSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
impl Default for IslandInteractionSize {
    fn default() -> Self {
        Self {
            width: FALLBACK_CLOSED_WIDTH,
            height: FALLBACK_CLOSED_HEIGHT,
        }
    }
}

#[cfg(target_os = "macos")]
struct IslandPassThroughViewIvars {
    interaction_size: Cell<IslandInteractionSize>,
    passthrough: Cell<bool>,
}

#[cfg(target_os = "macos")]
objc2::define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[ivars = IslandPassThroughViewIvars]
    struct IslandPassThroughView;

    impl IslandPassThroughView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            if self.ivars().passthrough.get() {
                return std::ptr::null_mut();
            }

            if !rect_contains_point(self.interaction_rect(), point) {
                return std::ptr::null_mut();
            }

            unsafe { msg_send![super(self), hitTest: point] }
        }

        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> Bool {
            Bool::YES
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> Bool {
            Bool::NO
        }
    }

    unsafe impl NSObjectProtocol for IslandPassThroughView {}
);

#[cfg(target_os = "macos")]
impl IslandPassThroughView {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let view = Self::alloc(mtm).set_ivars(IslandPassThroughViewIvars {
            interaction_size: Cell::new(IslandInteractionSize::default()),
            passthrough: Cell::new(current_mouse_passthrough()),
        });

        unsafe { msg_send![super(view), initWithFrame: frame] }
    }

    fn set_interaction_size(&self, width: f64, height: f64) {
        self.ivars().interaction_size.set(IslandInteractionSize {
            width: width.max(0.0),
            height: height.max(0.0),
        });
    }

    fn set_passthrough(&self, passthrough: bool) {
        self.ivars().passthrough.set(passthrough);
    }

    fn interaction_rect(&self) -> NSRect {
        let bounds = self.bounds();
        let size = self.ivars().interaction_size.get();
        let padded_width = (size.width + HIT_TEST_PADDING_X * 2.0).min(bounds.size.width);
        let padded_height = (size.height + HIT_TEST_PADDING_Y * 2.0).min(bounds.size.height);
        let x = ((bounds.size.width - padded_width) / 2.0).max(0.0);
        let y = (bounds.size.height - padded_height).max(0.0);

        NSRect::new(NSPoint::new(x, y), NSSize::new(padded_width, padded_height))
    }
}

#[cfg(target_os = "macos")]
struct IslandEventMonitor {
    #[allow(dead_code)]
    monitor: Retained<AnyObject>,
    #[allow(dead_code)]
    block: RcBlock<dyn Fn(NonNull<NSEvent>)>,
}

#[cfg(target_os = "macos")]
struct IslandLocalEventMonitor {
    #[allow(dead_code)]
    monitor: Retained<AnyObject>,
    #[allow(dead_code)]
    block: RcBlock<dyn Fn(NonNull<NSEvent>) -> *mut NSEvent>,
}

#[cfg(target_os = "macos")]
thread_local! {
    static ISLAND_OUTSIDE_CLICK_MONITOR: RefCell<Option<IslandEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_OUTSIDE_CLICK_LOCAL_MONITOR: RefCell<Option<IslandLocalEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_HOVER_MONITOR: RefCell<Option<IslandEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_HOVER_LOCAL_MONITOR: RefCell<Option<IslandLocalEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_COLLAPSED_CLICK_MONITOR: RefCell<Option<IslandEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_COLLAPSED_CLICK_LOCAL_MONITOR: RefCell<Option<IslandLocalEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_TRANSPARENT_CLICK_MONITOR: RefCell<Option<IslandLocalEventMonitor>> = const { RefCell::new(None) };
    static ISLAND_TRANSPARENT_CLICK_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ISLAND_POINTER_INSIDE: Cell<bool> = const { Cell::new(false) };
}

#[cfg(target_os = "macos")]
type CGEventRef = *mut c_void;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        mouse_cursor_position: NSPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    fn CGEventPost(tap_location: u32, event: CGEventRef);
    fn CFRelease(cf: *const c_void);
}

#[cfg(target_os = "macos")]
fn rect_contains_point(rect: NSRect, point: NSPoint) -> bool {
    point.x >= rect.origin.x
        && point.x <= rect.origin.x + rect.size.width
        && point.y >= rect.origin.y
        && point.y <= rect.origin.y + rect.size.height
}

#[cfg(target_os = "macos")]
fn store_interaction_size(width: f64, height: f64) {
    ISLAND_INTERACTION_WIDTH_BITS.store(width.to_bits(), Ordering::Relaxed);
    ISLAND_INTERACTION_HEIGHT_BITS.store(height.to_bits(), Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn current_interaction_size() -> IslandInteractionSize {
    IslandInteractionSize {
        width: f64::from_bits(ISLAND_INTERACTION_WIDTH_BITS.load(Ordering::Relaxed)),
        height: f64::from_bits(ISLAND_INTERACTION_HEIGHT_BITS.load(Ordering::Relaxed)),
    }
}

#[cfg(target_os = "macos")]
fn current_mouse_passthrough() -> bool {
    ISLAND_MOUSE_PASSTHROUGH.load(Ordering::Relaxed)
}

#[cfg(target_os = "macos")]
fn store_mouse_passthrough(passthrough: bool) {
    ISLAND_MOUSE_PASSTHROUGH.store(passthrough, Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
fn is_expanded_interaction() -> bool {
    current_interaction_size().height > COLLAPSED_OUTSIDE_CLICK_THRESHOLD
}

#[cfg(target_os = "macos")]
fn cg_mouse_event_for_nsevent(event_type: NSEventType) -> Option<(u32, u32, bool)> {
    match event_type {
        NSEventType::LeftMouseDown => Some((K_CG_EVENT_LEFT_MOUSE_DOWN, 0, true)),
        NSEventType::LeftMouseUp => Some((K_CG_EVENT_LEFT_MOUSE_UP, 0, false)),
        NSEventType::RightMouseDown => Some((K_CG_EVENT_RIGHT_MOUSE_DOWN, 1, true)),
        NSEventType::RightMouseUp => Some((K_CG_EVENT_RIGHT_MOUSE_UP, 1, false)),
        NSEventType::OtherMouseDown => Some((K_CG_EVENT_OTHER_MOUSE_DOWN, 2, true)),
        NSEventType::OtherMouseUp => Some((K_CG_EVENT_OTHER_MOUSE_UP, 2, false)),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn cg_event_location_from_screen_point(screen_point: NSPoint) -> NSPoint {
    let Some(mtm) = MainThreadMarker::new() else {
        return screen_point;
    };

    let screens = NSScreen::screens(mtm);
    let global_top = screens.iter().fold(f64::NEG_INFINITY, |current, screen| {
        let frame = screen.frame();
        current.max(frame.origin.y + frame.size.height)
    });

    if global_top.is_finite() {
        NSPoint::new(screen_point.x, global_top - screen_point.y)
    } else {
        screen_point
    }
}

#[cfg(target_os = "macos")]
fn replay_island_mouse_event(event_type: NSEventType, screen_point: NSPoint) {
    let Some((mouse_type, mouse_button, _is_down)) = cg_mouse_event_for_nsevent(event_type) else {
        return;
    };

    let cg_point = cg_event_location_from_screen_point(screen_point);

    unsafe {
        let event = CGEventCreateMouseEvent(std::ptr::null(), mouse_type, cg_point, mouse_button);
        if event.is_null() {
            return;
        }

        CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, ISLAND_REPLAY_TAG);
        CGEventPost(K_CG_HID_EVENT_TAP, event);
        CFRelease(event as *const c_void);
    }
}

pub struct IslandState {
    pub session_store: Arc<SessionStore>,
    pub provider_registry: Arc<ProviderRegistry>,
    pub plugins: Arc<Vec<IslandPluginInfo>>,
    pub bridge_server: Arc<BridgeServer>,
    pub transport_endpoint: PathBuf,
    pub enabled: AtomicBool,
}

impl IslandState {
    pub fn new() -> Self {
        let session_store = Arc::new(SessionStore::new());
        let settings = load_island_settings();
        let plugin_load = load_plugins(&island_plugin_dirs());

        let mut registry = ProviderRegistry::new();
        for provider in builtin_providers() {
            registry.register(provider);
        }
        for provider in plugin_load.providers {
            registry.register(provider);
        }
        let provider_registry = Arc::new(registry);

        let bridge_server = Arc::new(BridgeServer::new(
            session_store.clone(),
            provider_registry.clone(),
        ));

        let transport_endpoint = island_transport_endpoint();

        Self {
            session_store,
            provider_registry,
            plugins: Arc::new(plugin_load.plugins),
            bridge_server,
            transport_endpoint,
            enabled: AtomicBool::new(settings.enabled),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum HookPreference {
    Installed,
    Uninstalled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IslandSettings {
    #[serde(default = "default_island_enabled")]
    enabled: bool,
    #[serde(default)]
    preferred_screen: Option<String>,
    #[serde(default)]
    provider_hooks_bootstrapped: bool,
    #[serde(default)]
    provider_hook_preferences: HashMap<String, HookPreference>,
}

impl Default for IslandSettings {
    fn default() -> Self {
        Self {
            enabled: default_island_enabled(),
            preferred_screen: None,
            provider_hooks_bootstrapped: false,
            provider_hook_preferences: HashMap::new(),
        }
    }
}

static SETTINGS_WRITE_LOCK: Mutex<()> = Mutex::new(());

fn update_island_settings<F>(update: F) -> Result<IslandSettings, String>
where
    F: FnOnce(&mut IslandSettings),
{
    let _guard = SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut settings = load_island_settings();
    update(&mut settings);
    persist_island_settings(&settings)?;
    Ok(settings)
}

fn set_provider_hook_preference(provider_id: &str, preference: HookPreference) {
    if let Err(error) = update_island_settings(|settings| {
        settings
            .provider_hook_preferences
            .insert(provider_id.to_string(), preference);
    }) {
        log::warn!(
            "Failed to persist hook preference for provider '{}': {}",
            provider_id,
            error
        );
    }
}

fn default_island_enabled() -> bool {
    true
}

fn island_data_dir() -> PathBuf {
    std::env::var_os("YORLING_DATA_DIR")
        .map(PathBuf::from)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn island_support_dir() -> PathBuf {
    island_data_dir().join("yorling")
}

#[cfg(windows)]
fn island_transport_endpoint() -> PathBuf {
    PathBuf::from(r"\\.\pipe\yorling-island")
}

#[cfg(not(windows))]
fn island_transport_endpoint() -> PathBuf {
    island_support_dir().join("island.sock")
}

fn island_settings_path() -> PathBuf {
    island_support_dir().join("island-settings.json")
}

fn island_plugin_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![island_support_dir().join("plugins")];

    if let Some(home) = dirs::home_dir() {
        dirs.push(
            home.join(".config")
                .join("yorling")
                .join("island")
                .join("plugins"),
        );
    }

    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("src-tauri").join("island-plugins"));
    }

    let mut deduped = Vec::new();
    for dir in dirs {
        if !deduped.contains(&dir) {
            deduped.push(dir);
        }
    }
    deduped
}

fn load_island_settings() -> IslandSettings {
    let path = island_settings_path();
    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return IslandSettings::default(),
        Err(error) => {
            log::warn!(
                "Failed to read island settings from {}: {}",
                path.display(),
                error
            );
            return IslandSettings::default();
        }
    };

    match serde_json::from_str::<IslandSettings>(&contents) {
        Ok(settings) => settings,
        Err(error) => {
            log::warn!(
                "Failed to parse island settings from {}: {}",
                path.display(),
                error
            );
            IslandSettings::default()
        }
    }
}

fn persist_island_settings(settings: &IslandSettings) -> Result<(), String> {
    let path = island_settings_path();

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create island settings directory {}: {}",
                parent.display(),
                error
            )
        })?;
    }

    let json = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("Failed to serialize island settings: {}", error))?;

    std::fs::write(&path, json).map_err(|error| {
        format!(
            "Failed to persist island settings to {}: {}",
            path.display(),
            error
        )
    })
}

async fn install_detected_provider_hooks(
    registry: &ProviderRegistry,
    bridge_path: &Path,
    transport_endpoint: &Path,
    preferences: &HashMap<String, HookPreference>,
) -> Vec<String> {
    let mut installed = Vec::new();

    for provider in registry.list() {
        if !provider.manages_hooks() || !provider.is_available_on_system() {
            continue;
        }

        if matches!(
            preferences.get(provider.id()),
            Some(HookPreference::Uninstalled)
        ) {
            log::info!(
                "Skipping automatic hook install for '{}' because the user previously uninstalled it",
                provider.id()
            );
            continue;
        }

        let status = provider.verify_hooks(bridge_path, transport_endpoint).await;
        match status {
            yorling_island_core::provider::HookStatus::Installed => {}
            yorling_island_core::provider::HookStatus::Broken { reason } => {
                log::warn!(
                    "Skipping automatic hook install for '{}' because the existing config is broken: {}",
                    provider.id(),
                    reason
                );
            }
            yorling_island_core::provider::HookStatus::NotInstalled
            | yorling_island_core::provider::HookStatus::Outdated => {
                match provider
                    .install_hooks(bridge_path, transport_endpoint)
                    .await
                {
                    Ok(()) => {
                        log::info!(
                            "Automatically installed hooks for detected provider '{}'",
                            provider.id()
                        );
                        installed.push(provider.id().to_string());
                    }
                    Err(error) => {
                        log::warn!(
                            "Failed to automatically install hooks for '{}': {}",
                            provider.id(),
                            error
                        );
                    }
                }
            }
        }
    }

    installed
}

pub async fn bootstrap_provider_hooks_if_needed(island_state: &Arc<IslandState>) {
    let mut settings = load_island_settings();
    if settings.provider_hooks_bootstrapped {
        return;
    }

    let bridge_path = match resolve_bridge_binary_path() {
        Ok(path) => path,
        Err(error) => {
            log::warn!("Skipping initial provider hook bootstrap: {}", error);
            return;
        }
    };

    let installed = install_detected_provider_hooks(
        island_state.provider_registry.as_ref(),
        &bridge_path,
        &island_state.transport_endpoint,
        &settings.provider_hook_preferences,
    )
    .await;

    settings.provider_hooks_bootstrapped = true;
    if let Err(error) = persist_island_settings(&settings) {
        log::warn!(
            "Failed to persist provider hook bootstrap marker: {}",
            error
        );
    }

    if !installed.is_empty() {
        log::info!(
            "Initial provider hook bootstrap completed for: {}",
            installed.join(", ")
        );
    }
}

fn bridge_binary_name() -> &'static str {
    #[cfg(windows)]
    {
        "yorling-bridge.exe"
    }

    #[cfg(not(windows))]
    {
        "yorling-bridge"
    }
}

fn bridge_sidecar_name() -> &'static str {
    #[cfg(windows)]
    {
        concat!("yorling-bridge-", env!("YORLING_TARGET_TRIPLE"), ".exe")
    }

    #[cfg(not(windows))]
    {
        concat!("yorling-bridge-", env!("YORLING_TARGET_TRIPLE"))
    }
}

fn bridge_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(path) = std::env::var_os("YORLING_BRIDGE_PATH") {
        candidates.push(PathBuf::from(path));
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            for binary_name in [bridge_binary_name(), bridge_sidecar_name()] {
                candidates.push(exe_dir.join(binary_name));
            }

            if let Some(contents_dir) = exe_dir.parent() {
                for binary_name in [bridge_binary_name(), bridge_sidecar_name()] {
                    candidates.push(contents_dir.join("Resources").join(binary_name));
                    candidates.push(contents_dir.join("resources").join(binary_name));
                    candidates.push(contents_dir.join("Helpers").join(binary_name));
                    candidates.push(contents_dir.join("helpers").join(binary_name));
                    candidates.push(contents_dir.join("MacOS").join(binary_name));
                }
            }
        }
    }

    for binary_name in [bridge_binary_name(), bridge_sidecar_name()] {
        candidates.push(island_support_dir().join("bin").join(binary_name));
    }

    if let Ok(cwd) = std::env::current_dir() {
        for binary_name in [bridge_binary_name(), bridge_sidecar_name()] {
            candidates.push(cwd.join("target").join("debug").join(binary_name));
            candidates.push(cwd.join("target").join("release").join(binary_name));
            candidates.push(cwd.join("src-tauri").join("binaries").join(binary_name));
        }
    }

    let mut deduped = Vec::new();
    for candidate in candidates {
        if !deduped.contains(&candidate) {
            deduped.push(candidate);
        }
    }

    deduped
}

fn resolve_bridge_binary_path() -> Result<PathBuf, String> {
    if let Some(candidate) = bridge_binary_candidates()
        .into_iter()
        .find(|candidate| candidate.is_file())
    {
        return Ok(candidate);
    }

    if let Some(materialized) = materialize_embedded_bridge()? {
        return Ok(materialized);
    }

    let searched = bridge_binary_candidates()
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(format!(
        "Cannot find yorling-bridge binary for target {}. Looked in: {}. Build `cargo build -p yorling-island-bridge` or set YORLING_BRIDGE_PATH.",
        BRIDGE_TARGET_TRIPLE, searched
    ))
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn materialize_embedded_bridge() -> Result<Option<PathBuf>, String> {
    let bridge_dir = island_support_dir().join("bin");
    let bridge_path = bridge_dir.join(bridge_binary_name());

    if let Some(parent) = bridge_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create embedded bridge directory {}: {}",
                parent.display(),
                error
            )
        })?;
    }

    let should_write = match std::fs::read(&bridge_path) {
        Ok(existing) => existing != EMBEDDED_WINDOWS_BRIDGE,
        Err(error) if error.kind() == ErrorKind::NotFound => true,
        Err(error) => {
            return Err(format!(
                "Failed to read embedded bridge target {}: {}",
                bridge_path.display(),
                error
            ));
        }
    };

    if should_write {
        std::fs::write(&bridge_path, EMBEDDED_WINDOWS_BRIDGE).map_err(|error| {
            format!(
                "Failed to write embedded yorling-bridge to {}: {}",
                bridge_path.display(),
                error
            )
        })?;
    }

    Ok(Some(bridge_path))
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn materialize_embedded_bridge() -> Result<Option<PathBuf>, String> {
    Ok(None)
}

// ─── Island window creation ───

pub fn create_island_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    initially_enabled: bool,
) -> Result<WebviewWindow<R>, tauri::Error> {
    let preferred_screen = detect_main_screen_info();
    let (initial_window_width, initial_window_height) =
        initial_island_window_size(preferred_screen.as_ref());

    let window =
        WebviewWindowBuilder::new(app, "island", tauri::WebviewUrl::App("island.html".into()))
            .title("")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .resizable(false)
            .visible(false)
            .accept_first_mouse(true)
            .inner_size(initial_window_width, initial_window_height)
            .build()?;

    // Platform-specific window configuration
    #[cfg(target_os = "macos")]
    {
        configure_macos_island(&window);
        position_island_window(&window, preferred_screen.as_ref());
    }

    #[cfg(target_os = "windows")]
    {
        configure_windows_island(&window);
        position_windows_island_window(
            &window,
            preferred_screen.as_ref(),
            FALLBACK_CLOSED_WIDTH,
            FALLBACK_CLOSED_HEIGHT,
        );
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    position_island_window(&window, preferred_screen.as_ref());

    if initially_enabled {
        let _ = window.show();
    }

    Ok(window)
}

fn initial_island_window_size(preferred_screen: Option<&ScreenInfoDto>) -> (f64, f64) {
    #[cfg(target_os = "windows")]
    {
        let closed_width = preferred_screen
            .map(|screen| screen.closed_width)
            .unwrap_or(FALLBACK_CLOSED_WIDTH);
        let closed_height = preferred_screen
            .map(|screen| screen.closed_height)
            .unwrap_or(FALLBACK_CLOSED_HEIGHT);
        return (closed_width, closed_height);
    }

    #[cfg(not(target_os = "windows"))]
    {
        let initial_window_width = preferred_screen
            .as_ref()
            .map(|screen| screen.screen_width)
            .unwrap_or(ISLAND_INITIAL_WINDOW_WIDTH);
        (initial_window_width, ISLAND_WINDOW_HEIGHT)
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_island<R: Runtime>(window: &WebviewWindow<R>) {
    if let Err(error) = window.set_shadow(false) {
        log::warn!("Failed to disable Windows island shadow: {error}");
    }

    // The Windows island uses a window that tracks the visible island bounds
    // instead of the macOS full-width pass-through surface. Keep cursor events
    // enabled so the collapsed pill can be clicked directly.
    if let Err(error) = window.set_ignore_cursor_events(false) {
        log::warn!("Failed to enable Windows island cursor events: {error}");
    }
}

#[cfg(target_os = "macos")]
fn configure_macos_island<R: Runtime>(window: &WebviewWindow<R>) {
    let Some(ns_window) = super::get_ns_window(window) else {
        return;
    };

    // Set window level above main menu (NSMainMenuWindowLevel = 24, +1)
    ns_window.setLevel((25 as isize).into());

    // Join all spaces, stationary, fullscreen auxiliary
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    let style = ns_window.styleMask() | NSWindowStyleMask::NonactivatingPanel;
    ns_window.setStyleMask(style);
    ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    ns_window.setOpaque(false);
    ns_window.setHasShadow(false);
    ns_window.setMovable(false);
    ns_window.setHidesOnDeactivate(false);
    ns_window.setIgnoresMouseEvents(current_mouse_passthrough());

    install_island_hit_test_view(ns_window);
    install_island_hover_monitor(window);
    install_island_collapsed_click_monitor(window);
    install_island_transparent_click_monitor(window);
    install_island_outside_click_monitor(window);
}

#[cfg(target_os = "macos")]
fn install_island_hit_test_view(ns_window: &NSWindow) {
    let Some(content_view) = ns_window.contentView() else {
        return;
    };

    if content_view.isKindOfClass(IslandPassThroughView::class()) {
        if let Some(wrapper) = content_view.downcast_ref::<IslandPassThroughView>() {
            let size = current_interaction_size();
            wrapper.set_interaction_size(size.width, size.height);
            wrapper.set_passthrough(current_mouse_passthrough());
        }
        return;
    }

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let wrapper = IslandPassThroughView::new(mtm, content_view.frame());
    let autoresizing =
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable;

    wrapper.setAutoresizesSubviews(true);
    wrapper.setAutoresizingMask(autoresizing);
    wrapper.set_passthrough(current_mouse_passthrough());
    content_view.setFrame(wrapper.bounds());
    content_view.setAutoresizingMask(autoresizing);

    ns_window.setContentView(Some(&wrapper));
    wrapper.addSubview(&content_view);
}

#[cfg(target_os = "macos")]
fn island_interaction_hit_rect<R: Runtime>(window: &WebviewWindow<R>) -> Option<NSRect> {
    let interaction_size = current_interaction_size();
    if interaction_size.width <= 0.0 || interaction_size.height <= 0.0 {
        return None;
    }

    let ns_window = super::get_ns_window(window)?;
    if !ns_window.isVisible() {
        return None;
    }

    let frame = ns_window.frame();
    let padded_width = interaction_size.width + HIT_TEST_PADDING_X * 2.0;
    let padded_height = interaction_size.height + HIT_TEST_PADDING_Y * 2.0;

    Some(NSRect::new(
        NSPoint::new(
            frame.origin.x + (frame.size.width - padded_width) / 2.0,
            frame.origin.y + frame.size.height - padded_height,
        ),
        NSSize::new(padded_width, padded_height),
    ))
}

#[cfg(target_os = "macos")]
fn island_interaction_contains_point<R: Runtime>(
    window: &WebviewWindow<R>,
    point: NSPoint,
) -> bool {
    island_interaction_hit_rect(window)
        .map(|rect| rect_contains_point(rect, point))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn emit_island_hover_trigger<R: Runtime>(window: &WebviewWindow<R>) {
    let is_inside = island_interaction_contains_point(window, NSEvent::mouseLocation());

    ISLAND_POINTER_INSIDE.with(|pointer_inside| {
        let was_inside = pointer_inside.replace(is_inside);
        if was_inside == is_inside {
            return;
        }

        let event_name = if is_inside {
            "island-hover-trigger-enter"
        } else {
            "island-hover-trigger-leave"
        };
        let _ = window.emit(event_name, ());
    });
}

#[cfg(target_os = "macos")]
fn install_island_hover_monitor<R: Runtime>(window: &WebviewWindow<R>) {
    let global_window_handle = window.clone();
    let local_window_handle = window.clone();

    ISLAND_HOVER_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |_event: NonNull<NSEvent>| {
            emit_island_hover_trigger(&global_window_handle);
        });

        let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged,
            &block,
        ) else {
            log::warn!("Failed to install island hover monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandEventMonitor { monitor, block });
    });

    ISLAND_HOVER_LOCAL_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            emit_island_hover_trigger(&local_window_handle);
            event.as_ptr()
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::MouseMoved | NSEventMask::LeftMouseDragged,
                &block,
            )
        };

        let Some(monitor) = monitor else {
            log::warn!("Failed to install island local hover monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandLocalEventMonitor { monitor, block });
    });
}

#[cfg(target_os = "macos")]
fn emit_island_collapsed_click<R: Runtime>(window: &WebviewWindow<R>) {
    let interaction_size = current_interaction_size();
    if interaction_size.height > COLLAPSED_OUTSIDE_CLICK_THRESHOLD {
        return;
    }

    if !current_mouse_passthrough() {
        return;
    }

    if island_interaction_contains_point(window, NSEvent::mouseLocation()) {
        let _ = window.emit("island-collapsed-click", ());
    }
}

#[cfg(target_os = "macos")]
fn install_island_collapsed_click_monitor<R: Runtime>(window: &WebviewWindow<R>) {
    let global_window_handle = window.clone();
    let local_window_handle = window.clone();

    ISLAND_COLLAPSED_CLICK_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |_event: NonNull<NSEvent>| {
            emit_island_collapsed_click(&global_window_handle);
        });

        let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown,
            &block,
        ) else {
            log::warn!("Failed to install island collapsed-click monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandEventMonitor { monitor, block });
    });

    ISLAND_COLLAPSED_CLICK_LOCAL_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            emit_island_collapsed_click(&local_window_handle);
            event.as_ptr()
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDown,
                &block,
            )
        };

        let Some(monitor) = monitor else {
            log::warn!("Failed to install island local collapsed-click monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandLocalEventMonitor { monitor, block });
    });
}

#[cfg(target_os = "macos")]
fn install_island_outside_click_monitor<R: Runtime>(window: &WebviewWindow<R>) {
    let global_window_handle = window.clone();
    let local_window_handle = window.clone();

    ISLAND_OUTSIDE_CLICK_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |_event: NonNull<NSEvent>| {
            if should_emit_island_outside_click(&global_window_handle) {
                let _ = global_window_handle.emit("island-outside-click", ());
            }
        });

        let Some(monitor) = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
            NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown,
            &block,
        ) else {
            log::warn!("Failed to install island outside-click monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandEventMonitor { monitor, block });
    });

    ISLAND_OUTSIDE_CLICK_LOCAL_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            if should_emit_island_outside_click(&local_window_handle) {
                let _ = local_window_handle.emit("island-outside-click", ());
            }
            event.as_ptr()
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDown
                    | NSEventMask::RightMouseDown
                    | NSEventMask::OtherMouseDown,
                &block,
            )
        };

        let Some(monitor) = monitor else {
            log::warn!("Failed to install island local outside-click monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandLocalEventMonitor { monitor, block });
    });
}

#[cfg(target_os = "macos")]
fn install_island_transparent_click_monitor<R: Runtime>(window: &WebviewWindow<R>) {
    let window_handle = window.clone();

    ISLAND_TRANSPARENT_CLICK_MONITOR.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }

        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            let event_type = event_ref.r#type();
            let Some((_mouse_type, _mouse_button, is_down)) =
                cg_mouse_event_for_nsevent(event_type)
            else {
                return event.as_ptr();
            };

            let replay_active = ISLAND_TRANSPARENT_CLICK_ACTIVE.with(|active| active.get());

            if !replay_active && (current_mouse_passthrough() || !is_expanded_interaction()) {
                return event.as_ptr();
            }

            let Some(ns_window) = super::get_ns_window(&window_handle) else {
                return event.as_ptr();
            };

            if event_ref.windowNumber() != ns_window.windowNumber() {
                return event.as_ptr();
            }

            let Some(content_view) = ns_window.contentView() else {
                return event.as_ptr();
            };

            let Some(wrapper) = content_view.downcast_ref::<IslandPassThroughView>() else {
                return event.as_ptr();
            };

            let location = event_ref.locationInWindow();
            if replay_active && !is_down {
                let screen_point = ns_window.convertPointToScreen(location);
                replay_island_mouse_event(event_type, screen_point);
                ISLAND_TRANSPARENT_CLICK_ACTIVE.with(|active| active.set(false));
                let _ = window_handle.emit("island-outside-click", ());
                return std::ptr::null_mut();
            }

            if rect_contains_point(wrapper.interaction_rect(), location) {
                return event.as_ptr();
            }

            if is_down {
                ISLAND_TRANSPARENT_CLICK_ACTIVE.with(|active| active.set(true));
            }

            let screen_point = ns_window.convertPointToScreen(location);
            replay_island_mouse_event(event_type, screen_point);
            return std::ptr::null_mut();
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDown
                    | NSEventMask::LeftMouseUp
                    | NSEventMask::RightMouseDown
                    | NSEventMask::RightMouseUp
                    | NSEventMask::OtherMouseDown
                    | NSEventMask::OtherMouseUp,
                &block,
            )
        };

        let Some(monitor) = monitor else {
            log::warn!("Failed to install island transparent-click monitor");
            return;
        };

        *slot.borrow_mut() = Some(IslandLocalEventMonitor { monitor, block });
    });
}

#[cfg(target_os = "macos")]
fn should_emit_island_outside_click<R: Runtime>(window: &WebviewWindow<R>) -> bool {
    if ISLAND_TRANSPARENT_CLICK_ACTIVE.with(|active| active.get()) {
        return false;
    }

    let interaction_size = current_interaction_size();
    if interaction_size.height <= COLLAPSED_OUTSIDE_CLICK_THRESHOLD {
        return false;
    }

    !island_interaction_contains_point(window, NSEvent::mouseLocation())
}

fn position_island_window<R: Runtime>(
    window: &WebviewWindow<R>,
    preferred_screen: Option<&ScreenInfoDto>,
) {
    let Some(monitor) = preferred_screen
        .and_then(|screen| find_matching_monitor(window, screen))
        .or_else(|| window.current_monitor().ok().flatten())
    else {
        return;
    };

    let scale = monitor.scale_factor();
    let monitor_position = monitor.position();
    let monitor_x = monitor_position.x as f64 / scale;
    let monitor_y = monitor_position.y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;
    let target_window_width = preferred_screen
        .map(|screen| screen.screen_width)
        .unwrap_or(monitor_width);
    let _ = window.set_size(Size::Logical(LogicalSize::new(
        target_window_width,
        ISLAND_WINDOW_HEIGHT,
    )));

    let x = monitor_x;
    let y = monitor_y;
    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));
}

#[cfg(target_os = "windows")]
fn resize_windows_island_window<R: Runtime>(
    window: &WebviewWindow<R>,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let window_width = width.max(FALLBACK_CLOSED_WIDTH).ceil();
    let window_height = height.max(FALLBACK_CLOSED_HEIGHT).ceil();

    window
        .set_size(Size::Logical(LogicalSize::new(window_width, window_height)))
        .map_err(|error| error.to_string())?;

    let preferred_screen = load_island_settings().preferred_screen;
    let preferred_screen_info = detect_screen_info_for_window(window, preferred_screen.as_deref());
    position_windows_island_window(
        window,
        preferred_screen_info.as_ref(),
        window_width,
        window_height,
    );

    Ok(())
}

#[cfg(target_os = "windows")]
fn position_windows_island_window<R: Runtime>(
    window: &WebviewWindow<R>,
    preferred_screen: Option<&ScreenInfoDto>,
    window_width: f64,
    window_height: f64,
) {
    let Some(monitor) = preferred_screen
        .and_then(|screen| find_matching_monitor(window, screen))
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| {
            window
                .available_monitors()
                .ok()
                .and_then(|monitors| monitors.into_iter().next())
        })
    else {
        return;
    };

    let scale = monitor.scale_factor();
    let monitor_position = monitor.position();
    let monitor_x = monitor_position.x as f64 / scale;
    let monitor_y = monitor_position.y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;
    let x = monitor_x + ((monitor_width - window_width) / 2.0).max(0.0);
    let y = monitor_y + TOP_BAR_ISLAND_MARGIN;

    let _ = window.set_position(Position::Logical(LogicalPosition::new(x, y)));

    if let Err(error) =
        window.set_size(Size::Logical(LogicalSize::new(window_width, window_height)))
    {
        log::warn!("Failed to apply Windows island window size: {error}");
    }
}

fn find_matching_monitor<R: Runtime>(
    window: &WebviewWindow<R>,
    preferred_screen: &ScreenInfoDto,
) -> Option<Monitor> {
    let monitors = window.available_monitors().ok()?;
    let mut exact_match_index = None;
    let mut name_match_index = None;
    let mut size_match_index = None;

    for (index, monitor) in monitors.iter().enumerate() {
        let scale = monitor.scale_factor();
        let logical_width = monitor.size().width as f64 / scale;
        let logical_height = monitor.size().height as f64 / scale;
        let matches_size = (logical_width - preferred_screen.screen_width).abs() < 2.0
            && (logical_height - preferred_screen.screen_height).abs() < 2.0;
        let matches_name = monitor
            .name()
            .as_deref()
            .is_some_and(|name| name == preferred_screen.screen_name.as_str());

        if matches_name && matches_size && exact_match_index.is_none() {
            exact_match_index = Some(index);
        }

        if matches_name && name_match_index.is_none() {
            name_match_index = Some(index);
        }

        if matches_size && size_match_index.is_none() {
            size_match_index = Some(index);
        }
    }

    exact_match_index
        .or(name_match_index)
        .or(size_match_index)
        .and_then(|index| monitors.into_iter().nth(index))
}

// ─── Tauri Commands ───

#[derive(Serialize, Deserialize, Clone)]
pub struct SessionStateDto {
    pub id: String,
    pub provider_id: String,
    pub phase: String,
    pub task_title: Option<String>,
    pub provider_session_id: Option<String>,
    pub chat_messages: Vec<ChatMessage>,
    pub tools_in_flight: Vec<String>,
    pub pending_permission: Option<PendingPermissionDto>,
    pub pending_question: Option<PendingQuestionDto>,
    pub subagent_count: usize,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub terminal_context: Option<TerminalContext>,
    pub transcript_preview: Option<TranscriptPreview>,
    pub richer_snapshot: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PendingPermissionDto {
    pub request_id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub received_at: i64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PendingQuestionDto {
    pub request_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub received_at: i64,
    pub can_answer: bool,
}

impl From<&SessionState> for SessionStateDto {
    fn from(s: &SessionState) -> Self {
        Self {
            id: s.id.clone(),
            provider_id: s.provider_id.clone(),
            phase: format!("{:?}", s.phase).to_lowercase(),
            task_title: s.task_title.clone(),
            provider_session_id: s.provider_session_id.clone(),
            chat_messages: s.chat_messages.clone(),
            tools_in_flight: s.tools_in_flight.iter().map(|t| t.tool.clone()).collect(),
            pending_permission: s.pending_permission.as_ref().map(|p| PendingPermissionDto {
                request_id: p.request_id.clone(),
                tool: p.tool.clone(),
                input: p.input.clone(),
                received_at: p.received_at,
            }),
            pending_question: s.pending_question.as_ref().map(|q| PendingQuestionDto {
                request_id: q.request_id.clone(),
                question: q.question.clone(),
                options: q.options.clone(),
                received_at: q.received_at,
                can_answer: q.can_answer,
            }),
            subagent_count: s.subagents.len(),
            started_at: s.started_at,
            ended_at: s.ended_at,
            terminal_context: s.terminal.clone(),
            transcript_preview: s.transcript_preview.clone(),
            richer_snapshot: !s.chat_messages.is_empty()
                || s.transcript_preview.as_ref().is_some_and(|preview| {
                    preview.provider_session_id.is_some()
                        || !preview.chat_preview.is_empty()
                        || !preview.tool_history.is_empty()
                }),
        }
    }
}

#[tauri::command]
pub async fn get_island_sessions(
    state: tauri::State<'_, Arc<IslandState>>,
) -> Result<Vec<SessionStateDto>, String> {
    Ok(state
        .session_store
        .all_sessions()
        .iter()
        .map(SessionStateDto::from)
        .collect())
}

#[tauri::command]
pub async fn approve_permission(
    state: tauri::State<'_, Arc<IslandState>>,
    request_id: String,
    decision: String,
) -> Result<(), String> {
    let decision = match decision.as_str() {
        "allow" => Decision::Allow,
        "deny" => Decision::Deny,
        "allow_always" => Decision::AllowAlways,
        _ => return Err("Invalid decision".to_string()),
    };

    state
        .bridge_server
        .respond_permission(&request_id, decision)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn answer_question(
    state: tauri::State<'_, Arc<IslandState>>,
    request_id: String,
    answer: String,
) -> Result<(), String> {
    state
        .bridge_server
        .respond_question(&request_id, answer)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_approval_rules(
    state: tauri::State<'_, Arc<IslandState>>,
) -> Result<Vec<yorling_island_core::approval::ApprovalRule>, String> {
    Ok(state.bridge_server.approval_store().all_rules())
}

#[tauri::command]
pub async fn remove_approval_rule(
    state: tauri::State<'_, Arc<IslandState>>,
    index: usize,
) -> Result<(), String> {
    state.bridge_server.approval_store().remove_rule(index);
    Ok(())
}

#[tauri::command]
pub async fn clear_approval_rules(state: tauri::State<'_, Arc<IslandState>>) -> Result<(), String> {
    state.bridge_server.approval_store().clear_all();
    Ok(())
}

#[tauri::command]
pub async fn set_island_interaction_bounds(
    window: WebviewWindow,
    width: f64,
    height: f64,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        store_interaction_size(width, height);

        let window_handle = window.clone();
        let window_for_lookup = window.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        window_handle
            .run_on_main_thread(move || {
                let result = if let Some(ns_window) = super::get_ns_window(&window_for_lookup) {
                    if let Some(content_view) = ns_window.contentView() {
                        if let Some(wrapper) = content_view.downcast_ref::<IslandPassThroughView>()
                        {
                            wrapper.set_interaction_size(width, height);
                            Ok(())
                        } else {
                            Err("Island hit-test view is unavailable".to_string())
                        }
                    } else {
                        Err("Cannot access island content view".to_string())
                    }
                } else {
                    Err("Cannot access native window".to_string())
                };

                let _ = tx.send(result);
            })
            .map_err(|error| error.to_string())?;

        rx.await
            .map_err(|_| "Failed to update island interaction bounds".to_string())??;
    }

    #[cfg(target_os = "windows")]
    {
        resize_windows_island_window(&window, width, height)?;
    }

    Ok(())
}

#[tauri::command]
pub async fn set_island_mouse_passthrough(
    window: WebviewWindow,
    passthrough: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        store_mouse_passthrough(passthrough);

        let window_handle = window.clone();
        let window_for_lookup = window.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();

        window_handle
            .run_on_main_thread(move || {
                let result = if let Some(ns_window) = super::get_ns_window(&window_for_lookup) {
                    ns_window.setIgnoresMouseEvents(passthrough);

                    if let Some(content_view) = ns_window.contentView() {
                        if let Some(wrapper) = content_view.downcast_ref::<IslandPassThroughView>()
                        {
                            wrapper.set_passthrough(passthrough);
                            Ok(())
                        } else {
                            Err("Island hit-test view is unavailable".to_string())
                        }
                    } else {
                        Err("Cannot access island content view".to_string())
                    }
                } else {
                    Err("Cannot access native window".to_string())
                };

                let _ = tx.send(result);
            })
            .map_err(|error| error.to_string())?;

        rx.await
            .map_err(|_| "Failed to update island mouse passthrough".to_string())??;
    }

    #[cfg(target_os = "windows")]
    {
        let _ = passthrough;
        window
            .set_ignore_cursor_events(false)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn install_provider_hooks(
    state: tauri::State<'_, Arc<IslandState>>,
    provider_id: String,
) -> Result<(), String> {
    let provider = state
        .provider_registry
        .get(&provider_id)
        .ok_or_else(|| format!("Unknown provider: {}", provider_id))?;

    let bridge_path = resolve_bridge_binary_path()?;
    provider
        .install_hooks(&bridge_path, &state.transport_endpoint)
        .await
        .map_err(|e| e.to_string())?;

    set_provider_hook_preference(&provider_id, HookPreference::Installed);
    Ok(())
}

#[tauri::command]
pub async fn uninstall_provider_hooks(
    state: tauri::State<'_, Arc<IslandState>>,
    provider_id: String,
) -> Result<(), String> {
    let provider = state
        .provider_registry
        .get(&provider_id)
        .ok_or_else(|| format!("Unknown provider: {}", provider_id))?;

    provider
        .uninstall_hooks()
        .await
        .map_err(|e| e.to_string())?;

    set_provider_hook_preference(&provider_id, HookPreference::Uninstalled);
    Ok(())
}

#[tauri::command]
pub async fn get_provider_status(
    state: tauri::State<'_, Arc<IslandState>>,
) -> Result<Vec<ProviderInfo>, String> {
    let bridge_path = resolve_bridge_binary_path();
    let mut infos = Vec::new();

    for provider in state.provider_registry.list() {
        let hook_status = match &bridge_path {
            Ok(bridge_path) => {
                provider
                    .verify_hooks(bridge_path, &state.transport_endpoint)
                    .await
            }
            Err(reason) => yorling_island_core::provider::HookStatus::Broken {
                reason: reason.clone(),
            },
        };
        let config_paths = provider
            .config_paths()
            .into_iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        let health_report = build_health_report(
            provider,
            &hook_status,
            bridge_path
                .as_ref()
                .map(|path| path.as_path())
                .map_err(|error| error.as_str()),
        );
        infos.push(ProviderInfo {
            id: provider.id().to_string(),
            display_name: provider.display_name().to_string(),
            hook_status,
            supports_blocking: provider.supports_blocking_permission(),
            origin: provider.origin(),
            plugin_id: provider.plugin_id().map(str::to_string),
            manages_hooks: provider.manages_hooks(),
            config_paths,
            health_report,
            slot_count: provider.slot_count(),
        });
    }

    Ok(infos)
}

#[tauri::command]
pub async fn get_island_plugins(
    state: tauri::State<'_, Arc<IslandState>>,
) -> Result<Vec<IslandPluginInfo>, String> {
    Ok(state.plugins.as_ref().clone())
}

#[tauri::command]
pub async fn get_island_screen_info<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<ScreenInfoDto, String> {
    #[cfg(not(target_os = "macos"))]
    let app_for_lookup = app.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let screen_info = detect_main_screen_info();

        #[cfg(not(target_os = "macos"))]
        let screen_info = detect_screen_info_for_app(&app_for_lookup, None);

        let _ = tx.send(screen_info.unwrap_or_else(ScreenInfoDto::fallback));
    })
    .map_err(|error| error.to_string())?;

    rx.await
        .map_err(|_| "Failed to receive screen info from main thread".to_string())
}

#[tauri::command]
pub async fn set_island_enabled<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, Arc<IslandState>>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = load_island_settings();
    settings.enabled = enabled;
    persist_island_settings(&settings)?;
    state.enabled.store(enabled, Ordering::Relaxed);

    if let Some(window) = app.get_webview_window("island") {
        if enabled {
            window.show().map_err(|e| e.to_string())?;
        } else {
            window.hide().map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn get_island_enabled(state: tauri::State<'_, Arc<IslandState>>) -> Result<bool, String> {
    Ok(state.enabled.load(Ordering::Relaxed))
}

#[tauri::command]
pub async fn get_preferred_screen() -> Result<Option<String>, String> {
    Ok(load_island_settings().preferred_screen)
}

#[tauri::command]
pub async fn get_all_screens<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<ScreenInfoDto>, String> {
    #[cfg(not(target_os = "macos"))]
    let app_for_lookup = app.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let screens = detect_all_screens();

        #[cfg(not(target_os = "macos"))]
        let screens = detect_all_screens_for_app(&app_for_lookup);

        let _ = tx.send(screens);
    })
    .map_err(|error| error.to_string())?;

    rx.await
        .map_err(|_| "Failed to receive screen list from main thread".to_string())
}

#[tauri::command]
pub async fn set_preferred_screen<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, Arc<IslandState>>,
    screen_name: Option<String>,
) -> Result<(), String> {
    let mut settings = load_island_settings();
    settings.enabled = state.enabled.load(Ordering::Relaxed);
    settings.preferred_screen = screen_name;
    persist_island_settings(&settings)?;

    // Re-position the island window on the new screen
    if let Some(window) = app.get_webview_window("island") {
        #[cfg(not(target_os = "macos"))]
        let preferred_name = settings.preferred_screen.clone();
        #[cfg(not(target_os = "macos"))]
        let app_for_lookup = app.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.run_on_main_thread(move || {
            #[cfg(target_os = "macos")]
            let screen = detect_main_screen_info();

            #[cfg(not(target_os = "macos"))]
            let screen = detect_screen_info_for_app(&app_for_lookup, preferred_name.as_deref());

            let _ = tx.send(screen);
        })
        .map_err(|e| e.to_string())?;

        let screen_info = rx
            .await
            .map_err(|_| "Failed to get screen info".to_string())?;

        #[cfg(target_os = "windows")]
        {
            let size = window.inner_size().ok();
            let scale = window.scale_factor().unwrap_or(1.0);
            let width = size
                .as_ref()
                .map(|size| size.width as f64 / scale)
                .unwrap_or(FALLBACK_CLOSED_WIDTH);
            let height = size
                .as_ref()
                .map(|size| size.height as f64 / scale)
                .unwrap_or(FALLBACK_CLOSED_HEIGHT);
            position_windows_island_window(&window, screen_info.as_ref(), width, height);
        }

        #[cfg(not(target_os = "windows"))]
        position_island_window(&window, screen_info.as_ref());

        // Emit updated screen info to frontend
        let info = screen_info.unwrap_or_else(ScreenInfoDto::fallback);
        let _ = app.emit("island-screen-changed", &info);
    }

    Ok(())
}

// ─── Terminal jump ───

/// Check if any terminal associated with the given sessions is the frontmost app.
/// Used for smart suppression: if the user is already looking at the terminal,
/// we don't need to auto-expand the island.
#[tauri::command]
pub async fn is_terminal_frontmost(
    state: tauri::State<'_, Arc<IslandState>>,
    session_ids: Vec<String>,
) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::NSWorkspace;

        if MainThreadMarker::new().is_none() {
            return Ok(false);
        };

        let workspace = NSWorkspace::sharedWorkspace();
        let Some(frontmost) = workspace.frontmostApplication() else {
            return Ok(false);
        };

        let frontmost_bundle = frontmost
            .bundleIdentifier()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let frontmost_name = frontmost
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Known terminal bundle IDs
        let terminal_bundles = [
            "com.mitchellh.ghostty",
            "com.apple.Terminal",
            "dev.warp.Warp-Stable",
            "com.googlecode.iterm2",
            "net.kovidgoyal.kitty",
            "io.alacritty",
            "com.github.wez.wezterm",
            "com.todesktop.230313mzl4w4u92", // Cursor
            "com.microsoft.VSCode",
            "com.microsoft.VSCodeInsiders",
            "com.exafunction.windsurf",
            "dev.zed.Zed",
        ];

        // Check if frontmost app matches any session's terminal
        for sid in &session_ids {
            if let Some(session) = state.session_store.get_session(sid) {
                if let Some(ref ctx) = session.terminal {
                    // Match by bundle ID first
                    if let Some(ref bid) = ctx.terminal_bundle_id {
                        if bid == &frontmost_bundle {
                            return Ok(true);
                        }
                    }
                    // Match by app name
                    if let Some(ref app) = ctx.terminal_app {
                        if frontmost_name.contains(app) || app.contains(&frontmost_name) {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        // Also check if frontmost is any known terminal (even without session match)
        if terminal_bundles.iter().any(|b| *b == frontmost_bundle) {
            return Ok(true);
        }

        Ok(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, session_ids);
        Ok(false)
    }
}

#[tauri::command]
pub async fn jump_to_terminal(
    state: tauri::State<'_, Arc<IslandState>>,
    session_id: String,
) -> Result<(), String> {
    let session = state
        .session_store
        .get_session(&session_id)
        .ok_or_else(|| format!("Session not found: {}", session_id))?;

    log::info!(
        "[Island] jump_to_terminal: session={}, provider={}, terminal={:?}",
        session_id,
        session.provider_id,
        session.terminal
    );

    #[cfg(target_os = "macos")]
    {
        focus_terminal_macos(session.terminal.as_ref(), &session.provider_id)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = session;
        return Err("Terminal jump is only supported on macOS".into());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn focus_terminal_macos(
    terminal: Option<&TerminalContext>,
    provider_id: &str,
) -> Result<(), String> {
    let app_name = normalize_terminal_name(terminal, provider_id);
    log::info!(
        "[Island] focus_terminal_macos: resolved app_name={}",
        app_name
    );

    let script = match app_name.as_str() {
        "iTerm2" => iterm_jump_script(terminal),
        "Ghostty" => ghostty_jump_script(terminal),
        "Warp" => warp_jump_script(terminal),
        "Terminal" => terminal_app_jump_script(terminal),
        "Alacritty" => format!(r#"tell application "Alacritty" to activate"#),
        "kitty" => format!(r#"tell application "kitty" to activate"#),
        other => format!(
            r#"tell application "{}" to activate"#,
            escape_applescript(other)
        ),
    };

    match run_osascript(&script) {
        Ok(_) => {
            log::info!("[Island] AppleScript jump succeeded for {}", app_name);
            Ok(())
        }
        Err(e) => {
            log::warn!(
                "[Island] AppleScript jump failed for {}: {}. Trying open -a fallback.",
                app_name,
                e
            );
            // Fallback: use `open -a` which reliably activates apps on macOS
            let status = std::process::Command::new("open")
                .arg("-a")
                .arg(&app_name)
                .status()
                .map_err(|err| format!("open -a failed: {}", err))?;
            if status.success() {
                log::info!("[Island] open -a fallback succeeded for {}", app_name);
                Ok(())
            } else {
                Err(format!(
                    "Both AppleScript and open -a failed for {}",
                    app_name
                ))
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn normalize_terminal_name(terminal: Option<&TerminalContext>, provider_id: &str) -> String {
    // First try bundle ID from terminal context
    if let Some(bundle_id) = terminal.and_then(|ctx| ctx.terminal_bundle_id.as_deref()) {
        if bundle_id == "com.mitchellh.ghostty" {
            return "Ghostty".into();
        }
        if bundle_id.contains("warp") {
            return "Warp".into();
        }
        if bundle_id == "com.apple.Terminal" {
            return "Terminal".into();
        }
        if bundle_id.contains("iterm") {
            return "iTerm2".into();
        }
        // IDE bundle IDs
        if bundle_id == "com.todesktop.230313mzl4w4u92" {
            return "Cursor".into();
        }
        if bundle_id == "com.microsoft.VSCode" || bundle_id == "com.microsoft.VSCodeInsiders" {
            return "Visual Studio Code".into();
        }
        if bundle_id == "com.exafunction.windsurf" {
            return "Windsurf".into();
        }
        if bundle_id == "dev.zed.Zed" {
            return "Zed".into();
        }
    }

    // Try app name from terminal context (case-insensitive to handle
    // TERM_PROGRAM values like "ghostty", "vscode", "Apple_Terminal")
    if let Some(app_name) = terminal.and_then(|ctx| ctx.terminal_app.as_deref()) {
        let lower = app_name.to_lowercase();
        if lower.contains("ghostty") {
            return "Ghostty".into();
        } else if lower.contains("warp") {
            return "Warp".into();
        } else if lower.contains("iterm") {
            return "iTerm2".into();
        } else if lower.contains("kitty") {
            return "kitty".into();
        } else if lower.contains("alacritty") {
            return "Alacritty".into();
        } else if lower.contains("cursor") {
            return "Cursor".into();
        } else if lower.contains("windsurf") {
            return "Windsurf".into();
        } else if lower.contains("zed") {
            return "Zed".into();
        } else if lower.contains("vscode") || lower.contains("code") {
            return "Visual Studio Code".into();
        } else if lower.contains("terminal") || lower == "apple_terminal" {
            return "Terminal".into();
        } else {
            return app_name.to_string();
        }
    }

    // Fallback: infer app from provider_id when no terminal context
    match provider_id {
        "cursor" => "Cursor".into(),
        "copilot" => "Visual Studio Code".into(),
        "codebuddy" => "CodeBuddy".into(),
        "workbuddy" => "WorkBuddy".into(),
        "qoder" => "Qoder".into(),
        "qoderwork" => "QoderWork".into(),
        "qwen-code" => "Qwen Code".into(),
        "windsurf" => "Windsurf".into(),
        "zed" => "Zed".into(),
        // Terminal-based CLI providers default to the system terminal
        _ => "Terminal".into(),
    }
}

#[cfg(target_os = "macos")]
fn iterm_jump_script(terminal: Option<&TerminalContext>) -> String {
    let tty = terminal
        .and_then(|ctx| ctx.tty.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let cwd = terminal
        .and_then(|ctx| ctx.cwd.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let session_id = terminal
        .and_then(|ctx| ctx.terminal_session_id.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let dir_name = terminal
        .and_then(|ctx| ctx.cwd.as_deref())
        .and_then(|cwd| std::path::Path::new(cwd).file_name())
        .and_then(|name| name.to_str())
        .map(escape_applescript)
        .unwrap_or_default();

    format!(
        r#"tell application "iTerm2"
    activate
    repeat with w in windows
        repeat with t in tabs of w
            repeat with s in sessions of t
                set matched to false
                if "{session_id}" is not "" then
                    try
                        if (unique ID of s as text) is "{session_id}" then set matched to true
                    end try
                end if
                if not matched and "{tty}" is not "" then
                    try
                        if (tty of s as text) is "{tty}" then set matched to true
                    end try
                end if
                if not matched and "{cwd}" is not "" then
                    try
                        if (profile name of s as text) contains "{dir_name}" then set matched to true
                    end try
                end if
                if matched then
                    select t
                    return "matched"
                end if
            end repeat
        end repeat
    end repeat
end tell
return ""#
    )
}

#[cfg(target_os = "macos")]
fn ghostty_jump_script(terminal: Option<&TerminalContext>) -> String {
    let terminal_session_id = terminal
        .and_then(|ctx| ctx.terminal_session_id.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let working_directory = terminal
        .and_then(|ctx| ctx.cwd.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let pane_title = terminal
        .and_then(|ctx| ctx.pane_title.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();

    let resolved_id = resolve_ghostty_terminal_id(terminal);

    // If we resolved an exact Ghostty surface ID, use it as the primary match.
    let effective_session_id = resolved_id
        .as_deref()
        .map(|id| escape_applescript(id))
        .unwrap_or(terminal_session_id);

    format!(
        r#"tell application "Ghostty"
    if not (it is running) then return ""
    activate

    set targetWindow to missing value
    set targetTab to missing value
    set targetTerminal to missing value
    set cwdMatches to {{}}
    set titleMatches to {{}}
    set combinedMatches to {{}}

    repeat with aWindow in windows
        repeat with aTab in tabs of aWindow
            repeat with aTerminal in terminals of aTab
                set cwdMatched to false
                set titleMatched to false
                -- Priority 1: exact session/surface ID match (from bridge or TTY-resolved)
                if "{effective_session_id}" is not "" then
                    try
                        if (id of aTerminal as text) is "{effective_session_id}" then
                            set targetWindow to aWindow
                            set targetTab to aTab
                            set targetTerminal to aTerminal
                            exit repeat
                        end if
                    end try
                end if
                -- Priority 2: collect directory/title matches and only use them
                -- when they uniquely identify a single surface.
                if "{working_directory}" is not "" then
                    try
                        if (working directory of aTerminal as text) is "{working_directory}" then
                            set cwdMatched to true
                        end if
                    end try
                end if
                if "{pane_title}" is not "" then
                    try
                        if (name of aTerminal as text) contains "{pane_title}" then
                            set titleMatched to true
                        end if
                    end try
                end if
                if cwdMatched then copy {{aWindow, aTab, aTerminal}} to end of cwdMatches
                if titleMatched then copy {{aWindow, aTab, aTerminal}} to end of titleMatches
                if cwdMatched and titleMatched then copy {{aWindow, aTab, aTerminal}} to end of combinedMatches
            end repeat
            if targetTerminal is not missing value then exit repeat
        end repeat
        if targetTerminal is not missing value then exit repeat
    end repeat

    if targetTerminal is missing value then
        set matchTriple to missing value
        if (count of combinedMatches) is 1 then
            set matchTriple to item 1 of combinedMatches
        else if (count of cwdMatches) is 1 then
            set matchTriple to item 1 of cwdMatches
        else if (count of titleMatches) is 1 then
            set matchTriple to item 1 of titleMatches
        else
            return ""
        end if

        set targetWindow to item 1 of matchTriple
        set targetTab to item 2 of matchTriple
        set targetTerminal to item 3 of matchTriple
    end if

    if targetTerminal is missing value then return ""

    repeat 3 times
        try
            if targetWindow is not missing value then activate window targetWindow
        end try
        delay 0.05
        try
            if targetTab is not missing value then select tab targetTab
        end try
        delay 0.05
        try
            focus targetTerminal
        end try
        delay 0.08
        if "{effective_session_id}" is "" then return "matched"
        try
            if (id of focused terminal of selected tab of front window as text) is "{effective_session_id}" then return "matched"
        end try
    end repeat
end tell
return ""#
    )
}

#[cfg(target_os = "macos")]
fn terminal_app_jump_script(terminal: Option<&TerminalContext>) -> String {
    let tty = terminal
        .and_then(|ctx| ctx.tty.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let pane_title = terminal
        .and_then(|ctx| ctx.pane_title.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();

    format!(
        r#"tell application "Terminal"
    if not (it is running) then
        activate
        return ""
    end if
    activate
    repeat with aWindow in windows
        repeat with aTab in tabs of aWindow
            if "{tty}" is not "" then
                try
                    if (tty of aTab as text) is "{tty}" then
                        set selected of aTab to true
                        set frontmost of aWindow to true
                        return "matched"
                    end if
                end try
            end if
            if "{pane_title}" is not "" then
                try
                    if (custom title of aTab as text) contains "{pane_title}" then
                        set selected of aTab to true
                        set frontmost of aWindow to true
                        return "matched"
                    end if
                end try
            end if
        end repeat
    end repeat
end tell
return ""#
    )
}

#[cfg(target_os = "macos")]
fn warp_jump_script(terminal: Option<&TerminalContext>) -> String {
    // Phase 1: Try SQLite-based precision jump
    if let Some(result) = warp_sqlite_precision_jump(terminal) {
        return result;
    }

    // Phase 2: Fallback to AppleScript title matching
    let pane_title = terminal
        .and_then(|ctx| ctx.pane_title.as_deref())
        .map(escape_applescript)
        .unwrap_or_default();
    let cwd_label = terminal
        .and_then(|ctx| ctx.cwd.as_deref())
        .and_then(|cwd| std::path::Path::new(cwd).file_name())
        .and_then(|name| name.to_str())
        .map(escape_applescript)
        .unwrap_or_default();
    let session_hint = terminal
        .and_then(|ctx| ctx.terminal_session_id.as_deref())
        .map(|value| escape_applescript(&value.chars().take(8).collect::<String>()))
        .unwrap_or_default();

    format!(
        r#"tell application "Warp" to activate
tell application "System Events"
    tell process "Warp"
        if not (exists front window) then return ""
        repeat 10 times
            try
                set currentTitle to name of front window
                if "{pane_title}" is not "" and currentTitle contains "{pane_title}" then return "matched"
                if "{cwd_label}" is not "" and currentTitle contains "{cwd_label}" then return "matched"
                if "{session_hint}" is not "" and currentTitle contains "{session_hint}" then return "matched"
            end try
            keystroke "]" using {{command down, shift down}}
            delay 0.08
        end repeat
    end tell
end tell
return ""#
    )
}

/// Attempt SQLite-based precision jump for Warp.
/// Looks up the target pane UUID from Warp's internal SQLite database,
/// then cycles tabs with Cmd+Shift+] while verifying via SQLite that
/// the focused pane matches the target.
#[cfg(target_os = "macos")]
fn warp_sqlite_precision_jump(terminal: Option<&TerminalContext>) -> Option<String> {
    let db_path = warp_sqlite_db_path()?;

    // Try to find target pane UUID from: explicit warp_pane_uuid, or cwd lookup
    let target_uuid = terminal
        .and_then(|ctx| ctx.warp_pane_uuid.clone())
        .or_else(|| {
            let cwd = terminal.and_then(|ctx| ctx.cwd.as_deref())?;
            lookup_warp_pane_uuid_from_db(&db_path, cwd).ok()?
        })?;

    // Check if already focused
    let focused = current_focused_warp_pane_uuid_from_db(&db_path)
        .ok()
        .flatten();
    if focused.as_deref() == Some(target_uuid.as_str()) {
        return Some(
            r#"tell application "Warp" to activate
return "matched""#
                .to_string(),
        );
    }

    // Get tab count for cycling limit
    let tab_count = warp_tab_count_in_active_window(&db_path).unwrap_or(10);
    let max_cycles = tab_count + 2;
    let db_escaped = db_path.display().to_string().replace('"', "\\\"");
    let target_escaped = escape_applescript(&target_uuid);

    // Build AppleScript that cycles tabs and checks SQLite after each cycle
    Some(format!(
        r#"tell application "Warp" to activate
delay 0.15
set sqlitePath to "{db_escaped}"
set targetUUID to "{target_escaped}"
repeat {max_cycles} times
    try
        set focusedUUID to do shell script "sqlite3 -readonly " & quoted form of sqlitePath & " \"SELECT hex(tp.uuid) FROM app a JOIN windows w ON w.id = a.active_window_id JOIN tabs t ON t.window_id = w.id JOIN pane_nodes pn ON pn.tab_id = t.id AND pn.is_leaf = 1 JOIN terminal_panes tp ON tp.id = pn.id ORDER BY t.id LIMIT 1 OFFSET (SELECT active_tab_index FROM windows WHERE id = a.active_window_id);\""
        if focusedUUID is targetUUID then return "matched"
    end try
    tell application "System Events"
        tell process "Warp"
            keystroke "]" using {{command down, shift down}}
        end tell
    end tell
    delay 0.1
end repeat
return ""#
    ))
}

/// Default path to Warp's internal SQLite database on macOS
#[cfg(target_os = "macos")]
fn warp_sqlite_db_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let candidates = [
        home.join("Library/Group Containers/2BBY89MBSN.dev.warp/Library/Application Support/dev.warp.Warp-Stable/warp.sqlite"),
        home.join("Library/Application Support/dev.warp.Warp-Stable/warp.sqlite"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// Look up a Warp pane UUID by cwd using the commands + blocks tables.
/// Returns the hex UUID string of the most-recently-used pane matching the cwd.
#[cfg(target_os = "macos")]
fn lookup_warp_pane_uuid_from_db(
    db_path: &std::path::Path,
    cwd: &str,
) -> Result<Option<String>, String> {
    use rusqlite::Connection;

    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Warp SQLite open failed: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_millis(60))
        .map_err(|e| format!("Warp SQLite busy timeout: {e}"))?;

    // First try: direct cwd match on terminal_panes table (if it exists)
    let direct: Option<String> = conn
        .query_row(
            "SELECT hex(uuid) FROM terminal_panes WHERE cwd = ?1 OR cwd = ?2 ORDER BY id DESC LIMIT 1",
            rusqlite::params![cwd, normalize_firmlink(cwd)],
            |row| row.get(0),
        )
        .ok();
    if direct.is_some() {
        return Ok(direct);
    }

    // Fallback: cross-reference through commands + blocks tables.
    // Finds commands matching the cwd, then joins blocks whose block_id
    // matches the command's session_id pattern (precmd-{session_id}-*).
    let result: Option<String> = conn
        .query_row(
            r#"SELECT hex(b.pane_leaf_uuid)
FROM commands c
JOIN blocks b
    ON b.block_id GLOB 'precmd-' || c.session_id || '-*'
WHERE c.pwd IN (?1, ?2)
ORDER BY c.id DESC
LIMIT 1"#,
            rusqlite::params![cwd, normalize_firmlink(cwd)],
            |row| row.get(0),
        )
        .ok();

    Ok(result)
}

/// Read the currently focused pane UUID from Warp's SQLite database.
/// Follows: app → active_window → active_tab_index → tab → pane_node(leaf) → terminal_pane.
#[cfg(target_os = "macos")]
fn current_focused_warp_pane_uuid_from_db(
    db_path: &std::path::Path,
) -> Result<Option<String>, String> {
    use rusqlite::Connection;

    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Warp SQLite open failed: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_millis(60))
        .map_err(|e| format!("Warp SQLite busy timeout: {e}"))?;

    let result: Option<String> = conn
        .query_row(
            r#"SELECT hex(tp.uuid)
FROM app a
JOIN windows w ON w.id = a.active_window_id
JOIN tabs t ON t.window_id = w.id
JOIN pane_nodes pn ON pn.tab_id = t.id AND pn.is_leaf = 1
JOIN terminal_panes tp ON tp.id = pn.id
ORDER BY t.id
LIMIT 1 OFFSET (SELECT w2.active_tab_index FROM app a2 JOIN windows w2 ON w2.id = a2.active_window_id)"#,
            [],
            |row| row.get(0),
        )
        .ok();

    Ok(result)
}

/// Count tabs in the currently active Warp window, used for cycling limit.
#[cfg(target_os = "macos")]
fn warp_tab_count_in_active_window(db_path: &std::path::Path) -> Result<usize, String> {
    use rusqlite::Connection;

    let conn = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Warp SQLite open failed: {e}"))?;
    conn.busy_timeout(std::time::Duration::from_millis(60))
        .map_err(|e| format!("Warp SQLite busy timeout: {e}"))?;

    let count: usize = conn
        .query_row(
            r#"SELECT COUNT(*)
FROM app a
JOIN tabs t ON t.window_id = a.active_window_id"#,
            [],
            |row| row.get(0),
        )
        .map_err(|e| format!("Warp tab count query failed: {e}"))?;

    Ok(count)
}

/// Normalize macOS firmlink paths (/tmp ↔ /private/tmp)
#[cfg(target_os = "macos")]
fn normalize_firmlink(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("/private/tmp") {
        format!("/tmp{rest}")
    } else if let Some(rest) = path.strip_prefix("/tmp") {
        format!("/private/tmp{rest}")
    } else if let Some(rest) = path.strip_prefix("/private/var") {
        format!("/var{rest}")
    } else if let Some(rest) = path.strip_prefix("/var") {
        format!("/private/var{rest}")
    } else {
        path.to_string()
    }
}

#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Result<String, String> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("Failed to run osascript: {}", error))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(if stderr.trim().is_empty() {
            "AppleScript command failed".into()
        } else {
            stderr.trim().to_string()
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct GhosttyTermInfo {
    id: String,
    cwd: String,
    name: String,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct GhosttyProcessDescriptor {
    pid: u32,
    cwd: String,
    command_text: Option<String>,
}

#[cfg(target_os = "macos")]
fn resolve_ghostty_terminal_id(terminal: Option<&TerminalContext>) -> Option<String> {
    let terminals = list_ghostty_terminals()?;

    if let Some(agent_pid) = terminal.and_then(|ctx| ctx.pid) {
        if let Some(descriptor) = ghostty_process_descriptor_for_pid(agent_pid) {
            if let Some(id) = match_ghostty_terminal_id(&terminals, &descriptor) {
                log::info!(
                    "[Island] Ghostty resolution: matched agent pid {} to terminal {}",
                    descriptor.pid,
                    id
                );
                return Some(id);
            }
        }
    }

    if let Some(tty) = terminal.and_then(|ctx| ctx.tty.as_deref()) {
        if let Some(descriptor) = ghostty_process_descriptor_for_tty(tty) {
            if let Some(id) = match_ghostty_terminal_id(&terminals, &descriptor) {
                log::info!(
                    "[Island] Ghostty resolution: matched tty {} (pid {}) to terminal {}",
                    tty,
                    descriptor.pid,
                    id
                );
                return Some(id);
            }
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn list_ghostty_terminals() -> Option<Vec<GhosttyTermInfo>> {
    let query_script = r#"tell application "Ghostty"
    if not (it is running) then return ""
    set resultText to ""
    repeat with w in windows
        repeat with t in tabs of w
            repeat with aTerminal in terminals of t
                try
                    set termId to id of aTerminal as text
                    set termDir to working directory of aTerminal as text
                    set termName to name of aTerminal as text
                    set resultText to resultText & termId & "|||" & termDir & "|||" & termName & "
"
                end try
            end repeat
        end repeat
    end repeat
    return resultText
end tell"#;

    let script_output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(query_script)
        .output()
        .ok()?;
    let script_text = String::from_utf8_lossy(&script_output.stdout);
    let mut terminals = Vec::new();
    for line in script_text.lines() {
        let parts: Vec<&str> = line.splitn(3, "|||").collect();
        if parts.len() >= 2 {
            let term_id = parts[0].trim();
            let term_cwd = parts[1].trim();
            let term_name = if parts.len() >= 3 {
                parts[2].trim()
            } else {
                ""
            };
            if !term_id.is_empty() {
                terminals.push(GhosttyTermInfo {
                    id: term_id.to_string(),
                    cwd: term_cwd.to_string(),
                    name: term_name.to_string(),
                });
            }
        }
    }
    Some(terminals)
}

#[cfg(target_os = "macos")]
fn ghostty_process_descriptor_for_pid(pid: u32) -> Option<GhosttyProcessDescriptor> {
    Some(GhosttyProcessDescriptor {
        pid,
        cwd: working_directory_for_pid(pid)?,
        command_text: command_text_for_pid(pid),
    })
}

#[cfg(target_os = "macos")]
fn ghostty_process_descriptor_for_tty(tty: &str) -> Option<GhosttyProcessDescriptor> {
    let pid = preferred_pid_for_tty(tty)?;
    ghostty_process_descriptor_for_pid(pid)
}

#[cfg(target_os = "macos")]
fn preferred_pid_for_tty(tty: &str) -> Option<u32> {
    let tty_device = tty.strip_prefix("/dev/").unwrap_or(tty);
    let ps_output = std::process::Command::new("ps")
        .args(["-t", tty_device, "-o", "pid=,stat="])
        .output()
        .ok()?;
    let ps_text = String::from_utf8_lossy(&ps_output.stdout);

    let mut first_pid: Option<u32> = None;
    for line in ps_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let pid = parts[0].parse::<u32>().ok()?;
            if first_pid.is_none() {
                first_pid = Some(pid);
            }
            if parts[1].contains('+') {
                return Some(pid);
            }
        }
    }

    first_pid
}

#[cfg(target_os = "macos")]
fn working_directory_for_pid(pid: u32) -> Option<String> {
    let lsof_output = std::process::Command::new("lsof")
        .args(["-p", &pid.to_string(), "-Fn", "-d", "cwd"])
        .output()
        .ok()?;
    let lsof_text = String::from_utf8_lossy(&lsof_output.stdout);
    for line in lsof_text.lines() {
        if let Some(path) = line.strip_prefix('n') {
            if path.starts_with('/') {
                return Some(path.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn command_text_for_pid(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() {
        None
    } else {
        Some(command)
    }
}

#[cfg(target_os = "macos")]
fn match_ghostty_terminal_id(
    terminals: &[GhosttyTermInfo],
    descriptor: &GhosttyProcessDescriptor,
) -> Option<String> {
    let cwd_matches: Vec<&GhosttyTermInfo> = terminals
        .iter()
        .filter(|term| ghostty_paths_match(&term.cwd, &descriptor.cwd))
        .collect();

    if cwd_matches.len() == 1 {
        return Some(cwd_matches[0].id.clone());
    }

    if cwd_matches.len() > 1 {
        if let Some(command_text) = descriptor.command_text.as_deref() {
            if let Some(terminal) =
                pick_unique_ghostty_terminal_by_title(&cwd_matches, command_text)
            {
                return Some(terminal.id.clone());
            }
        }
        log::info!(
            "[Island] Ghostty resolution: pid {} had {} cwd matches for '{}'; refusing ambiguous focus",
            descriptor.pid,
            cwd_matches.len(),
            descriptor.cwd
        );
    }

    None
}

#[cfg(target_os = "macos")]
fn pick_unique_ghostty_terminal_by_title<'a>(
    matches: &'a [&'a GhosttyTermInfo],
    command_text: &str,
) -> Option<&'a GhosttyTermInfo> {
    let tokens = ghostty_command_match_tokens(command_text);
    let mut unique_match: Option<&GhosttyTermInfo> = None;

    for token in tokens {
        let token_matches: Vec<&GhosttyTermInfo> = matches
            .iter()
            .copied()
            .filter(|term| term.name.to_lowercase().contains(&token))
            .collect();
        if token_matches.len() == 1 {
            if let Some(existing) = unique_match {
                if existing.id != token_matches[0].id {
                    return None;
                }
            } else {
                unique_match = Some(token_matches[0]);
            }
        }
    }

    unique_match
}

#[cfg(target_os = "macos")]
fn ghostty_command_match_tokens(command_text: &str) -> Vec<String> {
    const COMMON_WRAPPERS: &[&str] = &[
        "bash", "bin", "bun", "env", "fish", "node", "python", "python3", "ruby", "sh", "zsh",
    ];

    let mut tokens = Vec::new();
    for raw_part in command_text.split_whitespace() {
        let part = raw_part.trim_matches(|c| c == '"' || c == '\'');
        for candidate in [part, part.rsplit('/').next().unwrap_or(part)] {
            for token in
                candidate.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            {
                let lowered = token
                    .trim_matches(|c: char| c == '-' || c == '_')
                    .to_lowercase();
                if lowered.len() < 3 || COMMON_WRAPPERS.contains(&lowered.as_str()) {
                    continue;
                }
                if !tokens.iter().any(|existing| existing == &lowered) {
                    tokens.push(lowered);
                }
            }
        }
    }
    tokens
}

#[cfg(target_os = "macos")]
fn ghostty_paths_match(left: &str, right: &str) -> bool {
    left == right
        || normalize_firmlink(left) == right
        || left == normalize_firmlink(right)
        || normalize_firmlink(left) == normalize_firmlink(right)
}

#[cfg(target_os = "macos")]
fn escape_applescript(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[derive(Serialize, Deserialize)]
pub struct ScreenInfoDto {
    pub screen_name: String,
    pub has_notch: bool,
    pub screen_width: f64,
    pub screen_height: f64,
    pub notch_rect: Option<ScreenRectDto>,
    pub scale_factor: f64,
    pub is_builtin: bool,
    pub placement_mode: String,
    pub top_inset: f64,
    pub closed_width: f64,
    pub closed_height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenRectDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ScreenInfoDto {
    fn fallback() -> Self {
        Self {
            screen_name: String::new(),
            has_notch: false,
            screen_width: 0.0,
            screen_height: 0.0,
            notch_rect: None,
            scale_factor: 1.0,
            is_builtin: false,
            placement_mode: "top_bar".to_string(),
            top_inset: 0.0,
            closed_width: FALLBACK_CLOSED_WIDTH,
            closed_height: FALLBACK_CLOSED_HEIGHT,
        }
    }
}

#[cfg(target_os = "macos")]
fn detect_main_screen_info() -> Option<ScreenInfoDto> {
    let pref = load_island_settings().preferred_screen;
    detect_preferred_screen_info(pref.as_deref())
}

#[cfg(target_os = "macos")]
fn detect_preferred_screen_info(preferred_name: Option<&str>) -> Option<ScreenInfoDto> {
    let mtm = MainThreadMarker::new()?;
    let screens = NSScreen::screens(mtm);
    let main_display_id = NSScreen::mainScreen(mtm).map(|screen| screen.CGDirectDisplayID());

    // If user has a preference, try to match by name first
    if let Some(name) = preferred_name {
        if let Some(screen) = screens
            .iter()
            .find(|s| s.localizedName().to_string() == name)
        {
            return Some(screen_info_from_screen(&screen));
        }
    }

    // Default priority: notch+builtin > notch > main > first
    screens
        .iter()
        .find(|screen| screen_has_notch(screen) && screen_display(screen).is_builtin())
        .or_else(|| screens.iter().find(|screen| screen_has_notch(screen)))
        .or_else(|| {
            main_display_id.and_then(|display_id| {
                screens
                    .iter()
                    .find(|screen| screen.CGDirectDisplayID() == display_id)
            })
        })
        .or_else(|| screens.iter().next())
        .as_deref()
        .map(screen_info_from_screen)
}

#[cfg(target_os = "macos")]
fn detect_all_screens() -> Vec<ScreenInfoDto> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    NSScreen::screens(mtm)
        .iter()
        .map(|screen| screen_info_from_screen(&screen))
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn detect_main_screen_info() -> Option<ScreenInfoDto> {
    None
}

#[cfg(not(target_os = "macos"))]
fn detect_screen_info_for_app<R: Runtime>(
    app: &tauri::AppHandle<R>,
    preferred_name: Option<&str>,
) -> Option<ScreenInfoDto> {
    let window = app
        .get_webview_window("island")
        .or_else(|| app.get_webview_window("main"))?;
    detect_screen_info_for_window(&window, preferred_name)
}

#[cfg(not(target_os = "macos"))]
fn detect_all_screens_for_app<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<ScreenInfoDto> {
    let Some(window) = app
        .get_webview_window("island")
        .or_else(|| app.get_webview_window("main"))
    else {
        return Vec::new();
    };

    window
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .map(screen_info_from_monitor)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
fn detect_screen_info_for_window<R: Runtime>(
    window: &WebviewWindow<R>,
    preferred_name: Option<&str>,
) -> Option<ScreenInfoDto> {
    if let Some(preferred_name) = preferred_name {
        if let Ok(monitors) = window.available_monitors() {
            if let Some(monitor) = monitors.iter().find(|monitor| {
                monitor
                    .name()
                    .as_deref()
                    .is_some_and(|name| name == preferred_name)
            }) {
                return Some(screen_info_from_monitor(monitor));
            }
        }
    }

    window
        .current_monitor()
        .ok()
        .flatten()
        .as_ref()
        .map(screen_info_from_monitor)
        .or_else(|| {
            window
                .available_monitors()
                .ok()
                .and_then(|monitors| monitors.into_iter().next())
                .as_ref()
                .map(screen_info_from_monitor)
        })
}

#[cfg(not(target_os = "macos"))]
fn screen_info_from_monitor(monitor: &Monitor) -> ScreenInfoDto {
    let scale_factor = monitor.scale_factor();
    let screen_width = monitor.size().width as f64 / scale_factor;
    let screen_height = monitor.size().height as f64 / scale_factor;
    let screen_name = monitor.name().cloned().unwrap_or_else(|| {
        format!(
            "Display {}x{}",
            screen_width.round() as u32,
            screen_height.round() as u32
        )
    });

    ScreenInfoDto {
        screen_name,
        has_notch: false,
        screen_width,
        screen_height,
        notch_rect: None,
        scale_factor,
        is_builtin: false,
        placement_mode: "top_bar".to_string(),
        top_inset: TOP_BAR_ISLAND_MARGIN,
        closed_width: FALLBACK_CLOSED_WIDTH,
        closed_height: FALLBACK_CLOSED_HEIGHT,
    }
}

#[cfg(target_os = "macos")]
fn compute_notch_rect(
    frame: NSRect,
    left_area: NSRect,
    right_area: NSRect,
    safe_top: f64,
) -> Option<ScreenRectDto> {
    let frame_origin_x = frame.origin.x as f64;
    let frame_max_y = (frame.origin.y + frame.size.height) as f64;
    let left_max_x = (left_area.origin.x + left_area.size.width) as f64;
    let right_min_x = right_area.origin.x as f64;
    let notch_width = (right_min_x - left_max_x).max(0.0);
    let notch_height =
        (frame_max_y - left_area.origin.y.min(right_area.origin.y) as f64).max(safe_top);

    if safe_top <= 0.5 || notch_width <= 1.0 || notch_height <= 1.0 {
        return None;
    }

    Some(ScreenRectDto {
        x: (left_max_x - frame_origin_x).max(0.0),
        y: 0.0,
        width: notch_width,
        height: notch_height,
    })
}

#[cfg(target_os = "macos")]
fn screen_info_from_screen(screen: &NSScreen) -> ScreenInfoDto {
    let frame = screen.frame();
    let visible_frame = screen.visibleFrame();
    let safe_area = screen.safeAreaInsets();
    let left_area = screen.auxiliaryTopLeftArea();
    let right_area = screen.auxiliaryTopRightArea();
    let display = screen_display(screen);
    let notch_rect = compute_notch_rect(frame, left_area, right_area, safe_area.top as f64);
    let has_notch = notch_rect.is_some();
    let screen_width = frame.size.width as f64;
    let screen_height = frame.size.height as f64;
    let top_inset = compute_top_inset(frame, visible_frame, safe_area.top as f64);
    let max_closed_width = (screen_width - 48.0).max(180.0);
    let closed_width = notch_rect
        .as_ref()
        .map(|notch| notch.width.max(FALLBACK_CLOSED_WIDTH).min(max_closed_width))
        .unwrap_or(FALLBACK_CLOSED_WIDTH.min(max_closed_width).max(180.0));
    let closed_height = notch_rect
        .as_ref()
        .map(|notch| notch.height.max(FALLBACK_CLOSED_HEIGHT))
        .unwrap_or(FALLBACK_CLOSED_HEIGHT.max(top_inset));

    ScreenInfoDto {
        screen_name: screen.localizedName().to_string(),
        has_notch,
        screen_width,
        screen_height,
        notch_rect,
        scale_factor: screen.backingScaleFactor() as f64,
        is_builtin: display.is_builtin(),
        placement_mode: if has_notch {
            "notch".to_string()
        } else {
            "top_bar".to_string()
        },
        top_inset,
        closed_width,
        closed_height,
    }
}

#[cfg(target_os = "macos")]
fn screen_display(screen: &NSScreen) -> CGDisplay {
    CGDisplay::new(screen.CGDirectDisplayID())
}

#[cfg(target_os = "macos")]
fn screen_has_notch(screen: &NSScreen) -> bool {
    let safe_top = screen.safeAreaInsets().top as f64;
    let left_area = screen.auxiliaryTopLeftArea();
    let right_area = screen.auxiliaryTopRightArea();
    safe_top > 0.5 || left_area.size.width as f64 > 1.0 || right_area.size.width as f64 > 1.0
}

#[cfg(target_os = "macos")]
fn compute_top_inset(frame: NSRect, visible_frame: NSRect, safe_top: f64) -> f64 {
    let frame_max_y = (frame.origin.y + frame.size.height) as f64;
    let visible_max_y = (visible_frame.origin.y + visible_frame.size.height) as f64;
    let reserved_top = (frame_max_y - visible_max_y).max(0.0);

    if reserved_top > 0.5 {
        reserved_top
    } else if safe_top > 0.5 {
        safe_top
    } else {
        24.0
    }
}

// ─── Event forwarding ───

pub fn setup_event_forwarding<R: Runtime>(app: &tauri::AppHandle<R>, store: &Arc<SessionStore>) {
    let mut rx = store.subscribe();
    let app = app.clone();
    let store = store.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Convert event to a DTO for the frontend
            let _ = app.emit("island-event", &event);

            if let Some(session) = store.get_session(&event.session_id) {
                let _ = app.emit("island-session-sync", SessionStateDto::from(&session));
            } else {
                let _ = app.emit("island-session-remove", &event.session_id);
            }
        }
    });
}

/// Start the bridge server transport in the background.
pub fn start_bridge_server(island_state: &Arc<IslandState>) {
    let (transport, rx) = BridgeTransport::new(island_state.transport_endpoint.clone());
    let bridge_server = island_state.bridge_server.clone();

    // Start transport listener
    tauri::async_runtime::spawn(async move {
        if let Err(e) = transport.start().await {
            log::error!("Island bridge transport failed: {}", e);
        }
    });

    // Start bridge server message processor
    tauri::async_runtime::spawn(async move {
        bridge_server.run(rx).await;
    });

    // Start session cleanup task — remove ended sessions after 5 minutes
    let cleanup_store = island_state.session_store.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            let cutoff = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
                - 300_000;
            cleanup_store.cleanup_ended_before(cutoff);
        }
    });

    // Start stale reply cleanup task — detect expired bridge connections quickly
    // so a request answered or cancelled outside Yorling does not linger for long.
    let stale_bridge = island_state.bridge_server.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            stale_bridge.cleanup_stale_replies();
        }
    });

    // Keep transcript-derived state fresh so direct terminal interactions are reflected
    // even when a provider does not emit a follow-up hook event.
    let transcript_bridge = island_state.bridge_server.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            transcript_bridge.refresh_sessions_from_transcripts().await;
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }
    });

    // Start hook auto-repair task — verify and re-install every 5 minutes
    let repair_registry = island_state.provider_registry.clone();
    let repair_endpoint = island_state.transport_endpoint.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let bridge_path = match resolve_bridge_binary_path() {
                Ok(path) => path,
                Err(error) => {
                    log::warn!("Skipping island hook auto-repair: {}", error);
                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                    continue;
                }
            };
            // Re-read preferences each cycle so user changes take effect without restart.
            let preferences = load_island_settings().provider_hook_preferences;
            for provider in repair_registry.list() {
                if matches!(
                    preferences.get(provider.id()),
                    Some(HookPreference::Uninstalled)
                ) {
                    // User explicitly uninstalled this provider — never auto-reinstall.
                    continue;
                }
                let status = provider.verify_hooks(&bridge_path, &repair_endpoint).await;
                match status {
                    yorling_island_core::provider::HookStatus::NotInstalled
                    | yorling_island_core::provider::HookStatus::Outdated => {
                        let has_existing_config =
                            provider.config_paths().iter().any(|path| path.exists());
                        let user_requested_install = matches!(
                            preferences.get(provider.id()),
                            Some(HookPreference::Installed)
                        );
                        if matches!(
                            status,
                            yorling_island_core::provider::HookStatus::NotInstalled
                        ) && !has_existing_config
                            && !user_requested_install
                        {
                            continue;
                        }
                        log::info!(
                            "Auto-repairing hooks for provider '{}' (status: {:?})",
                            provider.id(),
                            status,
                        );
                        if let Err(e) = provider.install_hooks(&bridge_path, &repair_endpoint).await
                        {
                            log::warn!(
                                "Failed to auto-repair hooks for '{}': {}",
                                provider.id(),
                                e,
                            );
                        }
                    }
                    yorling_island_core::provider::HookStatus::Broken { reason } => {
                        log::warn!(
                            "Skipping automatic repair for provider '{}' because its hook config is broken: {}",
                            provider.id(),
                            reason,
                        );
                    }
                    _ => {}
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
        }
    });
}

// ─── Fullscreen detection ───

pub fn start_fullscreen_monitor<R: Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut was_hidden = false;
        let mut latch_count: u32 = 0;
        loop {
            // Poll every 1.5s for responsive fullscreen transitions
            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

            let is_fullscreen = is_fullscreen_app_active();

            if is_fullscreen {
                // Entering fullscreen: latch immediately
                latch_count = 0;
                if !was_hidden {
                    was_hidden = true;
                    // On notch screens, enable edge-reveal mode instead of full hide
                    let has_notch = detect_main_screen_info()
                        .as_ref()
                        .is_some_and(|info| info.has_notch);
                    let mode = if has_notch { "edge" } else { "hidden" };
                    let _ = app.emit("island-visibility", mode);
                }
            } else if was_hidden {
                // Exiting fullscreen: require 2 consecutive non-fullscreen polls
                // to avoid flickering during space-switch animations
                latch_count += 1;
                if latch_count >= 2 {
                    was_hidden = false;
                    let _ = app.emit("island-visibility", "visible");
                }
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn is_fullscreen_app_active() -> bool {
    use objc2_app_kit::NSWorkspace;

    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };

    // Strategy: Check if menu bar is hidden on the main screen.
    // In fullscreen mode, the menu bar disappears, so visibleFrame matches frame height.
    // This works without Screen Recording permission.
    let Some(main_screen) = NSScreen::mainScreen(mtm) else {
        return false;
    };
    let frame = main_screen.frame();
    let visible = main_screen.visibleFrame();
    let frame_max_y = frame.origin.y as f64 + frame.size.height as f64;
    let visible_max_y = visible.origin.y as f64 + visible.size.height as f64;
    let menu_bar_gap = frame_max_y - visible_max_y;

    // If menu bar is visible (gap >= 1 pixel), no app is fullscreen on main display
    if menu_bar_gap >= 1.0 {
        return false;
    }

    // Menu bar is hidden — check that the frontmost app is NOT Yorling itself
    let workspace = NSWorkspace::sharedWorkspace();
    if let Some(frontmost) = workspace.frontmostApplication() {
        let bundle_id = frontmost.bundleIdentifier();
        if let Some(bid) = bundle_id {
            let bid_str = bid.to_string();
            if bid_str.contains("yorling") {
                return false;
            }
        }
    }

    true
}

#[cfg(not(target_os = "macos"))]
fn is_fullscreen_app_active() -> bool {
    false
}

// ─── Screen change monitor ───

pub fn start_screen_change_monitor<R: Runtime>(app: &tauri::AppHandle<R>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_count = screen_count();
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

            let current_count = screen_count();
            if current_count != last_count {
                last_count = current_count;

                // Re-detect and re-position
                let app_clone = app.clone();
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = app.run_on_main_thread(move || {
                    let screen = detect_main_screen_info();
                    let _ = tx.send(screen);
                });

                if let Ok(screen_info) = rx.await {
                    if let Some(window) = app_clone.get_webview_window("island") {
                        position_island_window(&window, screen_info.as_ref());
                    }
                    let info = screen_info.unwrap_or_else(ScreenInfoDto::fallback);
                    let _ = app_clone.emit("island-screen-changed", &info);
                }
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn screen_count() -> usize {
    MainThreadMarker::new()
        .map(|mtm| NSScreen::screens(mtm).len())
        .unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
fn screen_count() -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::fs;
    use std::sync::{Arc as StdArc, Mutex};

    #[cfg(target_os = "macos")]
    use rusqlite::Connection;
    use yorling_island_core::event::{IslandEvent, RawHookPayload};
    use yorling_island_core::provider::{AgentProvider, HookStatus};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_settings_root(test_name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("yorling-island-{test_name}-{nanos}"))
    }

    #[test]
    fn island_state_defaults_to_enabled_without_saved_settings() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = temp_settings_root("default-enabled");
        unsafe {
            std::env::set_var("YORLING_DATA_DIR", &root);
        }

        let state = IslandState::new();
        assert!(state.enabled.load(Ordering::Relaxed));

        unsafe {
            std::env::remove_var("YORLING_DATA_DIR");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn island_state_reads_saved_enabled_flag_from_settings() {
        let _guard = ENV_LOCK.lock().unwrap();
        let root = temp_settings_root("persisted-disabled");
        let settings_dir = root.join("yorling");
        fs::create_dir_all(&settings_dir).unwrap();
        fs::write(
            settings_dir.join("island-settings.json"),
            r#"{"enabled":false}"#,
        )
        .unwrap();

        unsafe {
            std::env::set_var("YORLING_DATA_DIR", &root);
        }

        let state = IslandState::new();
        assert!(!state.enabled.load(Ordering::Relaxed));

        unsafe {
            std::env::remove_var("YORLING_DATA_DIR");
        }
        let _ = fs::remove_dir_all(root);
    }

    struct TestProvider {
        id: &'static str,
        detection_path: PathBuf,
        verify_status: HookStatus,
        installs: StdArc<Mutex<usize>>,
    }

    #[async_trait]
    impl AgentProvider for TestProvider {
        fn id(&self) -> &str {
            self.id
        }

        fn display_name(&self) -> &str {
            self.id
        }

        async fn install_hooks(
            &self,
            _bridge_path: &Path,
            _transport_endpoint: &Path,
        ) -> anyhow::Result<()> {
            *self.installs.lock().unwrap() += 1;
            Ok(())
        }

        async fn uninstall_hooks(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn verify_hooks(
            &self,
            _bridge_path: &Path,
            _transport_endpoint: &Path,
        ) -> HookStatus {
            self.verify_status.clone()
        }

        fn normalize_event(&self, _raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
            anyhow::bail!("not used in test")
        }

        fn supports_blocking_permission(&self) -> bool {
            false
        }

        fn encode_permission_response(&self, _decision: Decision) -> anyhow::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        fn encode_question_response(
            &self,
            _answer: &str,
            _from_permission: bool,
            _answer_key: Option<&str>,
        ) -> anyhow::Result<Vec<u8>> {
            Ok(Vec::new())
        }

        fn detection_paths(&self) -> Vec<PathBuf> {
            vec![self.detection_path.clone()]
        }
    }

    #[test]
    fn install_detected_provider_hooks_only_installs_available_providers() {
        let root = temp_settings_root("detected-provider-install");
        fs::create_dir_all(&root).unwrap();

        let installs = StdArc::new(Mutex::new(0usize));
        let unavailable_installs = StdArc::new(Mutex::new(0usize));
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(TestProvider {
            id: "detected",
            detection_path: root.join("detected"),
            verify_status: HookStatus::NotInstalled,
            installs: installs.clone(),
        }));
        registry.register(Box::new(TestProvider {
            id: "missing",
            detection_path: root.join("missing"),
            verify_status: HookStatus::NotInstalled,
            installs: unavailable_installs.clone(),
        }));
        fs::create_dir_all(root.join("detected")).unwrap();

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let installed = runtime.block_on(async {
            install_detected_provider_hooks(
                &registry,
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
                &HashMap::new(),
            )
            .await
        });

        assert_eq!(installed, vec!["detected".to_string()]);
        assert_eq!(*installs.lock().unwrap(), 1);
        assert_eq!(*unavailable_installs.lock().unwrap(), 0);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn install_detected_provider_hooks_skips_broken_provider_configs() {
        let root = temp_settings_root("detected-provider-broken");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("broken")).unwrap();

        let installs = StdArc::new(Mutex::new(0usize));
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(TestProvider {
            id: "broken",
            detection_path: root.join("broken"),
            verify_status: HookStatus::Broken {
                reason: "invalid json".into(),
            },
            installs: installs.clone(),
        }));

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let installed = runtime.block_on(async {
            install_detected_provider_hooks(
                &registry,
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
                &HashMap::new(),
            )
            .await
        });

        assert!(installed.is_empty());
        assert_eq!(*installs.lock().unwrap(), 0);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn lookup_warp_pane_uuid_matches_latest_command_for_cwd() {
        let root = temp_settings_root("warp-lookup");
        fs::create_dir_all(&root).unwrap();
        let db_path = root.join("warp.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE commands (
                id INTEGER PRIMARY KEY,
                session_id INTEGER NOT NULL,
                command TEXT NOT NULL,
                pwd TEXT NOT NULL
            );
            CREATE TABLE blocks (
                id INTEGER PRIMARY KEY,
                pane_leaf_uuid BLOB NOT NULL,
                block_id TEXT NOT NULL,
                pwd TEXT
            );
            ",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commands (id, session_id, command, pwd) VALUES (1, 1001, 'claude --resume', '/tmp/demo')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO commands (id, session_id, command, pwd) VALUES (2, 1002, 'codex', '/tmp/demo')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (id, pane_leaf_uuid, block_id, pwd) VALUES (1, X'00112233445566778899AABBCCDDEEFF', 'precmd-1001-1', '/tmp/demo')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO blocks (id, pane_leaf_uuid, block_id, pwd) VALUES (2, X'FFEEDDCCBBAA99887766554433221100', 'precmd-1002-1', '/tmp/demo')",
            [],
        )
        .unwrap();

        let pane_uuid = lookup_warp_pane_uuid_from_db(&db_path, "/tmp/demo").unwrap();
        assert_eq!(
            pane_uuid.as_deref(),
            Some("FFEEDDCCBBAA99887766554433221100")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn focused_warp_pane_reads_active_tab_from_sqlite() {
        let root = temp_settings_root("warp-focused");
        fs::create_dir_all(&root).unwrap();
        let db_path = root.join("warp.sqlite");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE app (
                active_window_id INTEGER
            );
            CREATE TABLE windows (
                id INTEGER PRIMARY KEY,
                active_tab_index INTEGER NOT NULL
            );
            CREATE TABLE tabs (
                id INTEGER PRIMARY KEY,
                window_id INTEGER NOT NULL
            );
            CREATE TABLE pane_nodes (
                id INTEGER PRIMARY KEY,
                tab_id INTEGER NOT NULL,
                is_leaf INTEGER NOT NULL
            );
            CREATE TABLE terminal_panes (
                id INTEGER PRIMARY KEY,
                uuid BLOB NOT NULL,
                cwd TEXT
            );
            ",
        )
        .unwrap();
        conn.execute("INSERT INTO app (active_window_id) VALUES (7)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO windows (id, active_tab_index) VALUES (7, 1)",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO tabs (id, window_id) VALUES (10, 7)", [])
            .unwrap();
        conn.execute("INSERT INTO tabs (id, window_id) VALUES (11, 7)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO pane_nodes (id, tab_id, is_leaf) VALUES (100, 10, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO pane_nodes (id, tab_id, is_leaf) VALUES (101, 11, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO terminal_panes (id, uuid, cwd) VALUES (100, X'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA', '/tmp/a')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO terminal_panes (id, uuid, cwd) VALUES (101, X'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB', '/tmp/b')",
            [],
        )
        .unwrap();

        let focused = current_focused_warp_pane_uuid_from_db(&db_path).unwrap();
        assert_eq!(focused.as_deref(), Some("BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"));
        assert_eq!(warp_tab_count_in_active_window(&db_path).unwrap(), 2);

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_command_tokens_extract_cli_name_from_wrapped_command() {
        let tokens = ghostty_command_match_tokens(
            "node /Users/example/.local/share/codex/dist/index.js --resume session-1",
        );

        assert!(tokens.contains(&"codex".to_string()));
        assert!(tokens.contains(&"resume".to_string()));
        assert!(!tokens.contains(&"node".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_match_requires_unique_candidate_when_cwd_is_ambiguous() {
        let terminals = vec![
            GhosttyTermInfo {
                id: "term-1".into(),
                cwd: "/tmp/demo".into(),
                name: "shell".into(),
            },
            GhosttyTermInfo {
                id: "term-2".into(),
                cwd: "/private/tmp/demo".into(),
                name: "shell".into(),
            },
        ];
        let descriptor = GhosttyProcessDescriptor {
            pid: 42,
            cwd: "/tmp/demo".into(),
            command_text: Some("zsh".into()),
        };

        assert_eq!(match_ghostty_terminal_id(&terminals, &descriptor), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_match_uses_title_tokens_to_disambiguate_same_cwd() {
        let terminals = vec![
            GhosttyTermInfo {
                id: "term-agent".into(),
                cwd: "/tmp/demo".into(),
                name: "codex resume session-1".into(),
            },
            GhosttyTermInfo {
                id: "term-shell".into(),
                cwd: "/tmp/demo".into(),
                name: "npm run dev".into(),
            },
        ];
        let descriptor = GhosttyProcessDescriptor {
            pid: 7,
            cwd: "/private/tmp/demo".into(),
            command_text: Some(
                "node /Users/example/.local/share/codex/dist/index.js --resume session-1".into(),
            ),
        };

        assert_eq!(
            match_ghostty_terminal_id(&terminals, &descriptor).as_deref(),
            Some("term-agent")
        );
    }
}
