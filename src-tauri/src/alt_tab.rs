use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabWindow {
    pub id: u32,
    pub owner_pid: i32,
    pub app_name: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_icon_data_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabWindowLite {
    pub id: u32,
    pub owner_pid: i32,
    pub app_name: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl From<&AltTabWindow> for AltTabWindowLite {
    fn from(window: &AltTabWindow) -> Self {
        Self {
            id: window.id,
            owner_pid: window.owner_pid,
            app_name: window.app_name.clone(),
            title: window.title.clone(),
            width: window.width,
            height: window.height,
        }
    }
}

trait AltTabLayoutWindow {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
}

impl AltTabLayoutWindow for AltTabWindow {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

impl AltTabLayoutWindow for AltTabWindowLite {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }
}

fn normalized_aspect_ratio(width: u32, height: u32) -> f64 {
    const MIN_ASPECT_RATIO: f64 = 0.62;
    const MAX_ASPECT_RATIO: f64 = 2.4;

    let width = width.max(1) as f64;
    let height = height.max(1) as f64;
    (width / height).clamp(MIN_ASPECT_RATIO, MAX_ASPECT_RATIO)
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct AltTabOverlayTileLayout {
    pub row: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct AltTabOverlayLayout {
    pub rows: usize,
    pub panel_width: u32,
    pub panel_height: u32,
    pub tile_gap: u32,
    pub tile_height: u32,
    pub tiles: Vec<AltTabOverlayTileLayout>,
}

#[derive(Debug, Clone)]
struct PackedOverlayLayout {
    rows: usize,
    max_row_width: u32,
    content_height: u32,
    tile_height: u32,
    tiles: Vec<AltTabOverlayTileLayout>,
}

impl AltTabOverlayLayout {
    fn for_windows<T: AltTabLayoutWindow>(windows: &[T]) -> Self {
        Self::with_limits(windows, 1800, 920)
    }

    fn with_limits<T: AltTabLayoutWindow>(windows: &[T], max_width: u32, max_height: u32) -> Self {
        const PANEL_HORIZONTAL_CHROME: u32 = 32;
        const PANEL_VERTICAL_CHROME: u32 = 32;
        const TILE_GAP: u32 = 18;
        const MIN_TILE_HEIGHT: u32 = 96;
        const MAX_TILE_HEIGHT: u32 = 400;

        if windows.is_empty() {
            return Self::default();
        }

        let inner_width = max_width.saturating_sub(PANEL_HORIZONTAL_CHROME).max(1);
        let inner_height = max_height.saturating_sub(PANEL_VERTICAL_CHROME).max(1);
        let max_tile_height = inner_height.min(MAX_TILE_HEIGHT).max(1);
        let min_tile_height = MIN_TILE_HEIGHT.min(max_tile_height).max(1);

        let mut low = min_tile_height;
        let mut high = max_tile_height;
        let mut best = Self::pack_rows(
            windows,
            inner_width,
            inner_height,
            min_tile_height,
            TILE_GAP,
        );

        while low <= high {
            let tile_height = low + (high - low) / 2;
            if let Some(candidate) =
                Self::pack_rows(windows, inner_width, inner_height, tile_height, TILE_GAP)
            {
                best = Some(candidate);
                low = tile_height.saturating_add(1);
            } else if tile_height == 0 {
                break;
            } else {
                high = tile_height.saturating_sub(1);
            }
        }

        let Some(best) =
            best.or_else(|| Self::pack_rows(windows, inner_width, inner_height, 1, TILE_GAP))
        else {
            return Self::default();
        };

        Self::from_packed_layout(best)
    }

    fn pack_rows<T: AltTabLayoutWindow>(
        windows: &[T],
        inner_width: u32,
        inner_height: u32,
        tile_height: u32,
        tile_gap: u32,
    ) -> Option<PackedOverlayLayout> {
        if windows.is_empty() || tile_height == 0 {
            return None;
        }

        let mut tiles = Vec::with_capacity(windows.len());
        let mut row = 0usize;
        let mut current_row_width = 0u32;
        let mut max_row_width = 0u32;

        for window in windows {
            let tile_width = Self::tile_width(window, tile_height);
            if tile_width > inner_width {
                return None;
            }

            let next_row_width = if current_row_width == 0 {
                tile_width
            } else {
                current_row_width + tile_gap + tile_width
            };

            if current_row_width > 0 && next_row_width > inner_width {
                max_row_width = max_row_width.max(current_row_width);
                row += 1;
                current_row_width = tile_width;
            } else {
                current_row_width = next_row_width;
            }

            tiles.push(AltTabOverlayTileLayout {
                row,
                width: tile_width,
                height: tile_height,
            });
        }

        max_row_width = max_row_width.max(current_row_width);
        let rows = row + 1;
        let content_height = rows as u32 * tile_height + rows.saturating_sub(1) as u32 * tile_gap;
        if content_height > inner_height {
            return None;
        }

        Some(PackedOverlayLayout {
            rows,
            max_row_width,
            content_height,
            tile_height,
            tiles,
        })
    }

    fn tile_width<T: AltTabLayoutWindow>(window: &T, tile_height: u32) -> u32 {
        (normalized_aspect_ratio(window.width(), window.height()) * tile_height as f64)
            .round()
            .max(1.0) as u32
    }
}

fn normalize_window_dimension(value: f64) -> u32 {
    if !value.is_finite() {
        return 1;
    }

    value.round().max(1.0) as u32
}

fn build_alt_tab_window(
    id: u32,
    owner_pid: i32,
    app_name: String,
    title: String,
    width: u32,
    height: u32,
) -> AltTabWindow {
    AltTabWindow {
        id,
        owner_pid,
        app_name,
        title,
        width: width.max(1),
        height: height.max(1),
        app_icon_data_url: None,
        thumbnail_data_url: None,
    }
}

impl AltTabOverlayLayout {
    fn from_packed_layout(best: PackedOverlayLayout) -> Self {
        const PANEL_HORIZONTAL_CHROME: u32 = 32;
        const PANEL_VERTICAL_CHROME: u32 = 32;
        const TILE_GAP: u32 = 18;

        Self {
            rows: best.rows,
            panel_width: best.max_row_width + PANEL_HORIZONTAL_CHROME,
            panel_height: best.content_height + PANEL_VERTICAL_CHROME,
            tile_gap: TILE_GAP,
            tile_height: best.tile_height,
            tiles: best.tiles,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabOverlayState {
    pub visible: bool,
    pub session_id: u64,
    pub selected_index: Option<usize>,
    pub hovered_index: Option<usize>,
    pub windows: Vec<AltTabWindowLite>,
    pub layout: AltTabOverlayLayout,
}

impl AltTabOverlayState {
    pub fn hidden() -> Self {
        Self {
            visible: false,
            session_id: 0,
            selected_index: None,
            hovered_index: None,
            windows: Vec::new(),
            layout: AltTabOverlayLayout::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabOverlaySelection {
    pub visible: bool,
    pub session_id: u64,
    pub selected_index: Option<usize>,
    pub hovered_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabThumbnailUpdate {
    pub session_id: u64,
    pub window_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AltTabAppIconUpdate {
    pub session_id: u64,
    pub owner_pid: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_icon_data_url: Option<String>,
}

#[derive(Debug, Default)]
pub struct AltTabSession {
    generation: u64,
    windows: Vec<AltTabWindow>,
    selected_index: Option<usize>,
    hovered_index: Option<usize>,
}

impl AltTabSession {
    pub fn start(&mut self, windows: Vec<AltTabWindow>, reverse: bool) -> bool {
        if windows.len() < 2 {
            self.cancel();
            return false;
        }

        self.generation = self.generation.wrapping_add(1);
        self.windows = windows;
        self.selected_index = Some(if reverse { self.windows.len() - 1 } else { 1 });
        self.hovered_index = None;
        true
    }

    pub fn cycle(&mut self, reverse: bool) -> bool {
        let Some(current) = self.selected_index else {
            return false;
        };
        let len = self.windows.len();
        if len < 2 {
            return false;
        }

        self.selected_index = Some(if reverse {
            if current == 0 { len - 1 } else { current - 1 }
        } else {
            (current + 1) % len
        });
        self.hovered_index = None;
        true
    }

    pub fn commit(&mut self) -> Option<AltTabWindow> {
        let selected = self.selected().cloned();
        self.cancel();
        selected
    }

    pub fn activate_window(&mut self, window_id: u32) -> Option<AltTabWindow> {
        if !self.select_window(window_id) {
            return None;
        }

        self.commit()
    }

    pub fn cancel(&mut self) {
        self.windows.clear();
        self.selected_index = None;
        self.hovered_index = None;
    }

    pub fn selected(&self) -> Option<&AltTabWindow> {
        self.selected_index
            .and_then(|index| self.windows.get(index))
    }

    pub fn is_active(&self) -> bool {
        self.selected_index.is_some() && !self.windows.is_empty()
    }

    pub fn session_id(&self) -> u64 {
        self.generation
    }

    pub fn select_window(&mut self, window_id: u32) -> bool {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.id == window_id)
        else {
            return false;
        };

        let changed = self.selected_index != Some(index) || self.hovered_index.is_some();
        self.selected_index = Some(index);
        self.hovered_index = None;
        changed
    }

    pub fn set_hovered_window(&mut self, window_id: Option<u32>) -> bool {
        let hovered_index = match window_id {
            Some(window_id) => {
                let Some(index) = self
                    .windows
                    .iter()
                    .position(|window| window.id == window_id)
                else {
                    return false;
                };
                Some(index)
            }
            None => None,
        };

        if self.hovered_index == hovered_index {
            return false;
        }

        self.hovered_index = hovered_index;
        true
    }

    pub fn update_thumbnail(&mut self, window_id: u32, thumbnail_data_url: Option<String>) -> bool {
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.id == window_id)
        else {
            return false;
        };

        window.thumbnail_data_url = thumbnail_data_url;
        true
    }

    pub fn update_app_icon(&mut self, owner_pid: i32, app_icon_data_url: Option<String>) -> bool {
        let mut changed = false;

        for window in self
            .windows
            .iter_mut()
            .filter(|window| window.owner_pid == owner_pid)
        {
            if window.app_icon_data_url != app_icon_data_url {
                window.app_icon_data_url = app_icon_data_url.clone();
                changed = true;
            }
        }

        changed
    }

    pub fn overlay_selection(&self) -> AltTabOverlaySelection {
        AltTabOverlaySelection {
            visible: self.is_active(),
            session_id: if self.is_active() { self.generation } else { 0 },
            selected_index: self.selected_index,
            hovered_index: self.hovered_index,
        }
    }

    pub fn overlay_state(&self) -> AltTabOverlayState {
        if !self.is_active() {
            return AltTabOverlayState::hidden();
        }

        AltTabOverlayState {
            visible: true,
            session_id: self.generation,
            selected_index: self.selected_index,
            hovered_index: self.hovered_index,
            windows: self.windows.iter().map(AltTabWindowLite::from).collect(),
            layout: AltTabOverlayLayout::for_windows(&self.windows),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AltTabThumbnailCache {
    entries: HashMap<u32, String>,
}

impl AltTabThumbnailCache {
    pub fn store(&mut self, window_id: u32, thumbnail_data_url: String) {
        self.entries.insert(window_id, thumbnail_data_url);
    }

    pub fn hydrate_windows(&self, windows: &mut [AltTabWindow]) {
        for window in windows {
            if let Some(thumbnail_data_url) = self.entries.get(&window.id) {
                window.thumbnail_data_url = Some(thumbnail_data_url.clone());
            }
        }
    }

    pub fn get(&self, window_id: u32) -> Option<&String> {
        self.entries.get(&window_id)
    }

    pub fn capture_targets(&self, session: &AltTabSession) -> Vec<u32> {
        let selected_id = session.selected().map(|window| window.id);
        let mut targets = Vec::new();

        if let Some(selected_id) = selected_id {
            targets.push(selected_id);
        }

        targets.extend(
            session
                .windows
                .iter()
                .map(|window| window.id)
                .filter(|window_id| {
                    Some(*window_id) != selected_id && !self.entries.contains_key(window_id)
                }),
        );

        targets
    }

    pub fn retain_window_ids<I>(&mut self, window_ids: I)
    where
        I: IntoIterator<Item = u32>,
    {
        let visible_window_ids = window_ids.into_iter().collect::<HashSet<_>>();
        self.entries
            .retain(|window_id, _| visible_window_ids.contains(window_id));
    }
}

#[derive(Debug, Default, Clone)]
pub struct AltTabAppIconCache {
    entries: HashMap<i32, String>,
}

impl AltTabAppIconCache {
    pub fn store(&mut self, owner_pid: i32, app_icon_data_url: String) {
        self.entries.insert(owner_pid, app_icon_data_url);
    }

    pub fn hydrate_windows(&self, windows: &mut [AltTabWindow]) {
        for window in windows {
            if let Some(app_icon_data_url) = self.entries.get(&window.owner_pid) {
                window.app_icon_data_url = Some(app_icon_data_url.clone());
            }
        }
    }

    pub fn get(&self, owner_pid: i32) -> Option<&String> {
        self.entries.get(&owner_pid)
    }

    pub fn capture_targets(&self, windows: &[AltTabWindow]) -> Vec<i32> {
        let mut seen_pids = HashSet::new();
        let mut targets = Vec::new();

        for window in windows {
            if seen_pids.insert(window.owner_pid) && !self.entries.contains_key(&window.owner_pid) {
                targets.push(window.owner_pid);
            }
        }

        targets
    }

    pub fn retain_owner_pids<I>(&mut self, owner_pids: I)
    where
        I: IntoIterator<Item = i32>,
    {
        let visible_owner_pids = owner_pids.into_iter().collect::<HashSet<_>>();
        self.entries
            .retain(|owner_pid, _| visible_owner_pids.contains(owner_pid));
    }
}

#[derive(Debug, Default, Clone)]
pub struct AltTabWindowSnapshotCache {
    windows: Vec<AltTabWindow>,
    updated_at: Option<Instant>,
}

impl AltTabWindowSnapshotCache {
    pub fn store(&mut self, windows: Vec<AltTabWindow>) {
        self.windows = windows;
        self.updated_at = Some(Instant::now());
    }

    pub fn windows(&self) -> Vec<AltTabWindow> {
        self.windows.clone()
    }

    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.updated_at
            .map(|updated_at| updated_at.elapsed() <= max_age)
            .unwrap_or(false)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{
        AltTabAppIconCache, AltTabAppIconUpdate, AltTabOverlayLayout, AltTabOverlaySelection,
        AltTabOverlayState, AltTabSession, AltTabThumbnailCache, AltTabThumbnailUpdate,
        AltTabWindow, AltTabWindowLite, AltTabWindowSnapshotCache,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use core_graphics::{
        access::ScreenCaptureAccess,
        base::kCGImageAlphaPremultipliedLast,
        color_space::CGColorSpace,
        context::{CGContext, CGInterpolationQuality},
        display::CGRectNull,
        geometry::{CGPoint as CgPoint, CGRect as CgRect, CGSize as CgSize},
        image::CGImage,
        window::{
            create_image as cg_create_window_image, kCGWindowImageBestResolution,
            kCGWindowImageBoundsIgnoreFraming, kCGWindowListOptionIncludingWindow,
        },
    };
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSColor, NSPopUpMenuWindowLevel,
        NSRunningApplication, NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
        NSWindowStyleMask,
    };
    use objc2_foundation::NSDictionary;
    use png::{BitDepth, ColorType, Compression, Encoder};
    use std::collections::{HashMap, HashSet};
    use std::ffi::{CStr, CString, c_void};
    use std::ptr;
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, Instant};
    use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};
    use yorling_engine::engine::SystemAction;

    const ALT_TAB_WINDOW_LABEL: &str = "alt-tab-overlay";
    const ALT_TAB_OVERLAY_STATE_EVENT: &str = "alt-tab-overlay-state";
    const ALT_TAB_OVERLAY_SELECTION_EVENT: &str = "alt-tab-overlay-selection";
    const ALT_TAB_OVERLAY_THUMBNAIL_EVENT: &str = "alt-tab-overlay-thumbnail";
    const ALT_TAB_OVERLAY_APP_ICON_EVENT: &str = "alt-tab-overlay-app-icon";
    const ALT_TAB_AX_TIMEOUT_SECONDS: f32 = 0.35;
    const ALT_TAB_SNAPSHOT_MAX_AGE_MS: u64 = 2_000;
    const ALT_TAB_AX_MAX_PARALLELISM: usize = 4;
    const ALT_TAB_OVERLAY_REVEAL_DELAY_MS: u64 = 220;
    const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
    const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_CF_NUMBER_SINT64_TYPE: i32 = 4;
    const AX_SUCCESS: i32 = 0;
    const MIN_WINDOW_WIDTH: f64 = 100.0;
    const MIN_WINDOW_HEIGHT: f64 = 50.0;
    const AX_ROLE_WINDOW: &str = "AXWindow";
    const AX_STANDARD_WINDOW_SUBROLE: &str = "AXStandardWindow";
    const AX_DIALOG_SUBROLE: &str = "AXDialog";
    const AX_DOCUMENT_WINDOW_SUBROLE: &str = "AXDocumentWindow";
    const SLPS_MODE_USER_GENERATED: u32 = 0x200;
    const THUMBNAIL_MAX_WIDTH: usize = 480;
    const THUMBNAIL_MAX_HEIGHT: usize = 300;

    #[derive(Debug, Clone, Default)]
    struct OverlayWindowPresentation {
        layout: AltTabOverlayLayout,
        origin: Option<(f64, f64)>,
    }

    enum AltTabCommand {
        System(SystemAction),
        RevealPendingSession { session_id: u64 },
        SelectWindow { window_id: u32 },
        SetHoveredWindow { window_id: Option<u32> },
        ActivateWindow { window_id: u32 },
    }

    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFNumberRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;
    type AXUIElementRef = *const c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct ProcessSerialNumber {
        high_long_of_psn: u32,
        low_long_of_psn: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub(super) struct CgWindowEntry {
        pub(super) id: u32,
        pub(super) owner_pid: i32,
        pub(super) app_name: String,
        pub(super) title: Option<String>,
        pub(super) width: f64,
        pub(super) height: f64,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(super) struct AxWindowEntry {
        pub(super) id: u32,
        pub(super) title: Option<String>,
        pub(super) role: Option<String>,
        pub(super) subrole: Option<String>,
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *const c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFArrayRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect)
        -> bool;

        static kCGWindowNumber: CFStringRef;
        static kCGWindowLayer: CFStringRef;
        static kCGWindowBounds: CFStringRef;
        static kCGWindowOwnerPID: CFStringRef;
        static kCGWindowOwnerName: CFStringRef;
        static kCGWindowName: CFStringRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFArrayGetCount(the_array: CFArrayRef) -> isize;
        fn CFArrayGetValueAtIndex(the_array: CFArrayRef, idx: isize) -> *const c_void;
        fn CFDictionaryGetValue(the_dict: CFDictionaryRef, key: *const c_void) -> *const c_void;
        fn CFNumberGetValue(number: CFNumberRef, number_type: i32, value_ptr: *mut c_void) -> bool;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            cstr: *const i8,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(the_string: CFStringRef) -> isize;
        fn CFStringGetCString(
            the_string: CFStringRef,
            buffer: *mut i8,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
        fn CFRelease(cf: *const c_void);

        static kCFAllocatorDefault: *const c_void;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> i32;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_in_seconds: f32) -> i32;

        #[link_name = "_AXUIElementGetWindow"]
        fn ax_ui_element_get_window(element: AXUIElementRef, window_id: *mut u32) -> i32;

        fn GetProcessForPID(pid: i32, psn: *mut ProcessSerialNumber) -> i32;
    }

    #[link(name = "SkyLight", kind = "framework")]
    unsafe extern "C" {
        fn _SLPSSetFrontProcessWithOptions(
            psn: *mut ProcessSerialNumber,
            window_id: u32,
            mode: u32,
        ) -> i32;
        fn SLPSPostEventRecordTo(psn: *mut ProcessSerialNumber, bytes: *mut u8) -> i32;
    }

    #[derive(Clone)]
    pub struct AltTabManager {
        sender: mpsc::Sender<AltTabCommand>,
        overlay_state: Arc<Mutex<AltTabOverlayState>>,
        app_handle: Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: Arc<Mutex<AltTabAppIconCache>>,
    }

    impl AltTabManager {
        pub fn new() -> Self {
            let (sender, receiver) = mpsc::channel();
            let overlay_state = Arc::new(Mutex::new(AltTabOverlayState::hidden()));
            let app_handle = Arc::new(Mutex::new(None::<AppHandle>));
            let session = Arc::new(Mutex::new(AltTabSession::default()));
            let thumbnail_cache = Arc::new(Mutex::new(AltTabThumbnailCache::default()));
            let app_icon_cache = Arc::new(Mutex::new(AltTabAppIconCache::default()));
            let ax_window_cache = Arc::new(Mutex::new(
                HashMap::<i32, HashMap<u32, AxWindowEntry>>::new(),
            ));
            let window_snapshot_cache = Arc::new(Mutex::new(AltTabWindowSnapshotCache::default()));

            thread::spawn({
                let overlay_state = Arc::clone(&overlay_state);
                let app_handle = Arc::clone(&app_handle);
                let session = Arc::clone(&session);
                let thumbnail_cache = Arc::clone(&thumbnail_cache);
                let app_icon_cache = Arc::clone(&app_icon_cache);
                let ax_window_cache = Arc::clone(&ax_window_cache);
                let window_snapshot_cache = Arc::clone(&window_snapshot_cache);
                let command_sender = sender.clone();

                move || {
                    prime_ax_messaging_timeout();
                    run_worker(
                        receiver,
                        command_sender,
                        session,
                        overlay_state,
                        app_handle,
                        thumbnail_cache,
                        app_icon_cache,
                        ax_window_cache,
                        window_snapshot_cache,
                    )
                }
            });

            Self {
                sender,
                overlay_state,
                app_handle,
                thumbnail_cache,
                app_icon_cache,
            }
        }

        pub fn bind_app_handle(&self, app_handle: AppHandle) {
            *self.app_handle.lock().unwrap() = Some(app_handle.clone());
            if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
                configure_overlay_window(&window);
                prewarm_overlay_window(&window, &self.overlay_state);

                let state = self.overlay_state();
                let _ = window.emit(ALT_TAB_OVERLAY_STATE_EVENT, state.clone());
                publish_cached_asset_updates(
                    &self.overlay_state,
                    &self.app_handle,
                    &self.thumbnail_cache,
                    &self.app_icon_cache,
                    &state,
                );
            }
        }

        pub fn dispatch(&self, action: SystemAction) {
            let _ = self.sender.send(AltTabCommand::System(action));
        }

        pub fn select_window(&self, window_id: u32) {
            let _ = self.sender.send(AltTabCommand::SelectWindow { window_id });
        }

        pub fn set_hovered_window(&self, window_id: Option<u32>) {
            let _ = self
                .sender
                .send(AltTabCommand::SetHoveredWindow { window_id });
        }

        pub fn activate_window(&self, window_id: u32) {
            let _ = self
                .sender
                .send(AltTabCommand::ActivateWindow { window_id });
        }

        pub fn overlay_state(&self) -> AltTabOverlayState {
            self.overlay_state.lock().unwrap().clone()
        }

        pub fn thumbnail_data_url(&self, window_id: u32) -> Option<String> {
            self.thumbnail_cache.lock().unwrap().get(window_id).cloned()
        }

        pub fn app_icon_data_url(&self, owner_pid: i32) -> Option<String> {
            self.app_icon_cache.lock().unwrap().get(owner_pid).cloned()
        }
    }

    fn run_worker(
        receiver: mpsc::Receiver<AltTabCommand>,
        command_sender: mpsc::Sender<AltTabCommand>,
        session: Arc<Mutex<AltTabSession>>,
        overlay_state: Arc<Mutex<AltTabOverlayState>>,
        app_handle: Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: Arc<Mutex<AltTabAppIconCache>>,
        ax_window_cache: Arc<Mutex<HashMap<i32, HashMap<u32, AxWindowEntry>>>>,
        window_snapshot_cache: Arc<Mutex<AltTabWindowSnapshotCache>>,
    ) {
        while let Ok(command) = receiver.recv() {
            match command {
                AltTabCommand::System(SystemAction::AltTabCycle { reverse }) => {
                    handle_cycle(
                        &command_sender,
                        &session,
                        &overlay_state,
                        &app_handle,
                        &thumbnail_cache,
                        &app_icon_cache,
                        &ax_window_cache,
                        &window_snapshot_cache,
                        reverse,
                    );
                }
                AltTabCommand::System(SystemAction::AltTabCommit) => {
                    handle_commit(&session, &overlay_state, &app_handle);
                }
                AltTabCommand::System(SystemAction::AltTabCancel) => {
                    handle_cancel(&session, &overlay_state, &app_handle);
                }
                AltTabCommand::RevealPendingSession { session_id } => {
                    handle_reveal_pending_session(
                        &session,
                        &overlay_state,
                        &app_handle,
                        &thumbnail_cache,
                        &app_icon_cache,
                        session_id,
                    );
                }
                AltTabCommand::System(SystemAction::BracketModeChanged { .. }) => {}
                AltTabCommand::System(
                    SystemAction::MouseMove { .. }
                    | SystemAction::MouseClick { .. }
                    | SystemAction::MouseScroll { .. }
                    | SystemAction::MouseModeChanged { .. },
                ) => {}
                AltTabCommand::SelectWindow { window_id } => {
                    handle_select_window(&session, &overlay_state, &app_handle, window_id);
                }
                AltTabCommand::SetHoveredWindow { window_id } => {
                    handle_hovered_window(&session, &overlay_state, &app_handle, window_id);
                }
                AltTabCommand::ActivateWindow { window_id } => {
                    handle_activate_window(&session, &overlay_state, &app_handle, window_id);
                }
            }
        }
    }

    fn handle_cycle(
        command_sender: &mpsc::Sender<AltTabCommand>,
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        ax_window_cache: &Arc<Mutex<HashMap<i32, HashMap<u32, AxWindowEntry>>>>,
        window_snapshot_cache: &Arc<Mutex<AltTabWindowSnapshotCache>>,
        reverse: bool,
    ) {
        enum CycleUpdate {
            Full(AltTabOverlayState),
            Selection(AltTabOverlaySelection),
            DeferredReveal { session_id: u64 },
        }

        let cycle_started_at = Instant::now();
        let overlay_was_visible = is_overlay_visible(overlay_state);
        let next_update = {
            let mut session = session.lock().unwrap();
            if session.is_active() {
                if !session.cycle(reverse) {
                    session.cancel();
                    CycleUpdate::Full(session.overlay_state())
                } else if overlay_was_visible {
                    CycleUpdate::Selection(session.overlay_selection())
                } else {
                    CycleUpdate::Full(session.overlay_state())
                }
            } else {
                let (snapshot_is_fresh, mut windows) = {
                    let cache = window_snapshot_cache.lock().unwrap();
                    (
                        cache.is_fresh(Duration::from_millis(ALT_TAB_SNAPSHOT_MAX_AGE_MS)),
                        cache.windows(),
                    )
                };
                if windows.len() < 2 || !snapshot_is_fresh {
                    windows = refresh_snapshot_from_caches(
                        thumbnail_cache,
                        app_icon_cache,
                        ax_window_cache,
                        window_snapshot_cache,
                        true,
                    );
                }
                {
                    let mut thumbnail_cache = thumbnail_cache.lock().unwrap();
                    thumbnail_cache.retain_window_ids(windows.iter().map(|window| window.id));
                    thumbnail_cache.hydrate_windows(&mut windows);
                }
                {
                    let mut app_icon_cache = app_icon_cache.lock().unwrap();
                    app_icon_cache.retain_owner_pids(windows.iter().map(|window| window.owner_pid));
                    app_icon_cache.hydrate_windows(&mut windows);
                }
                if !session.start(windows, reverse) {
                    log::info!(
                        "Alt+Tab ignored because fewer than two eligible windows were found"
                    );
                    CycleUpdate::Full(session.overlay_state())
                } else {
                    CycleUpdate::DeferredReveal {
                        session_id: session.session_id(),
                    }
                }
            }
        };

        let cycle_elapsed = cycle_started_at.elapsed();
        if cycle_elapsed > Duration::from_millis(120) {
            log::debug!(
                "Alt+Tab cycle processing took {}ms",
                cycle_elapsed.as_millis()
            );
        }

        match next_update {
            CycleUpdate::Full(next_state) => {
                publish_full_overlay_update(
                    session,
                    overlay_state,
                    app_handle,
                    thumbnail_cache,
                    app_icon_cache,
                    next_state,
                );
            }
            CycleUpdate::Selection(selection) => {
                let should_show = selection.visible;
                publish_overlay_selection(overlay_state, app_handle, selection);

                if should_show {
                    show_overlay_window(app_handle, &OverlayWindowPresentation::default());
                } else {
                    hide_overlay_window(app_handle);
                }
            }
            CycleUpdate::DeferredReveal { session_id } => {
                schedule_overlay_reveal(command_sender, session_id);
            }
        }
    }

    fn schedule_overlay_reveal(command_sender: &mpsc::Sender<AltTabCommand>, session_id: u64) {
        let command_sender = command_sender.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(ALT_TAB_OVERLAY_REVEAL_DELAY_MS));
            let _ = command_sender.send(AltTabCommand::RevealPendingSession { session_id });
        });
    }

    fn handle_reveal_pending_session(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        session_id: u64,
    ) {
        if is_overlay_visible(overlay_state) {
            return;
        }

        let next_state = {
            let session = session.lock().unwrap();
            if !session.is_active() || session.session_id() != session_id {
                return;
            }
            session.overlay_state()
        };

        publish_full_overlay_update(
            session,
            overlay_state,
            app_handle,
            thumbnail_cache,
            app_icon_cache,
            next_state,
        );
    }

    fn publish_full_overlay_update(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        next_state: AltTabOverlayState,
    ) {
        let mut thumbnail_capture = None;
        let mut app_icon_capture = None;
        let mut cached_thumbnail_updates = Vec::new();
        let mut cached_app_icon_updates = Vec::new();

        let (next_state, presentation) = sync_overlay_layout_to_window(app_handle, next_state);
        if next_state.visible {
            (cached_thumbnail_updates, cached_app_icon_updates) =
                collect_cached_asset_updates(&next_state, thumbnail_cache, app_icon_cache);
            let (thumbnail_targets, app_icon_targets) = {
                let session = session.lock().unwrap();
                (
                    thumbnail_cache.lock().unwrap().capture_targets(&session),
                    app_icon_cache
                        .lock()
                        .unwrap()
                        .capture_targets(&session.windows),
                )
            };
            if !thumbnail_targets.is_empty() {
                thumbnail_capture = Some((next_state.session_id, thumbnail_targets));
            }
            if !app_icon_targets.is_empty() {
                app_icon_capture = Some((next_state.session_id, app_icon_targets));
            }
        }

        let should_show = next_state.visible;
        publish_overlay_state(overlay_state, app_handle, next_state);

        if should_show {
            show_overlay_window(app_handle, &presentation);
        } else {
            hide_overlay_window(app_handle);
        }

        for update in cached_thumbnail_updates {
            publish_thumbnail_update(overlay_state, app_handle, update);
        }

        for update in cached_app_icon_updates {
            publish_app_icon_update(overlay_state, app_handle, update);
        }

        if let Some((session_id, window_ids)) = thumbnail_capture {
            start_thumbnail_capture(
                session,
                overlay_state,
                app_handle,
                thumbnail_cache,
                session_id,
                window_ids,
            );
        }

        if let Some((session_id, owner_pids)) = app_icon_capture {
            start_app_icon_capture(
                session,
                overlay_state,
                app_handle,
                app_icon_cache,
                session_id,
                owner_pids,
            );
        }
    }

    fn is_overlay_visible(overlay_state: &Arc<Mutex<AltTabOverlayState>>) -> bool {
        overlay_state.lock().unwrap().visible
    }

    fn prime_ax_messaging_timeout() {
        unsafe {
            let system_wide = AXUIElementCreateSystemWide();
            if system_wide.is_null() {
                return;
            }
            let _ = AXUIElementSetMessagingTimeout(system_wide, ALT_TAB_AX_TIMEOUT_SECONDS);
            CFRelease(system_wide as *const c_void);
        }
    }

    fn refresh_snapshot_from_caches(
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        ax_window_cache: &Arc<Mutex<HashMap<i32, HashMap<u32, AxWindowEntry>>>>,
        window_snapshot_cache: &Arc<Mutex<AltTabWindowSnapshotCache>>,
        force_full_ax_refresh: bool,
    ) -> Vec<AltTabWindow> {
        let cg_windows = collect_current_cg_window_candidates();
        let mut windows = refresh_window_model_from_cache(
            &cg_windows,
            thumbnail_cache,
            app_icon_cache,
            ax_window_cache,
            window_snapshot_cache,
        );

        let refreshed_owner_pid_count =
            refresh_ax_window_cache(&cg_windows, ax_window_cache, force_full_ax_refresh);
        if refreshed_owner_pid_count > 0 {
            windows = refresh_window_model_from_cache(
                &cg_windows,
                thumbnail_cache,
                app_icon_cache,
                ax_window_cache,
                window_snapshot_cache,
            );
        }

        windows
    }

    fn refresh_window_model_from_cache(
        cg_windows: &[CgWindowEntry],
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        ax_window_cache: &Arc<Mutex<HashMap<i32, HashMap<u32, AxWindowEntry>>>>,
        window_snapshot_cache: &Arc<Mutex<AltTabWindowSnapshotCache>>,
    ) -> Vec<AltTabWindow> {
        let model_refresh_started_at = Instant::now();
        let visible_owner_pids = cg_windows
            .iter()
            .map(|window| window.owner_pid)
            .collect::<HashSet<_>>();

        let mut windows = {
            let mut ax_window_cache = ax_window_cache.lock().unwrap();
            ax_window_cache.retain(|owner_pid, _| visible_owner_pids.contains(owner_pid));
            merge_window_candidates(cg_windows.to_vec(), &ax_window_cache)
        };

        {
            let mut thumbnail_cache = thumbnail_cache.lock().unwrap();
            thumbnail_cache.retain_window_ids(windows.iter().map(|window| window.id));
            thumbnail_cache.hydrate_windows(&mut windows);
        }
        {
            let mut app_icon_cache = app_icon_cache.lock().unwrap();
            app_icon_cache.retain_owner_pids(windows.iter().map(|window| window.owner_pid));
            app_icon_cache.hydrate_windows(&mut windows);
        }

        window_snapshot_cache.lock().unwrap().store(windows.clone());

        let model_refresh_elapsed = model_refresh_started_at.elapsed();
        if model_refresh_elapsed > Duration::from_millis(80) {
            log::debug!(
                "Alt+Tab live model rebuild took {}ms (windows={})",
                model_refresh_elapsed.as_millis(),
                windows.len()
            );
        }

        windows
    }

    fn refresh_ax_window_cache(
        cg_windows: &[CgWindowEntry],
        ax_window_cache: &Arc<Mutex<HashMap<i32, HashMap<u32, AxWindowEntry>>>>,
        force_full_ax_refresh: bool,
    ) -> usize {
        let ax_refresh_started_at = Instant::now();
        let visible_owner_pids = cg_windows
            .iter()
            .map(|window| window.owner_pid)
            .collect::<HashSet<_>>();
        let owner_pids_to_refresh = {
            let ax_window_cache = ax_window_cache.lock().unwrap();
            if force_full_ax_refresh {
                owner_pids_in_cg_order(cg_windows)
            } else {
                missing_owner_pids_for_cg_windows(cg_windows, &ax_window_cache)
            }
        };
        if owner_pids_to_refresh.is_empty() {
            return 0;
        }

        let ax_updates = collect_accessibility_windows_for_owner_pids(&owner_pids_to_refresh);
        {
            let mut ax_window_cache = ax_window_cache.lock().unwrap();
            ax_window_cache.retain(|owner_pid, _| visible_owner_pids.contains(owner_pid));
            for (owner_pid, ax_windows) in ax_updates {
                if !ax_windows.is_empty() {
                    ax_window_cache.insert(owner_pid, ax_windows);
                }
            }
        }

        let ax_refresh_elapsed = ax_refresh_started_at.elapsed();
        if ax_refresh_elapsed > Duration::from_millis(120) {
            log::debug!(
                "Alt+Tab AX cache refresh took {}ms (refreshed_pids={}, full_refresh={})",
                ax_refresh_elapsed.as_millis(),
                owner_pids_to_refresh.len(),
                force_full_ax_refresh
            );
        }

        owner_pids_to_refresh.len()
    }

    fn handle_commit(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
    ) {
        let (selected, next_state) = {
            let mut session = session.lock().unwrap();
            let selected = session.commit();
            (selected, session.overlay_state())
        };

        publish_overlay_state(overlay_state, app_handle, next_state);
        hide_overlay_window(app_handle);

        if let Some(window) = selected {
            if let Err(error) = focus_window(&window) {
                log::warn!(
                    "Failed to focus Alt+Tab target window {}: {}",
                    window.id,
                    error
                );
            }
        }
    }

    fn handle_cancel(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
    ) {
        let next_state = {
            let mut session = session.lock().unwrap();
            session.cancel();
            session.overlay_state()
        };

        publish_overlay_state(overlay_state, app_handle, next_state);
        hide_overlay_window(app_handle);
    }

    fn handle_select_window(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        window_id: u32,
    ) {
        let selection = {
            let mut session = session.lock().unwrap();
            if !session.select_window(window_id) {
                return;
            }
            session.overlay_selection()
        };

        publish_overlay_selection(overlay_state, app_handle, selection);
    }

    fn handle_hovered_window(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        window_id: Option<u32>,
    ) {
        let selection = {
            let mut session = session.lock().unwrap();
            if !session.set_hovered_window(window_id) {
                return;
            }
            session.overlay_selection()
        };

        publish_overlay_selection(overlay_state, app_handle, selection);
    }

    fn handle_activate_window(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        window_id: u32,
    ) {
        let (selected, next_state) = {
            let mut session = session.lock().unwrap();
            let selected = session.activate_window(window_id);
            (selected, session.overlay_state())
        };

        publish_overlay_state(overlay_state, app_handle, next_state);
        hide_overlay_window(app_handle);

        if let Some(window) = selected {
            if let Err(error) = focus_window(&window) {
                log::warn!(
                    "Failed to focus Alt+Tab target window {}: {}",
                    window.id,
                    error
                );
            }
        }
    }

    fn publish_overlay_state(
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        next_state: AltTabOverlayState,
    ) {
        *overlay_state.lock().unwrap() = next_state.clone();
        emit_overlay_state(app_handle, &next_state);
    }

    fn publish_overlay_selection(
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        selection: AltTabOverlaySelection,
    ) {
        {
            let mut state = overlay_state.lock().unwrap();
            state.visible = selection.visible;
            state.selected_index = selection.selected_index;
            state.hovered_index = selection.hovered_index;
        }
        emit_overlay_selection(app_handle, &selection);
    }

    fn publish_thumbnail_update(
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        update: AltTabThumbnailUpdate,
    ) {
        let should_emit = {
            let state = overlay_state.lock().unwrap();
            state.session_id == update.session_id
                && state
                    .windows
                    .iter()
                    .any(|window| window.id == update.window_id)
        };
        if !should_emit {
            return;
        }

        emit_overlay_thumbnail(app_handle, &update);
    }

    fn publish_app_icon_update(
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        update: AltTabAppIconUpdate,
    ) {
        let should_emit = {
            let state = overlay_state.lock().unwrap();
            state.session_id == update.session_id
                && state
                    .windows
                    .iter()
                    .any(|window| window.owner_pid == update.owner_pid)
        };
        if !should_emit {
            return;
        }

        emit_overlay_app_icon(app_handle, &update);
    }

    fn collect_cached_asset_updates(
        state: &AltTabOverlayState,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
    ) -> (Vec<AltTabThumbnailUpdate>, Vec<AltTabAppIconUpdate>) {
        if !state.visible || state.session_id == 0 {
            return (Vec::new(), Vec::new());
        }

        let thumbnail_updates = {
            let thumbnail_cache = thumbnail_cache.lock().unwrap();
            state
                .windows
                .iter()
                .filter_map(|window| {
                    thumbnail_cache
                        .get(window.id)
                        .cloned()
                        .map(|thumbnail_data_url| AltTabThumbnailUpdate {
                            session_id: state.session_id,
                            window_id: window.id,
                            thumbnail_data_url: Some(thumbnail_data_url),
                        })
                })
                .collect::<Vec<_>>()
        };

        let app_icon_updates = {
            let app_icon_cache = app_icon_cache.lock().unwrap();
            let mut seen_owner_pids = HashSet::new();
            state
                .windows
                .iter()
                .filter_map(|window| {
                    if !seen_owner_pids.insert(window.owner_pid) {
                        return None;
                    }

                    app_icon_cache
                        .get(window.owner_pid)
                        .cloned()
                        .map(|app_icon_data_url| AltTabAppIconUpdate {
                            session_id: state.session_id,
                            owner_pid: window.owner_pid,
                            app_icon_data_url: Some(app_icon_data_url),
                        })
                })
                .collect::<Vec<_>>()
        };

        (thumbnail_updates, app_icon_updates)
    }

    fn publish_cached_asset_updates(
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        state: &AltTabOverlayState,
    ) {
        let (thumbnail_updates, app_icon_updates) =
            collect_cached_asset_updates(state, thumbnail_cache, app_icon_cache);

        for update in thumbnail_updates {
            publish_thumbnail_update(overlay_state, app_handle, update);
        }

        for update in app_icon_updates {
            publish_app_icon_update(overlay_state, app_handle, update);
        }
    }

    fn emit_overlay_state(app_handle: &Arc<Mutex<Option<AppHandle>>>, state: &AltTabOverlayState) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            if let Err(error) = window.emit(ALT_TAB_OVERLAY_STATE_EVENT, state) {
                log::debug!("Failed to emit Alt+Tab overlay state: {error}");
            }
        }
    }

    fn emit_overlay_selection(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        selection: &AltTabOverlaySelection,
    ) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            if let Err(error) = window.emit(ALT_TAB_OVERLAY_SELECTION_EVENT, selection) {
                log::debug!("Failed to emit Alt+Tab overlay selection: {error}");
            }
        }
    }

    fn emit_overlay_thumbnail(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        update: &AltTabThumbnailUpdate,
    ) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            if let Err(error) = window.emit(ALT_TAB_OVERLAY_THUMBNAIL_EVENT, update) {
                log::debug!("Failed to emit Alt+Tab thumbnail update: {error}");
            }
        }
    }

    fn emit_overlay_app_icon(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        update: &AltTabAppIconUpdate,
    ) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            if let Err(error) = window.emit(ALT_TAB_OVERLAY_APP_ICON_EVENT, update) {
                log::debug!("Failed to emit Alt+Tab app icon update: {error}");
            }
        }
    }

    fn sync_overlay_layout_to_window(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        mut next_state: AltTabOverlayState,
    ) -> (AltTabOverlayState, OverlayWindowPresentation) {
        if !next_state.visible {
            return (
                next_state.clone(),
                OverlayWindowPresentation {
                    layout: next_state.layout.clone(),
                    origin: None,
                },
            );
        }

        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            let presentation = OverlayWindowPresentation {
                layout: AltTabOverlayLayout::for_windows(&next_state.windows),
                origin: None,
            };
            next_state.layout = presentation.layout.clone();
            return (next_state, presentation);
        };
        let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) else {
            let presentation = OverlayWindowPresentation {
                layout: AltTabOverlayLayout::for_windows(&next_state.windows),
                origin: None,
            };
            next_state.layout = presentation.layout.clone();
            return (next_state, presentation);
        };

        let presentation = resolve_overlay_presentation(&window, &next_state.windows);
        next_state.layout = presentation.layout.clone();
        (next_state, presentation)
    }

    fn show_overlay_window(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        presentation: &OverlayWindowPresentation,
    ) {
        let Some(handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            let layout = presentation.layout.clone();
            let origin = presentation.origin;

            // Queue geometry and visibility before forcing AppKit front-ordering;
            // otherwise the first launch can show at the stale prewarm position.
            if layout.panel_width > 0 && layout.panel_height > 0 {
                let panel_width = layout.panel_width as f64;
                let panel_height = layout.panel_height as f64;
                let _ = window.set_size(LogicalSize::new(panel_width, panel_height));
                if let Some((position_x, position_y)) = origin {
                    let _ = window.set_position(LogicalPosition::new(position_x, position_y));
                } else {
                    let _ = window.center();
                }
            }
            let _ = window.show();
            let _ = window.set_focus();

            let window_for_lookup = window.clone();
            let _ = window.run_on_main_thread(move || {
                let Some(ns_window) = get_ns_window(&window_for_lookup) else {
                    return;
                };
                ns_window.orderFrontRegardless();
            });
        } else {
            log::warn!("Alt+Tab overlay window is not available");
        }
    }

    fn resolve_overlay_presentation(
        window: &WebviewWindow,
        windows: &[AltTabWindowLite],
    ) -> OverlayWindowPresentation {
        if windows.is_empty() {
            return OverlayWindowPresentation::default();
        }

        let Some(monitor) = overlay_target_monitor(window) else {
            return OverlayWindowPresentation {
                layout: AltTabOverlayLayout::for_windows(windows),
                origin: None,
            };
        };

        let scale_factor = monitor.scale_factor();
        let work_area = monitor.work_area();
        let work_area_x = work_area.position.x as f64 / scale_factor;
        let work_area_y = work_area.position.y as f64 / scale_factor;
        let work_area_width = work_area.size.width as f64 / scale_factor;
        let work_area_height = work_area.size.height as f64 / scale_factor;
        let max_width = ((work_area.size.width as f64 / scale_factor) * 0.94)
            .round()
            .max(960.0) as u32;
        let max_height = ((work_area.size.height as f64 / scale_factor) * 0.9)
            .round()
            .max(560.0) as u32;
        let layout = AltTabOverlayLayout::with_limits(windows, max_width, max_height);

        OverlayWindowPresentation {
            origin: Some(overlay_origin_for_work_area(
                work_area_x,
                work_area_y,
                work_area_width,
                work_area_height,
                layout.panel_width as f64,
                layout.panel_height as f64,
            )),
            layout,
        }
    }

    fn overlay_target_monitor(window: &WebviewWindow) -> Option<tauri::Monitor> {
        mouse_cursor_position()
            .and_then(|(x, y)| window.monitor_from_point(x, y).ok().flatten())
            .or_else(|| window.primary_monitor().ok().flatten())
            .or_else(|| window.current_monitor().ok().flatten())
            .or_else(|| window.available_monitors().ok()?.into_iter().next())
    }

    fn mouse_cursor_position() -> Option<(f64, f64)> {
        unsafe {
            let event = CGEventCreate(ptr::null());
            if event.is_null() {
                return None;
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            Some((point.x, point.y))
        }
    }

    pub(super) fn overlay_origin_for_work_area(
        work_area_x: f64,
        work_area_y: f64,
        work_area_width: f64,
        work_area_height: f64,
        panel_width: f64,
        panel_height: f64,
    ) -> (f64, f64) {
        let horizontal_padding = ((work_area_width - panel_width).max(0.0) / 2.0).round();
        let vertical_padding = ((work_area_height - panel_height).max(0.0) / 2.0).round();

        (
            work_area_x + horizontal_padding,
            work_area_y + vertical_padding,
        )
    }

    fn hide_overlay_window(app_handle: &Arc<Mutex<Option<AppHandle>>>) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(ALT_TAB_WINDOW_LABEL) {
            let _ = window.hide();
        }
    }

    fn collect_accessibility_windows_for_owner_pids(
        owner_pids: &[i32],
    ) -> Vec<(i32, HashMap<u32, AxWindowEntry>)> {
        if owner_pids.is_empty() {
            return Vec::new();
        }

        let available_parallelism = thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1);
        let parallelism = owner_pids
            .len()
            .min(available_parallelism)
            .min(ALT_TAB_AX_MAX_PARALLELISM)
            .max(1);
        if parallelism <= 1 {
            return owner_pids
                .iter()
                .map(|owner_pid| (*owner_pid, collect_accessibility_windows(*owner_pid)))
                .collect();
        }

        let chunk_size = (owner_pids.len() + parallelism - 1) / parallelism;
        let mut results = Vec::with_capacity(owner_pids.len());
        thread::scope(|scope| {
            let mut handles = Vec::new();
            for owner_pid_chunk in owner_pids.chunks(chunk_size) {
                handles.push(scope.spawn(move || {
                    owner_pid_chunk
                        .iter()
                        .map(|owner_pid| (*owner_pid, collect_accessibility_windows(*owner_pid)))
                        .collect::<Vec<_>>()
                }));
            }

            for handle in handles {
                match handle.join() {
                    Ok(chunk_results) => results.extend(chunk_results),
                    Err(_) => {
                        log::debug!("Alt+Tab AX worker panicked while collecting windows");
                    }
                }
            }
        });

        results
    }

    fn owner_pids_in_cg_order(cg_windows: &[CgWindowEntry]) -> Vec<i32> {
        let mut owner_pids = Vec::new();
        let mut seen_owner_pids = HashSet::new();

        for window in cg_windows {
            if seen_owner_pids.insert(window.owner_pid) {
                owner_pids.push(window.owner_pid);
            }
        }

        owner_pids
    }

    fn missing_owner_pids_for_cg_windows(
        cg_windows: &[CgWindowEntry],
        ax_windows_by_pid: &HashMap<i32, HashMap<u32, AxWindowEntry>>,
    ) -> Vec<i32> {
        let mut cg_window_ids_by_pid = HashMap::<i32, HashSet<u32>>::new();
        for window in cg_windows {
            cg_window_ids_by_pid
                .entry(window.owner_pid)
                .or_default()
                .insert(window.id);
        }

        let mut missing_owner_pids = Vec::new();
        for owner_pid in owner_pids_in_cg_order(cg_windows) {
            let Some(cg_window_ids) = cg_window_ids_by_pid.get(&owner_pid) else {
                continue;
            };
            let Some(ax_windows) = ax_windows_by_pid.get(&owner_pid) else {
                missing_owner_pids.push(owner_pid);
                continue;
            };
            if cg_window_ids
                .iter()
                .any(|window_id| !ax_windows.contains_key(window_id))
            {
                missing_owner_pids.push(owner_pid);
            }
        }

        missing_owner_pids
    }

    fn collect_current_cg_window_candidates() -> Vec<CgWindowEntry> {
        unsafe {
            let window_info = CGWindowListCopyWindowInfo(
                K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
                0,
            );
            if window_info.is_null() {
                return Vec::new();
            }

            let current_pid = std::process::id() as i32;
            let cg_windows = collect_cg_window_candidates(window_info, current_pid);
            CFRelease(window_info as *const c_void);
            cg_windows
        }
    }

    fn focus_window(window: &AltTabWindow) -> Result<(), String> {
        unsafe {
            let app = AXUIElementCreateApplication(window.owner_pid);
            if app.is_null() {
                return Err("Failed to create accessibility handle for target app".into());
            }

            let windows_attr = cf_string("AXWindows")?;
            let raise_action = cf_string("AXRaise")?;
            let mut psn = ProcessSerialNumber::default();
            if GetProcessForPID(window.owner_pid, &mut psn) != 0 {
                release_cf_types(&[
                    windows_attr as CFTypeRef,
                    raise_action as CFTypeRef,
                    app as CFTypeRef,
                ]);
                return Err("Failed to resolve process serial number for target app".into());
            }
            if _SLPSSetFrontProcessWithOptions(&mut psn, window.id, SLPS_MODE_USER_GENERATED) != 0 {
                release_cf_types(&[
                    windows_attr as CFTypeRef,
                    raise_action as CFTypeRef,
                    app as CFTypeRef,
                ]);
                return Err("Failed to bring the target app to the front".into());
            }
            make_key_window(&mut psn, window.id)?;

            let mut ax_windows: CFTypeRef = ptr::null();
            let copy_result = AXUIElementCopyAttributeValue(app, windows_attr, &mut ax_windows);
            if copy_result != AX_SUCCESS || ax_windows.is_null() {
                release_cf_types(&[
                    windows_attr as CFTypeRef,
                    raise_action as CFTypeRef,
                    app as CFTypeRef,
                ]);
                return Err("Failed to enumerate accessibility windows for target app".into());
            }

            let array = ax_windows as CFArrayRef;
            let count = CFArrayGetCount(array);
            let mut found = false;

            for index in 0..count {
                let element = CFArrayGetValueAtIndex(array, index) as AXUIElementRef;
                if element.is_null() {
                    continue;
                }

                let mut candidate_id = 0u32;
                if ax_ui_element_get_window(element, &mut candidate_id) == AX_SUCCESS
                    && candidate_id == window.id
                {
                    let raise_result = AXUIElementPerformAction(element, raise_action);
                    if raise_result != AX_SUCCESS {
                        log::debug!(
                            "AXRaise returned {} for Alt+Tab target window {}",
                            raise_result,
                            window.id
                        );
                    }
                    found = true;
                    break;
                }
            }

            release_cf_types(&[
                ax_windows,
                windows_attr as CFTypeRef,
                raise_action as CFTypeRef,
                app as CFTypeRef,
            ]);

            if found {
                Ok(())
            } else {
                Err("Unable to match the selected window in the Accessibility tree".into())
            }
        }
    }

    fn configure_overlay_window(window: &WebviewWindow) {
        let window_handle = window.clone();
        let window_for_lookup = window.clone();
        let _ = window_handle.run_on_main_thread(move || {
            let Some(ns_window) = get_ns_window(&window_for_lookup) else {
                return;
            };
            let style = ns_window.styleMask() | NSWindowStyleMask::NonactivatingPanel;
            ns_window.setStyleMask(style);
            ns_window.setLevel(NSPopUpMenuWindowLevel);
            ns_window.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
            ns_window.setAnimationBehavior(NSWindowAnimationBehavior::None);
            let clear = NSColor::clearColor();
            ns_window.setBackgroundColor(Some(&clear));
            ns_window.setOpaque(false);
            ns_window.setHasShadow(false);
            ns_window.setMovable(false);
            ns_window.setHidesOnDeactivate(false);
            ns_window.setIgnoresMouseEvents(false);
        });
    }

    fn prewarm_overlay_window(
        window: &WebviewWindow,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
    ) {
        if overlay_state.lock().unwrap().visible {
            return;
        }

        let _ = window.set_position(LogicalPosition::new(-10_000.0, -10_000.0));
        let _ = window.set_size(LogicalSize::new(100.0, 100.0));
        let _ = window.show();

        let window = window.clone();
        let overlay_state = Arc::clone(overlay_state);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(80));
            if overlay_state.lock().unwrap().visible {
                return;
            }

            let _ = window.hide();
        });
    }

    fn get_ns_window(window: &WebviewWindow) -> Option<&NSWindow> {
        let ns_window_ptr = match window.ns_window() {
            Ok(handle) => handle,
            Err(error) => {
                log::warn!("Failed to access native Alt+Tab overlay window: {error}");
                return None;
            }
        };

        unsafe { (ns_window_ptr as *mut NSWindow).as_ref() }
    }

    fn collect_cg_window_candidates(
        window_info: CFArrayRef,
        current_pid: i32,
    ) -> Vec<CgWindowEntry> {
        unsafe {
            let count = CFArrayGetCount(window_info);
            let mut windows = Vec::new();

            for index in 0..count {
                let dictionary = CFArrayGetValueAtIndex(window_info, index) as CFDictionaryRef;
                if dictionary.is_null() {
                    continue;
                }

                let layer = dictionary_i64(dictionary, kCGWindowLayer).unwrap_or_default();
                if layer != 0 {
                    continue;
                }

                let owner_pid =
                    dictionary_i64(dictionary, kCGWindowOwnerPID).unwrap_or_default() as i32;
                if owner_pid == current_pid || owner_pid == 0 {
                    continue;
                }

                let window_id =
                    dictionary_i64(dictionary, kCGWindowNumber).unwrap_or_default() as u32;
                if window_id == 0 {
                    continue;
                }

                let app_name =
                    dictionary_string(dictionary, kCGWindowOwnerName).unwrap_or_default();
                if app_name.trim().is_empty() {
                    continue;
                }

                let bounds = dictionary_rect(dictionary, kCGWindowBounds).unwrap_or_default();
                windows.push(CgWindowEntry {
                    id: window_id,
                    owner_pid,
                    app_name,
                    title: dictionary_string(dictionary, kCGWindowName),
                    width: bounds.size.width,
                    height: bounds.size.height,
                });
            }

            windows
        }
    }

    fn collect_accessibility_windows(pid: i32) -> HashMap<u32, AxWindowEntry> {
        unsafe {
            let mut windows = HashMap::new();
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return windows;
            }

            let windows_attr = match cf_string("AXWindows") {
                Ok(value) => value,
                Err(error) => {
                    log::warn!("Failed to create AXWindows attribute: {error}");
                    release_cf_types(&[app as CFTypeRef]);
                    return windows;
                }
            };
            let title_attr = match cf_string("AXTitle") {
                Ok(value) => value,
                Err(error) => {
                    log::warn!("Failed to create AXTitle attribute: {error}");
                    release_cf_types(&[windows_attr as CFTypeRef, app as CFTypeRef]);
                    return windows;
                }
            };
            let role_attr = match cf_string("AXRole") {
                Ok(value) => value,
                Err(error) => {
                    log::warn!("Failed to create AXRole attribute: {error}");
                    release_cf_types(&[
                        windows_attr as CFTypeRef,
                        title_attr as CFTypeRef,
                        app as CFTypeRef,
                    ]);
                    return windows;
                }
            };
            let subrole_attr = match cf_string("AXSubrole") {
                Ok(value) => value,
                Err(error) => {
                    log::warn!("Failed to create AXSubrole attribute: {error}");
                    release_cf_types(&[
                        windows_attr as CFTypeRef,
                        title_attr as CFTypeRef,
                        role_attr as CFTypeRef,
                        app as CFTypeRef,
                    ]);
                    return windows;
                }
            };

            let mut ax_windows: CFTypeRef = ptr::null();
            if AXUIElementCopyAttributeValue(app, windows_attr, &mut ax_windows) == AX_SUCCESS
                && !ax_windows.is_null()
            {
                let array = ax_windows as CFArrayRef;
                let count = CFArrayGetCount(array);
                for index in 0..count {
                    let element = CFArrayGetValueAtIndex(array, index) as AXUIElementRef;
                    if element.is_null() {
                        continue;
                    }

                    let mut window_id = 0u32;
                    if ax_ui_element_get_window(element, &mut window_id) != AX_SUCCESS
                        || window_id == 0
                    {
                        continue;
                    }

                    windows.insert(
                        window_id,
                        AxWindowEntry {
                            id: window_id,
                            title: ax_string_attribute(element, title_attr),
                            role: ax_string_attribute(element, role_attr),
                            subrole: ax_string_attribute(element, subrole_attr),
                        },
                    );
                }
            }

            release_cf_types(&[
                ax_windows,
                windows_attr as CFTypeRef,
                title_attr as CFTypeRef,
                role_attr as CFTypeRef,
                subrole_attr as CFTypeRef,
                app as CFTypeRef,
            ]);
            windows
        }
    }

    pub(super) fn merge_window_candidates(
        cg_windows: Vec<CgWindowEntry>,
        ax_windows_by_pid: &HashMap<i32, HashMap<u32, AxWindowEntry>>,
    ) -> Vec<AltTabWindow> {
        let mut seen = HashSet::new();
        let mut windows = Vec::new();

        for candidate in cg_windows {
            if !seen.insert(candidate.id) {
                continue;
            }

            let Some(ax_window) = ax_windows_by_pid
                .get(&candidate.owner_pid)
                .and_then(|windows| windows.get(&candidate.id))
            else {
                continue;
            };

            if !is_switchable_window(&candidate, ax_window) {
                continue;
            }

            let title = ax_window
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .map(str::to_owned)
                .or_else(|| {
                    candidate
                        .title
                        .as_deref()
                        .filter(|title| !title.trim().is_empty())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| candidate.app_name.clone());

            windows.push(super::build_alt_tab_window(
                candidate.id,
                candidate.owner_pid,
                candidate.app_name,
                title,
                super::normalize_window_dimension(candidate.width),
                super::normalize_window_dimension(candidate.height),
            ));
        }

        windows
    }

    fn start_thumbnail_capture(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        thumbnail_cache: &Arc<Mutex<AltTabThumbnailCache>>,
        session_id: u64,
        window_ids: Vec<u32>,
    ) {
        if window_ids.is_empty() {
            return;
        }

        thread::spawn({
            let session = Arc::clone(session);
            let overlay_state = Arc::clone(overlay_state);
            let app_handle = Arc::clone(app_handle);
            let thumbnail_cache = Arc::clone(thumbnail_cache);

            move || {
                if !ScreenCaptureAccess::default().preflight() {
                    log::info!(
                        "Alt+Tab previews are unavailable until Screen Recording permission is granted"
                    );
                    return;
                }

                for window_id in window_ids {
                    {
                        let session = session.lock().unwrap();
                        if !session.is_active() || session.session_id() != session_id {
                            return;
                        }
                    }

                    let Some(thumbnail_data_url) = capture_window_thumbnail(window_id) else {
                        continue;
                    };

                    let update = {
                        let mut session = session.lock().unwrap();
                        if !session.is_active() || session.session_id() != session_id {
                            return;
                        }
                        if !session.update_thumbnail(window_id, Some(thumbnail_data_url.clone())) {
                            continue;
                        }

                        thumbnail_cache
                            .lock()
                            .unwrap()
                            .store(window_id, thumbnail_data_url.clone());

                        AltTabThumbnailUpdate {
                            session_id,
                            window_id,
                            thumbnail_data_url: Some(thumbnail_data_url),
                        }
                    };

                    publish_thumbnail_update(&overlay_state, &app_handle, update);
                }
            }
        });
    }

    fn start_app_icon_capture(
        session: &Arc<Mutex<AltTabSession>>,
        overlay_state: &Arc<Mutex<AltTabOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        app_icon_cache: &Arc<Mutex<AltTabAppIconCache>>,
        session_id: u64,
        owner_pids: Vec<i32>,
    ) {
        if owner_pids.is_empty() {
            return;
        }

        thread::spawn({
            let session = Arc::clone(session);
            let overlay_state = Arc::clone(overlay_state);
            let app_handle = Arc::clone(app_handle);
            let app_icon_cache = Arc::clone(app_icon_cache);

            move || {
                for owner_pid in owner_pids {
                    {
                        let session = session.lock().unwrap();
                        if !session.is_active() || session.session_id() != session_id {
                            return;
                        }
                    }

                    let Some(app_icon_data_url) = capture_app_icon_data_url(owner_pid) else {
                        continue;
                    };

                    let update = {
                        let mut session = session.lock().unwrap();
                        if !session.is_active() || session.session_id() != session_id {
                            return;
                        }
                        if !session.update_app_icon(owner_pid, Some(app_icon_data_url.clone())) {
                            continue;
                        }

                        app_icon_cache
                            .lock()
                            .unwrap()
                            .store(owner_pid, app_icon_data_url.clone());

                        AltTabAppIconUpdate {
                            session_id,
                            owner_pid,
                            app_icon_data_url: Some(app_icon_data_url),
                        }
                    };

                    publish_app_icon_update(&overlay_state, &app_handle, update);
                }
            }
        });
    }

    fn capture_window_thumbnail(window_id: u32) -> Option<String> {
        let image = cg_create_window_image(
            unsafe { CGRectNull },
            kCGWindowListOptionIncludingWindow,
            window_id,
            thumbnail_capture_image_options(),
        )?;

        match encode_thumbnail_data_url(&image) {
            Ok(data_url) => Some(data_url),
            Err(error) => {
                log::debug!(
                    "Failed to capture Alt+Tab preview for window {}: {}",
                    window_id,
                    error
                );
                None
            }
        }
    }

    fn capture_app_icon_data_url(owner_pid: i32) -> Option<String> {
        let running_app = NSRunningApplication::runningApplicationWithProcessIdentifier(owner_pid)?;
        let app_icon = running_app.icon()?;
        let tiff_data = app_icon.TIFFRepresentation()?;
        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff_data)?;
        let properties =
            NSDictionary::<objc2_app_kit::NSBitmapImageRepPropertyKey, AnyObject>::new();
        let png_data = unsafe {
            bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
        }?;

        Some(format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(png_data.to_vec())
        ))
    }

    pub(super) fn thumbnail_capture_uses_best_resolution() -> bool {
        false
    }

    fn thumbnail_capture_image_options() -> u32 {
        let mut options = kCGWindowImageBoundsIgnoreFraming;
        if thumbnail_capture_uses_best_resolution() {
            options |= kCGWindowImageBestResolution;
        }
        options
    }

    fn encode_thumbnail_data_url(image: &CGImage) -> Result<String, String> {
        let scaled = scale_thumbnail_image(image)?;
        let png_bytes = encode_png_image(&scaled)?;
        Ok(format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(png_bytes)
        ))
    }

    fn scale_thumbnail_image(image: &CGImage) -> Result<CGImage, String> {
        let source_width = image.width();
        let source_height = image.height();
        if source_width == 0 || source_height == 0 {
            return Err("Window capture returned an empty image".into());
        }

        let (target_width, target_height) = thumbnail_dimensions(source_width, source_height);
        let color_space = CGColorSpace::create_device_rgb();
        let context = CGContext::create_bitmap_context(
            None,
            target_width,
            target_height,
            8,
            0,
            &color_space,
            kCGImageAlphaPremultipliedLast,
        );

        context.set_interpolation_quality(CGInterpolationQuality::CGInterpolationQualityHigh);
        context.draw_image(
            CgRect::new(
                &CgPoint::new(0.0, 0.0),
                &CgSize::new(target_width as f64, target_height as f64),
            ),
            image,
        );

        context
            .create_image()
            .ok_or_else(|| "Failed to create scaled preview bitmap".into())
    }

    fn thumbnail_dimensions(source_width: usize, source_height: usize) -> (usize, usize) {
        let scale = (THUMBNAIL_MAX_WIDTH as f64 / source_width as f64)
            .min(THUMBNAIL_MAX_HEIGHT as f64 / source_height as f64)
            .min(1.0);

        let width = ((source_width as f64) * scale).round().max(1.0) as usize;
        let height = ((source_height as f64) * scale).round().max(1.0) as usize;

        (width, height)
    }

    fn encode_png_image(image: &CGImage) -> Result<Vec<u8>, String> {
        let width = image.width() as u32;
        let height = image.height() as u32;
        let row_bytes = image.bytes_per_row();
        let visible_row_bytes = width as usize * 4;
        if row_bytes < visible_row_bytes {
            return Err("Scaled preview row is shorter than expected".into());
        }
        let data = image.data();
        let bytes = data.bytes();

        let mut rgba = Vec::with_capacity(visible_row_bytes * height as usize);
        for row in bytes.chunks(row_bytes).take(height as usize) {
            let visible_row = row
                .get(..visible_row_bytes)
                .ok_or_else(|| "Scaled preview row is shorter than expected".to_string())?;
            rgba.extend_from_slice(visible_row);
        }

        let mut png_bytes = Vec::new();
        let mut encoder = Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_compression(Compression::Fast);

        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("Failed to create PNG header: {error}"))?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| format!("Failed to encode PNG preview: {error}"))?;
        drop(writer);

        Ok(png_bytes)
    }

    pub(super) fn is_switchable_window(
        candidate: &CgWindowEntry,
        ax_window: &AxWindowEntry,
    ) -> bool {
        if candidate.id == 0
            || candidate.owner_pid == 0
            || candidate.app_name.trim().is_empty()
            || candidate.width <= MIN_WINDOW_WIDTH
            || candidate.height <= MIN_WINDOW_HEIGHT
        {
            return false;
        }

        if ax_window.id != candidate.id || ax_window.role.as_deref() != Some(AX_ROLE_WINDOW) {
            return false;
        }

        matches!(
            ax_window.subrole.as_deref(),
            Some(AX_STANDARD_WINDOW_SUBROLE | AX_DIALOG_SUBROLE | AX_DOCUMENT_WINDOW_SUBROLE)
        )
    }

    fn ax_string_attribute(element: AXUIElementRef, attribute: CFStringRef) -> Option<String> {
        unsafe {
            let mut value: CFTypeRef = ptr::null();
            if AXUIElementCopyAttributeValue(element, attribute, &mut value) != AX_SUCCESS
                || value.is_null()
            {
                return None;
            }

            let string = cf_string_to_string(value as CFStringRef);
            CFRelease(value as *const c_void);
            string
        }
    }

    fn make_key_window(psn: &mut ProcessSerialNumber, window_id: u32) -> Result<(), String> {
        unsafe {
            let mut bytes = [0u8; 0xf8];
            bytes[0x04] = 0xf8;
            bytes[0x3a] = 0x10;
            ptr::copy_nonoverlapping(
                &window_id as *const u32 as *const u8,
                bytes.as_mut_ptr().add(0x3c),
                std::mem::size_of::<u32>(),
            );
            ptr::write_bytes(bytes.as_mut_ptr().add(0x20), 0xff, 0x10);

            bytes[0x08] = 0x01;
            if SLPSPostEventRecordTo(psn, bytes.as_mut_ptr()) != 0 {
                return Err("Failed to post first key-window event".into());
            }

            bytes[0x08] = 0x02;
            if SLPSPostEventRecordTo(psn, bytes.as_mut_ptr()) != 0 {
                return Err("Failed to post second key-window event".into());
            }

            Ok(())
        }
    }

    fn release_cf_types(values: &[CFTypeRef]) {
        for &value in values {
            if !value.is_null() {
                unsafe { CFRelease(value as *const c_void) };
            }
        }
    }

    unsafe fn dictionary_i64(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<i64> {
        let value = unsafe { CFDictionaryGetValue(dictionary, key as *const c_void) };
        if value.is_null() {
            return None;
        }

        let mut number = 0i64;
        if unsafe {
            CFNumberGetValue(
                value as CFNumberRef,
                K_CF_NUMBER_SINT64_TYPE,
                &mut number as *mut _ as *mut c_void,
            )
        } {
            Some(number)
        } else {
            None
        }
    }

    unsafe fn dictionary_string(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<String> {
        let value = unsafe { CFDictionaryGetValue(dictionary, key as *const c_void) };
        if value.is_null() {
            return None;
        }

        unsafe { cf_string_to_string(value as CFStringRef) }
    }

    fn dictionary_rect(dictionary: CFDictionaryRef, key: CFStringRef) -> Option<CGRect> {
        unsafe {
            let value = CFDictionaryGetValue(dictionary, key as *const c_void);
            if value.is_null() {
                return None;
            }

            let mut rect = CGRect::default();
            if CGRectMakeWithDictionaryRepresentation(value as CFDictionaryRef, &mut rect) {
                Some(rect)
            } else {
                None
            }
        }
    }

    unsafe fn cf_string_to_string(value: CFStringRef) -> Option<String> {
        let length = unsafe { CFStringGetLength(value) };
        let buffer_size = length.saturating_mul(4).saturating_add(1);
        let mut buffer = vec![0i8; buffer_size as usize];
        if !unsafe {
            CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer_size,
                K_CF_STRING_ENCODING_UTF8,
            )
        } {
            return None;
        }

        let text = unsafe { CStr::from_ptr(buffer.as_ptr()) };
        Some(text.to_string_lossy().into_owned())
    }

    fn cf_string(value: &str) -> Result<CFStringRef, String> {
        let c_string = CString::new(value)
            .map_err(|_| format!("Invalid CoreFoundation string value: {value}"))?;
        let string_ref = unsafe {
            CFStringCreateWithCString(
                kCFAllocatorDefault,
                c_string.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            )
        };
        if string_ref.is_null() {
            Err(format!("Failed to create CFString for {value}"))
        } else {
            Ok(string_ref)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            AxWindowEntry, CGColorSpace, CGContext, CGImage, CgPoint, CgRect, CgSize,
            CgWindowEntry, kCGImageAlphaPremultipliedLast, missing_owner_pids_for_cg_windows,
            owner_pids_in_cg_order, scale_thumbnail_image,
        };
        use std::collections::HashMap;

        fn striped_test_image() -> CGImage {
            let color_space = CGColorSpace::create_device_rgb();
            let context = CGContext::create_bitmap_context(
                None,
                2,
                2,
                8,
                0,
                &color_space,
                kCGImageAlphaPremultipliedLast,
            );

            context.set_rgb_fill_color(1.0, 0.0, 0.0, 1.0);
            context.fill_rect(CgRect::new(&CgPoint::new(0.0, 1.0), &CgSize::new(2.0, 1.0)));
            context.set_rgb_fill_color(0.0, 0.0, 1.0, 1.0);
            context.fill_rect(CgRect::new(&CgPoint::new(0.0, 0.0), &CgSize::new(2.0, 1.0)));

            context
                .create_image()
                .expect("striped test image should be creatable")
        }

        fn visible_row_rgba(image: &CGImage, row_index: usize) -> Vec<u8> {
            let row_bytes = image.bytes_per_row();
            let visible_row_bytes = image.width() * 4;
            let data = image.data();
            let bytes = data.bytes();
            let start = row_index * row_bytes;
            let end = start + visible_row_bytes;

            bytes[start..end].to_vec()
        }

        #[test]
        fn test_scale_thumbnail_image_preserves_vertical_orientation() {
            let source = striped_test_image();
            let source_first_row = visible_row_rgba(&source, 0);
            let source_last_row = visible_row_rgba(&source, source.height() - 1);

            assert_ne!(source_first_row, source_last_row);

            let scaled =
                scale_thumbnail_image(&source).expect("striped test image should scale cleanly");

            assert_eq!(visible_row_rgba(&scaled, 0), source_first_row);
            assert_eq!(
                visible_row_rgba(&scaled, scaled.height() - 1),
                source_last_row
            );
        }

        #[test]
        fn test_owner_pids_in_cg_order_keeps_first_seen_z_order() {
            let cg_windows = vec![
                CgWindowEntry {
                    id: 10,
                    owner_pid: 200,
                    app_name: "Safari".into(),
                    title: Some("Docs".into()),
                    width: 1200.0,
                    height: 900.0,
                },
                CgWindowEntry {
                    id: 11,
                    owner_pid: 100,
                    app_name: "Code".into(),
                    title: Some("engine.rs".into()),
                    width: 1400.0,
                    height: 900.0,
                },
                CgWindowEntry {
                    id: 12,
                    owner_pid: 200,
                    app_name: "Safari".into(),
                    title: Some("Mail".into()),
                    width: 1200.0,
                    height: 900.0,
                },
            ];

            assert_eq!(owner_pids_in_cg_order(&cg_windows), vec![200, 100]);
        }

        #[test]
        fn test_missing_owner_pids_for_cg_windows_only_returns_uncached_pids() {
            let cg_windows = vec![
                CgWindowEntry {
                    id: 10,
                    owner_pid: 200,
                    app_name: "Safari".into(),
                    title: Some("Docs".into()),
                    width: 1200.0,
                    height: 900.0,
                },
                CgWindowEntry {
                    id: 11,
                    owner_pid: 100,
                    app_name: "Code".into(),
                    title: Some("engine.rs".into()),
                    width: 1400.0,
                    height: 900.0,
                },
                CgWindowEntry {
                    id: 12,
                    owner_pid: 200,
                    app_name: "Safari".into(),
                    title: Some("Mail".into()),
                    width: 1200.0,
                    height: 900.0,
                },
            ];

            let ax_windows_by_pid = HashMap::from([
                (
                    200,
                    HashMap::from([
                        (
                            10,
                            AxWindowEntry {
                                id: 10,
                                title: Some("Docs".into()),
                                role: Some("AXWindow".into()),
                                subrole: Some("AXStandardWindow".into()),
                            },
                        ),
                        (
                            12,
                            AxWindowEntry {
                                id: 12,
                                title: Some("Mail".into()),
                                role: Some("AXWindow".into()),
                                subrole: Some("AXStandardWindow".into()),
                            },
                        ),
                    ]),
                ),
                (
                    100,
                    HashMap::from([(
                        99,
                        AxWindowEntry {
                            id: 99,
                            title: Some("old".into()),
                            role: Some("AXWindow".into()),
                            subrole: Some("AXStandardWindow".into()),
                        },
                    )]),
                ),
            ]);

            assert_eq!(
                missing_owner_pids_for_cg_windows(&cg_windows, &ax_windows_by_pid),
                vec![100]
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::AltTabManager;

#[cfg(not(target_os = "macos"))]
#[derive(Clone, Default)]
pub struct AltTabManager;

#[cfg(not(target_os = "macos"))]
impl AltTabManager {
    pub fn new() -> Self {
        Self
    }

    pub fn bind_app_handle(&self, _app_handle: tauri::AppHandle) {}

    pub fn dispatch(&self, _action: yorling_engine::engine::SystemAction) {}

    pub fn select_window(&self, _window_id: u32) {}

    pub fn set_hovered_window(&self, _window_id: Option<u32>) {}

    pub fn activate_window(&self, _window_id: u32) {}

    pub fn overlay_state(&self) -> AltTabOverlayState {
        AltTabOverlayState::hidden()
    }

    pub fn thumbnail_data_url(&self, _window_id: u32) -> Option<String> {
        None
    }

    pub fn app_icon_data_url(&self, _owner_pid: i32) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::platform::{
        AxWindowEntry, CgWindowEntry, merge_window_candidates, overlay_origin_for_work_area,
        thumbnail_capture_uses_best_resolution,
    };
    use super::*;
    #[cfg(target_os = "macos")]
    use std::collections::HashMap;

    fn window(id: u32, app_name: &str, title: &str) -> AltTabWindow {
        AltTabWindow {
            id,
            owner_pid: 0,
            app_name: app_name.into(),
            title: title.into(),
            width: 1280,
            height: 800,
            app_icon_data_url: None,
            thumbnail_data_url: None,
        }
    }

    fn window_with_thumbnail(
        id: u32,
        app_name: &str,
        title: &str,
        thumbnail_data_url: &str,
    ) -> AltTabWindow {
        AltTabWindow {
            thumbnail_data_url: Some(thumbnail_data_url.into()),
            ..window(id, app_name, title)
        }
    }

    fn window_with_app_icon(
        id: u32,
        owner_pid: i32,
        app_name: &str,
        title: &str,
        app_icon_data_url: &str,
    ) -> AltTabWindow {
        AltTabWindow {
            owner_pid,
            app_icon_data_url: Some(app_icon_data_url.into()),
            ..window(id, app_name, title)
        }
    }

    #[test]
    fn test_start_forward_selects_second_window() {
        let mut session = AltTabSession::default();
        let started = session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Slack", "Team Chat"),
            ],
            false,
        );

        assert!(started);
        assert!(session.is_active());
        assert_eq!(session.selected().unwrap().id, 2);
    }

    #[test]
    fn test_start_reverse_selects_last_window() {
        let mut session = AltTabSession::default();
        let started = session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Slack", "Team Chat"),
            ],
            true,
        );

        assert!(started);
        assert_eq!(session.selected().unwrap().id, 3);
    }

    #[test]
    fn test_cycle_wraps_across_windows() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Slack", "Team Chat"),
            ],
            false,
        );

        session.cycle(false);
        assert_eq!(session.selected().unwrap().id, 3);

        session.cycle(false);
        assert_eq!(session.selected().unwrap().id, 1);

        session.cycle(true);
        assert_eq!(session.selected().unwrap().id, 3);
    }

    #[test]
    fn test_commit_returns_selected_window_and_clears_state() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Slack", "Team Chat"),
            ],
            false,
        );
        session.cycle(false);

        let selected = session.commit().unwrap();
        assert_eq!(selected.id, 3);
        assert!(!session.is_active());
        assert!(session.selected().is_none());
    }

    #[test]
    fn test_activate_window_returns_target_and_hides_overlay() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Slack", "Team Chat"),
            ],
            false,
        );

        let selected = session.activate_window(1).unwrap();

        assert_eq!(selected.id, 1);
        assert!(!session.is_active());
        assert_eq!(session.overlay_state(), AltTabOverlayState::hidden());
        assert_eq!(session.overlay_selection().visible, false);
    }

    #[test]
    fn test_overlay_selection_tracks_current_window() {
        let mut session = AltTabSession::default();
        session.start(
            vec![window(1, "Code", "engine.rs"), window(2, "Safari", "Docs")],
            false,
        );

        assert_eq!(
            session.overlay_selection(),
            AltTabOverlaySelection {
                visible: true,
                session_id: 1,
                selected_index: Some(1),
                hovered_index: None,
            }
        );

        session.cancel();

        assert_eq!(
            session.overlay_selection(),
            AltTabOverlaySelection {
                visible: false,
                session_id: 0,
                selected_index: None,
                hovered_index: None,
            }
        );
    }

    #[test]
    fn test_overlay_state_strips_thumbnail_data_from_serialized_state() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window_with_thumbnail(2, "Safari", "Docs", "data:image/png;base64,thumb"),
            ],
            false,
        );

        let overlay_state = session.overlay_state();
        assert_eq!(
            overlay_state.windows[1],
            AltTabWindowLite::from(&session.windows[1])
        );
        assert_eq!(overlay_state.session_id, 1);
        assert_eq!(overlay_state.layout.rows, 1);
        assert_eq!(overlay_state.layout.tiles.len(), 2);
    }

    #[test]
    fn test_overlay_state_strips_app_icon_data_from_serialized_state() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window_with_app_icon(2, 42, "Safari", "Docs", "data:image/png;base64,icon"),
            ],
            false,
        );

        let overlay_state = session.overlay_state();
        assert_eq!(
            overlay_state.windows[1],
            AltTabWindowLite::from(&session.windows[1])
        );
    }

    #[test]
    fn test_select_window_updates_selected_target() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Ghostty", "shell"),
            ],
            false,
        );

        assert!(session.select_window(3));
        assert_eq!(session.selected().unwrap().id, 3);
        assert!(!session.select_window(999));
        assert_eq!(session.selected().unwrap().id, 3);
    }

    #[test]
    fn test_hovered_window_is_tracked_separately_from_keyboard_selection() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Ghostty", "shell"),
            ],
            false,
        );

        assert!(session.set_hovered_window(Some(3)));

        assert_eq!(session.selected().unwrap().id, 2);
        assert_eq!(session.overlay_state().selected_index, Some(1));
        assert_eq!(session.overlay_state().hovered_index, Some(2));
        assert_eq!(session.commit().unwrap().id, 2);
    }

    #[test]
    fn test_keyboard_navigation_clears_hovered_window() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Ghostty", "shell"),
            ],
            false,
        );

        assert!(session.set_hovered_window(Some(3)));
        assert!(session.cycle(false));

        assert_eq!(session.selected().unwrap().id, 3);
        assert_eq!(session.overlay_selection().selected_index, Some(2));
        assert_eq!(session.overlay_selection().hovered_index, None);
    }

    #[test]
    fn test_update_thumbnail_replaces_matching_window_only() {
        let mut session = AltTabSession::default();
        session.start(
            vec![window(1, "Code", "engine.rs"), window(2, "Safari", "Docs")],
            false,
        );

        assert!(session.update_thumbnail(1, Some("data:image/png;base64,one".into())));
        assert!(!session.update_thumbnail(999, Some("data:image/png;base64,missing".into())));

        let overlay_state = session.overlay_state();
        assert_eq!(
            session.windows[0].thumbnail_data_url.as_deref(),
            Some("data:image/png;base64,one")
        );
        assert_eq!(session.windows[1].thumbnail_data_url, None);
        assert_eq!(overlay_state.windows[0].id, 1);
        assert_eq!(overlay_state.selected_index, Some(1));
    }

    #[test]
    fn test_update_app_icon_replaces_all_matching_windows_for_same_app() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                AltTabWindow {
                    owner_pid: 77,
                    ..window(1, "Safari", "Docs")
                },
                AltTabWindow {
                    owner_pid: 77,
                    ..window(2, "Safari", "Mail")
                },
                AltTabWindow {
                    owner_pid: 88,
                    ..window(3, "Code", "engine.rs")
                },
            ],
            false,
        );

        assert!(session.update_app_icon(77, Some("data:image/png;base64,safari".into())));
        assert!(!session.update_app_icon(999, Some("data:image/png;base64,missing".into())));

        let overlay_state = session.overlay_state();
        assert_eq!(
            session.windows[0].app_icon_data_url.as_deref(),
            Some("data:image/png;base64,safari")
        );
        assert_eq!(
            session.windows[1].app_icon_data_url.as_deref(),
            Some("data:image/png;base64,safari")
        );
        assert_eq!(session.windows[2].app_icon_data_url, None);
        assert_eq!(overlay_state.windows[2].owner_pid, 88);
    }

    #[test]
    fn test_thumbnail_cache_hydrates_new_session_windows() {
        let mut cache = AltTabThumbnailCache::default();
        cache.store(2, "data:image/png;base64,two".into());

        let mut windows = vec![window(1, "Code", "engine.rs"), window(2, "Safari", "Docs")];
        cache.hydrate_windows(&mut windows);

        assert_eq!(windows[0].thumbnail_data_url, None);
        assert_eq!(
            windows[1].thumbnail_data_url.as_deref(),
            Some("data:image/png;base64,two")
        );
    }

    #[test]
    fn test_app_icon_cache_hydrates_all_windows_for_same_app() {
        let mut cache = AltTabAppIconCache::default();
        cache.store(55, "data:image/png;base64,icon".into());

        let mut windows = vec![
            AltTabWindow {
                owner_pid: 55,
                ..window(1, "Safari", "Docs")
            },
            AltTabWindow {
                owner_pid: 55,
                ..window(2, "Safari", "Mail")
            },
            AltTabWindow {
                owner_pid: 88,
                ..window(3, "Code", "engine.rs")
            },
        ];
        cache.hydrate_windows(&mut windows);

        assert_eq!(
            windows[0].app_icon_data_url.as_deref(),
            Some("data:image/png;base64,icon")
        );
        assert_eq!(
            windows[1].app_icon_data_url.as_deref(),
            Some("data:image/png;base64,icon")
        );
        assert_eq!(windows[2].app_icon_data_url, None);
    }

    #[test]
    fn test_thumbnail_cache_prioritizes_selected_window_and_skips_other_cached_windows() {
        let mut session = AltTabSession::default();
        session.start(
            vec![
                window(1, "Code", "engine.rs"),
                window(2, "Safari", "Docs"),
                window(3, "Ghostty", "shell"),
            ],
            false,
        );

        let mut cache = AltTabThumbnailCache::default();
        cache.store(2, "data:image/png;base64,two".into());
        cache.store(3, "data:image/png;base64,three".into());

        assert_eq!(cache.capture_targets(&session), vec![2, 1]);
    }

    #[test]
    fn test_app_icon_cache_dedupes_owner_pid_targets_and_skips_cached_apps() {
        let windows = vec![
            AltTabWindow {
                owner_pid: 11,
                ..window(1, "Safari", "Docs")
            },
            AltTabWindow {
                owner_pid: 11,
                ..window(2, "Safari", "Mail")
            },
            AltTabWindow {
                owner_pid: 22,
                ..window(3, "Code", "engine.rs")
            },
        ];

        let mut cache = AltTabAppIconCache::default();
        cache.store(22, "data:image/png;base64,code".into());

        assert_eq!(cache.capture_targets(&windows), vec![11]);
    }

    #[test]
    fn test_overlay_layout_scales_from_window_count() {
        let compact_windows = vec![window(1, "Code", "engine.rs"), window(2, "Safari", "Docs")];
        let dense_windows = (0..13)
            .map(|index| window(index + 1, "Code", &format!("Window {}", index + 1)))
            .collect::<Vec<_>>();

        let compact = AltTabOverlayLayout::with_limits(&compact_windows, 1800, 920);
        let dense = AltTabOverlayLayout::with_limits(&dense_windows, 1800, 920);
        let ultrawide_dense = AltTabOverlayLayout::with_limits(&dense_windows, 2500, 920);

        assert_eq!(compact.rows, 1);
        assert!(compact.panel_width >= 1200);
        assert_eq!(compact.tiles.len(), compact_windows.len());
        assert!(dense.rows > compact.rows);
        assert!(dense.panel_height <= 920);
        assert_eq!(dense.tiles.len(), dense_windows.len());
        assert!(ultrawide_dense.panel_width > dense.panel_width);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_overlay_origin_centers_using_target_panel_size() {
        let origin = overlay_origin_for_work_area(100.0, 50.0, 1600.0, 900.0, 820.0, 420.0);

        assert_eq!(origin, (490.0, 290.0));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_thumbnail_capture_disables_best_resolution_for_consistent_orientation() {
        assert!(
            !thumbnail_capture_uses_best_resolution(),
            "bestResolution captures can arrive vertically inverted for some GPU-backed windows"
        );
    }

    #[cfg(target_os = "macos")]
    fn cg_window(
        id: u32,
        owner_pid: i32,
        app_name: &str,
        title: Option<&str>,
        width: f64,
        height: f64,
    ) -> CgWindowEntry {
        CgWindowEntry {
            id,
            owner_pid,
            app_name: app_name.into(),
            title: title.map(str::to_owned),
            width,
            height,
        }
    }

    #[cfg(target_os = "macos")]
    fn ax_window(
        id: u32,
        title: Option<&str>,
        role: Option<&str>,
        subrole: Option<&str>,
    ) -> AxWindowEntry {
        AxWindowEntry {
            id,
            title: title.map(str::to_owned),
            role: role.map(str::to_owned),
            subrole: subrole.map(str::to_owned),
        }
    }

    #[cfg(target_os = "macos")]
    fn ax_map(pid: i32, windows: Vec<AxWindowEntry>) -> HashMap<i32, HashMap<u32, AxWindowEntry>> {
        let mut per_window = HashMap::new();
        for window in windows {
            per_window.insert(window.id, window);
        }

        let mut per_pid = HashMap::new();
        per_pid.insert(pid, per_window);
        per_pid
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_merge_window_candidates_rejects_small_windows() {
        let windows = merge_window_candidates(
            vec![cg_window(7, 42, "Finder", Some("Tiny"), 80.0, 40.0)],
            &ax_map(
                42,
                vec![ax_window(
                    7,
                    Some("Tiny"),
                    Some("AXWindow"),
                    Some("AXStandardWindow"),
                )],
            ),
        );

        assert!(windows.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_merge_window_candidates_requires_real_window_subrole() {
        let windows = merge_window_candidates(
            vec![cg_window(9, 99, "Chrome", Some("Tooltip"), 400.0, 240.0)],
            &ax_map(
                99,
                vec![ax_window(
                    9,
                    Some("Tooltip"),
                    Some("AXWindow"),
                    Some("AXSystemDialog"),
                )],
            ),
        );

        assert!(windows.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_merge_window_candidates_prefers_ax_title_and_dedupes() {
        let windows = merge_window_candidates(
            vec![
                cg_window(11, 7, "Safari", Some("Old Tab Title"), 1280.0, 720.0),
                cg_window(11, 7, "Safari", Some("Duplicate Entry"), 1280.0, 720.0),
            ],
            &ax_map(
                7,
                vec![ax_window(
                    11,
                    Some("Current Tab Title"),
                    Some("AXWindow"),
                    Some("AXStandardWindow"),
                )],
            ),
        );

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, "Current Tab Title");
        assert_eq!(windows[0].app_name, "Safari");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_merge_window_candidates_preserve_window_dimensions_for_overlay_layout() {
        let windows = merge_window_candidates(
            vec![cg_window(
                21,
                77,
                "Safari",
                Some("Current Tab"),
                1512.0,
                982.0,
            )],
            &ax_map(
                77,
                vec![ax_window(
                    21,
                    Some("Current Tab"),
                    Some("AXWindow"),
                    Some("AXStandardWindow"),
                )],
            ),
        );

        let serialized = serde_json::to_value(&windows[0]).unwrap();
        assert_eq!(
            serialized.get("width").and_then(|value| value.as_u64()),
            Some(1512)
        );
        assert_eq!(
            serialized.get("height").and_then(|value| value.as_u64()),
            Some(982)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_overlay_layout_width_changes_with_window_aspect_ratios() {
        let wide_windows = merge_window_candidates(
            vec![
                cg_window(31, 101, "Safari", Some("Docs"), 1728.0, 972.0),
                cg_window(32, 101, "Ghostty", Some("Shell"), 1600.0, 900.0),
            ],
            &ax_map(
                101,
                vec![
                    ax_window(31, Some("Docs"), Some("AXWindow"), Some("AXStandardWindow")),
                    ax_window(
                        32,
                        Some("Shell"),
                        Some("AXWindow"),
                        Some("AXStandardWindow"),
                    ),
                ],
            ),
        );
        let tall_windows = merge_window_candidates(
            vec![
                cg_window(41, 202, "Notes", Some("Draft"), 820.0, 1380.0),
                cg_window(42, 202, "Preview", Some("PDF"), 900.0, 1440.0),
            ],
            &ax_map(
                202,
                vec![
                    ax_window(
                        41,
                        Some("Draft"),
                        Some("AXWindow"),
                        Some("AXStandardWindow"),
                    ),
                    ax_window(42, Some("PDF"), Some("AXWindow"), Some("AXStandardWindow")),
                ],
            ),
        );

        let mut wide_session = AltTabSession::default();
        wide_session.start(wide_windows, false);
        let mut tall_session = AltTabSession::default();
        tall_session.start(tall_windows, false);

        let wide_layout = wide_session.overlay_state().layout;
        let tall_layout = tall_session.overlay_state().layout;

        assert_eq!(wide_layout.rows, 1);
        assert_eq!(tall_layout.rows, 1);
        assert!(
            wide_layout.panel_width > tall_layout.panel_width,
            "expected wide windows to occupy a wider panel than tall windows: wide={:?}, tall={:?}",
            wide_layout,
            tall_layout
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_overlay_layout_serializes_explicit_rows_and_variable_tile_widths() {
        let windows = merge_window_candidates(
            vec![
                cg_window(51, 303, "Safari", Some("Wide"), 1920.0, 960.0),
                cg_window(52, 303, "Figma", Some("Board"), 1680.0, 960.0),
                cg_window(53, 303, "Linear", Some("Tickets"), 1500.0, 900.0),
                cg_window(54, 303, "Preview", Some("Portrait"), 900.0, 1400.0),
            ],
            &ax_map(
                303,
                vec![
                    ax_window(51, Some("Wide"), Some("AXWindow"), Some("AXStandardWindow")),
                    ax_window(
                        52,
                        Some("Board"),
                        Some("AXWindow"),
                        Some("AXStandardWindow"),
                    ),
                    ax_window(
                        53,
                        Some("Tickets"),
                        Some("AXWindow"),
                        Some("AXStandardWindow"),
                    ),
                    ax_window(
                        54,
                        Some("Portrait"),
                        Some("AXWindow"),
                        Some("AXStandardWindow"),
                    ),
                ],
            ),
        );
        let mut session = AltTabSession::default();
        session.start(windows, false);

        let serialized = serde_json::to_value(session.overlay_state()).unwrap();
        let layout = serialized.get("layout").unwrap();

        assert!(
            layout.get("columns").is_none(),
            "layout should no longer expose fixed columns"
        );
        assert_eq!(layout.get("rows").and_then(|value| value.as_u64()), Some(2));

        let tiles = layout
            .get("tiles")
            .and_then(|value| value.as_array())
            .expect("layout should serialize per-tile row metadata");
        assert_eq!(tiles.len(), 4);
        assert_eq!(
            tiles[0].get("row").and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            tiles[1].get("row").and_then(|value| value.as_u64()),
            Some(0)
        );
        assert_eq!(
            tiles[2].get("row").and_then(|value| value.as_u64()),
            Some(1)
        );
        assert_eq!(
            tiles[3].get("row").and_then(|value| value.as_u64()),
            Some(1)
        );
        assert!(
            tiles[0]
                .get("width")
                .and_then(|value| value.as_u64())
                .unwrap()
                > tiles[3]
                    .get("width")
                    .and_then(|value| value.as_u64())
                    .unwrap(),
            "wide windows should receive wider tiles than portrait windows"
        );
    }
}
