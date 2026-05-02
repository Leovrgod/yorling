#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{CString, c_void};
    use std::ptr;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;

    /// Monotonic generation counter for bracket overlay show/hide commands.
    /// Used to detect stale `run_on_main_thread` closures that would otherwise
    /// resurrect the overlay after a newer hide has been issued.
    static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

    use objc2_app_kit::{
        NSColor, NSPopUpMenuWindowLevel, NSWindow, NSWindowAnimationBehavior,
        NSWindowCollectionBehavior, NSWindowStyleMask,
    };
    use serde::Serialize;
    use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindow};
    use yorling_engine::engine::{BracketPair as EngineBracketPair, SystemAction};

    const BRACKET_OVERLAY_WINDOW_LABEL: &str = "bracket-overlay";
    const BRACKET_OVERLAY_STATE_EVENT: &str = "bracket-overlay-state";
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    const K_AX_SUCCESS: i32 = 0;
    const K_AX_VALUE_CGRECT_TYPE: u32 = 3;
    const K_AX_VALUE_CFRANGE_TYPE: u32 = 4;
    const PANEL_WIDTH: f64 = 270.0;
    const PANEL_HEIGHT: f64 = 52.0;
    const PANEL_MARGIN: f64 = 12.0;
    const PANEL_OFFSET_Y: f64 = 28.0;

    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;
    type AXUIElementRef = *const c_void;
    type AXValueRef = *const c_void;

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct CFRange {
        location: isize,
        length: isize,
    }

    #[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum BracketOverlayPair {
        Round,
        Square,
        Curly,
    }

    impl From<EngineBracketPair> for BracketOverlayPair {
        fn from(value: EngineBracketPair) -> Self {
            match value {
                EngineBracketPair::Round => Self::Round,
                EngineBracketPair::Square => Self::Square,
                EngineBracketPair::Curly => Self::Curly,
            }
        }
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct BracketOverlayState {
        pub visible: bool,
        pub pending_pair: Option<BracketOverlayPair>,
        pub anchor_x: f64,
        pub anchor_y: f64,
    }

    impl BracketOverlayState {
        fn hidden() -> Self {
            Self {
                visible: false,
                pending_pair: None,
                anchor_x: 0.0,
                anchor_y: 0.0,
            }
        }
    }

    enum BracketOverlayCommand {
        Update {
            visible: bool,
            pending_pair: Option<BracketOverlayPair>,
        },
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXUIElementCopyParameterizedAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            parameter: CFTypeRef,
            value: *mut CFTypeRef,
        ) -> i32;
        fn AXValueCreate(value_type: u32, value_ptr: *const c_void) -> AXValueRef;
        fn AXValueGetValue(value: AXValueRef, value_type: u32, value_ptr: *mut c_void) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: *const c_void);
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            cstr: *const i8,
            encoding: u32,
        ) -> CFStringRef;

        static kCFAllocatorDefault: *const c_void;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *const c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
    }

    #[derive(Clone)]
    pub struct BracketOverlayManager {
        sender: mpsc::Sender<BracketOverlayCommand>,
        state: Arc<Mutex<BracketOverlayState>>,
        app_handle: Arc<Mutex<Option<AppHandle>>>,
    }

    impl BracketOverlayManager {
        pub fn new() -> Self {
            let (sender, receiver) = mpsc::channel();
            let state = Arc::new(Mutex::new(BracketOverlayState::hidden()));
            let app_handle = Arc::new(Mutex::new(None::<AppHandle>));

            thread::spawn({
                let state = Arc::clone(&state);
                let app_handle = Arc::clone(&app_handle);
                move || run_worker(receiver, state, app_handle)
            });

            Self {
                sender,
                state,
                app_handle,
            }
        }

        pub fn bind_app_handle(&self, app_handle: AppHandle) {
            *self.app_handle.lock().unwrap() = Some(app_handle.clone());
            if let Some(window) = app_handle.get_webview_window(BRACKET_OVERLAY_WINDOW_LABEL) {
                configure_overlay_window(&window);
                let _ = window.emit(BRACKET_OVERLAY_STATE_EVENT, self.overlay_state());
            }
        }

        pub fn dispatch(&self, action: SystemAction) {
            let SystemAction::BracketModeChanged {
                visible,
                pending_pair,
            } = action
            else {
                return;
            };

            let _ = self.sender.send(BracketOverlayCommand::Update {
                visible,
                pending_pair: pending_pair.map(BracketOverlayPair::from),
            });
        }

        pub fn overlay_state(&self) -> BracketOverlayState {
            self.state.lock().unwrap().clone()
        }
    }

    impl Default for BracketOverlayManager {
        fn default() -> Self {
            Self::new()
        }
    }

    fn run_worker(
        receiver: mpsc::Receiver<BracketOverlayCommand>,
        state: Arc<Mutex<BracketOverlayState>>,
        app_handle: Arc<Mutex<Option<AppHandle>>>,
    ) {
        while let Ok(command) = receiver.recv() {
            match command {
                BracketOverlayCommand::Update {
                    visible,
                    pending_pair,
                } => handle_update(&state, &app_handle, visible, pending_pair),
            }
        }
    }

    fn handle_update(
        state: &Arc<Mutex<BracketOverlayState>>,
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        visible: bool,
        pending_pair: Option<BracketOverlayPair>,
    ) {
        // Bump generation so any in-flight main-thread closures from a prior
        // command will detect that they are stale and bail out.
        let generation = OVERLAY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        let next_state = if visible {
            let (anchor_x, anchor_y) = resolve_overlay_anchor(app_handle).unwrap_or((0.0, 0.0));
            log::info!(
                "Bracket overlay: anchor=({anchor_x:.0}, {anchor_y:.0}), pair={pending_pair:?}"
            );
            BracketOverlayState {
                visible: true,
                pending_pair,
                anchor_x,
                anchor_y,
            }
        } else {
            BracketOverlayState::hidden()
        };

        // Update shared state first (used by get_bracket_overlay_state command).
        *state.lock().unwrap() = next_state.clone();

        if next_state.visible {
            // Show window FIRST so the webview is active, then emit the state
            // event. Webviews in hidden macOS windows may not process events.
            show_overlay_window(app_handle, &next_state, generation);
        } else {
            // Emit event while the window is still visible, then hide.
            emit_overlay_state(app_handle, &next_state);
            hide_overlay_window(app_handle);
        }
    }

    fn emit_overlay_state(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        next_state: &BracketOverlayState,
    ) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(BRACKET_OVERLAY_WINDOW_LABEL) {
            let _ = window.emit(BRACKET_OVERLAY_STATE_EVENT, next_state.clone());
        }
    }

    /// Tolerance (in points) for considering the mouse "near" the focused element.
    const MOUSE_PROXIMITY_TOLERANCE: f64 = 50.0;

    fn resolve_overlay_anchor(app_handle: &Arc<Mutex<Option<AppHandle>>>) -> Option<(f64, f64)> {
        // 1. Best: precise caret position from the Accessibility API.
        if let Some(caret_rect) = focused_text_caret_rect() {
            let x = caret_rect.origin.x + (caret_rect.size.width / 2.0);
            let y = caret_rect.origin.y.max(0.0);
            log::info!("Bracket overlay anchor: caret rect ({x:.0}, {y:.0})");
            return Some((x, y));
        }

        // 2. Caret unavailable — use the mouse cursor position when it is
        //    inside (or close to) the focused element.  This is a much better
        //    proxy than the element's top-center because the user usually
        //    clicked near the text caret.
        let mouse = mouse_cursor_position();
        let elem_rect = focused_element_rect();

        if let (Some((mx, my)), Some(rect)) = (mouse, elem_rect) {
            let expanded = CGRect {
                origin: CGPoint {
                    x: rect.origin.x - MOUSE_PROXIMITY_TOLERANCE,
                    y: rect.origin.y - MOUSE_PROXIMITY_TOLERANCE,
                },
                size: CGSize {
                    width: rect.size.width + MOUSE_PROXIMITY_TOLERANCE * 2.0,
                    height: rect.size.height + MOUSE_PROXIMITY_TOLERANCE * 2.0,
                },
            };
            if mx >= expanded.origin.x
                && mx <= expanded.origin.x + expanded.size.width
                && my >= expanded.origin.y
                && my <= expanded.origin.y + expanded.size.height
            {
                let x = mx.clamp(rect.origin.x, rect.origin.x + rect.size.width);
                let y = my.clamp(rect.origin.y, rect.origin.y + rect.size.height);
                log::info!("Bracket overlay anchor: mouse clamped ({x:.0}, {y:.0})");
                return Some((x, y));
            }
        }

        // 3. Mouse is absent or far from the element — fall back to the
        //    vertical+horizontal centre of the focused element.
        if let Some(rect) = elem_rect {
            let x = rect.origin.x + (rect.size.width / 2.0);
            let y = rect.origin.y + (rect.size.height / 2.0);
            log::info!("Bracket overlay anchor: element center ({x:.0}, {y:.0})");
            return Some((x, y));
        }

        // 4. No element either — try raw mouse position.
        if let Some((mx, my)) = mouse {
            log::info!("Bracket overlay anchor: raw mouse ({mx:.0}, {my:.0})");
            return Some((mx, my));
        }

        // 5. Last resort: centre of the screen.
        default_anchor(app_handle)
    }

    /// Returns the current mouse cursor position in global display coordinates
    /// (top-left origin). Used as a fallback anchor when Accessibility APIs fail.
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

    fn focused_text_caret_rect() -> Option<CGRect> {
        unsafe {
            let focused = focused_ui_element()?;
            let selected_range_attr = cf_string("AXSelectedTextRange").ok()?;
            let bounds_for_range_attr = cf_string("AXBoundsForRange").ok()?;

            let mut selected_range_value: CFTypeRef = ptr::null();
            let selected_range_status = AXUIElementCopyAttributeValue(
                focused,
                selected_range_attr,
                &mut selected_range_value,
            );
            if selected_range_status != K_AX_SUCCESS || selected_range_value.is_null() {
                release_cf_types(&[
                    focused as CFTypeRef,
                    selected_range_attr as CFTypeRef,
                    bounds_for_range_attr as CFTypeRef,
                ]);
                return None;
            }

            let mut selected_range = CFRange::default();
            if !AXValueGetValue(
                selected_range_value as AXValueRef,
                K_AX_VALUE_CFRANGE_TYPE,
                &mut selected_range as *mut _ as *mut c_void,
            ) {
                release_cf_types(&[
                    focused as CFTypeRef,
                    selected_range_value,
                    selected_range_attr as CFTypeRef,
                    bounds_for_range_attr as CFTypeRef,
                ]);
                return None;
            }

            let caret_range = CFRange {
                location: selected_range.location,
                length: 0,
            };
            let caret_range_value = AXValueCreate(
                K_AX_VALUE_CFRANGE_TYPE,
                &caret_range as *const _ as *const c_void,
            );
            if caret_range_value.is_null() {
                release_cf_types(&[
                    focused as CFTypeRef,
                    selected_range_value,
                    selected_range_attr as CFTypeRef,
                    bounds_for_range_attr as CFTypeRef,
                ]);
                return None;
            }

            let mut bounds_value: CFTypeRef = ptr::null();
            let bounds_status = AXUIElementCopyParameterizedAttributeValue(
                focused,
                bounds_for_range_attr,
                caret_range_value as CFTypeRef,
                &mut bounds_value,
            );
            if bounds_status != K_AX_SUCCESS || bounds_value.is_null() {
                release_cf_types(&[
                    focused as CFTypeRef,
                    selected_range_value,
                    caret_range_value as CFTypeRef,
                    selected_range_attr as CFTypeRef,
                    bounds_for_range_attr as CFTypeRef,
                ]);
                return None;
            }

            let mut bounds = CGRect::default();
            let ok = AXValueGetValue(
                bounds_value as AXValueRef,
                K_AX_VALUE_CGRECT_TYPE,
                &mut bounds as *mut _ as *mut c_void,
            );

            release_cf_types(&[
                focused as CFTypeRef,
                selected_range_value,
                caret_range_value as CFTypeRef,
                bounds_value,
                selected_range_attr as CFTypeRef,
                bounds_for_range_attr as CFTypeRef,
            ]);

            if ok { Some(bounds) } else { None }
        }
    }

    fn focused_element_rect() -> Option<CGRect> {
        unsafe {
            let focused = focused_ui_element()?;
            let frame = ax_rect_attribute(focused, "AXFrame");
            release_cf_types(&[focused as CFTypeRef]);
            frame
        }
    }

    unsafe fn focused_ui_element() -> Option<AXUIElementRef> {
        let focused_attr = cf_string("AXFocusedUIElement").ok()?;
        let system_wide = unsafe { AXUIElementCreateSystemWide() };
        if system_wide.is_null() {
            release_cf_types(&[focused_attr as CFTypeRef]);
            return None;
        }

        let mut focused: CFTypeRef = ptr::null();
        let status =
            unsafe { AXUIElementCopyAttributeValue(system_wide, focused_attr, &mut focused) };

        release_cf_types(&[system_wide as CFTypeRef, focused_attr as CFTypeRef]);

        if status != K_AX_SUCCESS || focused.is_null() {
            None
        } else {
            Some(focused as AXUIElementRef)
        }
    }

    unsafe fn ax_rect_attribute(element: AXUIElementRef, attribute: &str) -> Option<CGRect> {
        let attribute = cf_string(attribute).ok()?;
        let mut value: CFTypeRef = ptr::null();
        let status = unsafe { AXUIElementCopyAttributeValue(element, attribute, &mut value) };
        if status != K_AX_SUCCESS || value.is_null() {
            release_cf_types(&[attribute as CFTypeRef]);
            return None;
        }

        let mut rect = CGRect::default();
        let ok = unsafe {
            AXValueGetValue(
                value as AXValueRef,
                K_AX_VALUE_CGRECT_TYPE,
                &mut rect as *mut _ as *mut c_void,
            )
        };

        release_cf_types(&[value, attribute as CFTypeRef]);
        if ok { Some(rect) } else { None }
    }

    fn default_anchor(app_handle: &Arc<Mutex<Option<AppHandle>>>) -> Option<(f64, f64)> {
        let app_handle = app_handle.lock().unwrap().clone()?;
        let window = app_handle.get_webview_window(BRACKET_OVERLAY_WINDOW_LABEL)?;
        let monitor = window.current_monitor().ok().flatten()?;
        let scale_factor = monitor.scale_factor();
        let work_area = monitor.work_area();
        let x = work_area.position.x as f64 / scale_factor
            + (work_area.size.width as f64 / scale_factor) * 0.5;
        let y = work_area.position.y as f64 / scale_factor
            + (work_area.size.height as f64 / scale_factor) * 0.35;
        Some((x, y))
    }

    fn overlay_origin_for_anchor(
        anchor_x: f64,
        anchor_y: f64,
        work_area_x: f64,
        work_area_y: f64,
        work_area_width: f64,
        work_area_height: f64,
        panel_width: f64,
        panel_height: f64,
    ) -> (f64, f64) {
        let min_x = work_area_x + PANEL_MARGIN;
        let max_x = work_area_x + (work_area_width - panel_width - PANEL_MARGIN).max(0.0);
        let min_y = work_area_y + PANEL_MARGIN;
        let max_y = work_area_y + (work_area_height - panel_height - PANEL_MARGIN).max(0.0);

        let x = (anchor_x - panel_width / 2.0).clamp(min_x, max_x.max(min_x));
        let above_y = anchor_y - panel_height - PANEL_OFFSET_Y;
        let y = if above_y >= min_y {
            above_y
        } else {
            (anchor_y + PANEL_OFFSET_Y).clamp(min_y, max_y.max(min_y))
        };

        (x, y)
    }

    fn show_overlay_window(
        app_handle: &Arc<Mutex<Option<AppHandle>>>,
        next_state: &BracketOverlayState,
        generation: u64,
    ) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            log::warn!("Bracket overlay: no app handle bound, cannot show");
            return;
        };

        let Some(window) = app_handle.get_webview_window(BRACKET_OVERLAY_WINDOW_LABEL) else {
            log::warn!("Bracket overlay: window '{BRACKET_OVERLAY_WINDOW_LABEL}' not found");
            return;
        };

        // Position the overlay near the anchor via Tauri APIs (dispatches to main thread).
        apply_overlay_layout(&window, next_state.anchor_x, next_state.anchor_y);

        // Tell Tauri the window should be visible (updates internal bookkeeping).
        let _ = window.show();

        // Configure NSWindow properties and bring to front in a single
        // main-thread closure. Runs after set_size/set_position/show because
        // main-thread dispatches execute in FIFO order. Re-emits the state
        // event so the webview receives it while guaranteed to be active.
        let window_for_thread = window.clone();
        let state_for_emit = next_state.clone();
        let _ = window.run_on_main_thread(move || {
            // If a newer show/hide command arrived since we were scheduled,
            // this closure is stale — skip it to avoid resurrecting a hidden overlay.
            if OVERLAY_GENERATION.load(Ordering::SeqCst) != generation {
                return;
            }

            let Some(ns_window) = get_ns_window(&window_for_thread) else {
                log::warn!("Bracket overlay: failed to obtain NSWindow handle");
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
            ns_window.setIgnoresMouseEvents(true);

            // Force the window on-screen at the configured level.
            ns_window.orderFrontRegardless();

            // Emit the state event now that the window is confirmed visible,
            // ensuring the webview receives and processes it.
            let _ = window_for_thread.emit(BRACKET_OVERLAY_STATE_EVENT, state_for_emit);
        });
    }

    fn apply_overlay_layout(window: &WebviewWindow, anchor_x: f64, anchor_y: f64) {
        let panel_width = PANEL_WIDTH;
        let panel_height = PANEL_HEIGHT;

        // Pick the monitor that actually contains the anchor point, not the
        // monitor the overlay window currently sits on.  This prevents the
        // panel from being clamped to the wrong display in multi-monitor setups.
        let monitor = find_monitor_for_point(window, anchor_x, anchor_y)
            .or_else(|| window.current_monitor().ok().flatten());

        if let Some(monitor) = monitor {
            let scale_factor = monitor.scale_factor();
            let work_area = monitor.work_area();
            let work_area_x = work_area.position.x as f64 / scale_factor;
            let work_area_y = work_area.position.y as f64 / scale_factor;
            let work_area_width = work_area.size.width as f64 / scale_factor;
            let work_area_height = work_area.size.height as f64 / scale_factor;
            let (x, y) = overlay_origin_for_anchor(
                anchor_x,
                anchor_y,
                work_area_x,
                work_area_y,
                work_area_width,
                work_area_height,
                panel_width,
                panel_height,
            );

            let _ = window.set_size(LogicalSize::new(panel_width, panel_height));
            let _ = window.set_position(LogicalPosition::new(x, y));
            return;
        }

        let _ = window.set_size(LogicalSize::new(panel_width, panel_height));
        let _ = window.center();
    }

    /// Find the monitor whose work-area contains the given logical point.
    fn find_monitor_for_point(window: &WebviewWindow, x: f64, y: f64) -> Option<tauri::Monitor> {
        let monitors = window.available_monitors().ok()?;
        for m in monitors {
            let sf = m.scale_factor();
            let pos = m.position();
            let size = m.size();
            let lx = pos.x as f64 / sf;
            let ly = pos.y as f64 / sf;
            let lw = size.width as f64 / sf;
            let lh = size.height as f64 / sf;
            if x >= lx && x < lx + lw && y >= ly && y < ly + lh {
                return Some(m);
            }
        }
        None
    }

    fn hide_overlay_window(app_handle: &Arc<Mutex<Option<AppHandle>>>) {
        let Some(app_handle) = app_handle.lock().unwrap().clone() else {
            return;
        };
        if let Some(window) = app_handle.get_webview_window(BRACKET_OVERLAY_WINDOW_LABEL) {
            let _ = window.hide();
            log::info!("Bracket overlay: hidden");
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
            ns_window.setIgnoresMouseEvents(true);
        });
    }

    fn get_ns_window(window: &WebviewWindow) -> Option<&NSWindow> {
        let ns_window_ptr = match window.ns_window() {
            Ok(handle) => handle,
            Err(error) => {
                log::warn!("Failed to access native bracket overlay window: {error}");
                return None;
            }
        };

        unsafe { (ns_window_ptr as *mut NSWindow).as_ref() }
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

    fn release_cf_types(values: &[CFTypeRef]) {
        for &value in values {
            if !value.is_null() {
                unsafe { CFRelease(value as *const c_void) };
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::overlay_origin_for_anchor;

        #[test]
        fn test_overlay_origin_prefers_above_caret_and_centers_horizontally() {
            let (x, y) =
                overlay_origin_for_anchor(420.0, 240.0, 0.0, 0.0, 1200.0, 800.0, 296.0, 76.0);
            assert_eq!(x, 272.0);
            assert_eq!(y, 136.0);
        }

        #[test]
        fn test_overlay_origin_flips_below_when_above_would_overflow() {
            let (x, y) =
                overlay_origin_for_anchor(120.0, 40.0, 0.0, 0.0, 1200.0, 800.0, 296.0, 76.0);
            assert_eq!(x, 12.0);
            assert_eq!(y, 68.0);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use serde::Serialize;
    use tauri::AppHandle;
    use yorling_engine::engine::SystemAction;

    #[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum BracketOverlayPair {
        Round,
        Square,
        Curly,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct BracketOverlayState {
        pub visible: bool,
        pub pending_pair: Option<BracketOverlayPair>,
        pub anchor_x: f64,
        pub anchor_y: f64,
    }

    #[derive(Clone, Default)]
    pub struct BracketOverlayManager;

    impl BracketOverlayManager {
        pub fn new() -> Self {
            Self
        }

        pub fn bind_app_handle(&self, _app_handle: AppHandle) {}

        pub fn dispatch(&self, _action: SystemAction) {}

        pub fn overlay_state(&self) -> BracketOverlayState {
            BracketOverlayState {
                visible: false,
                pending_pair: None,
                anchor_x: 0.0,
                anchor_y: 0.0,
            }
        }
    }
}

pub use imp::{BracketOverlayManager, BracketOverlayState};
