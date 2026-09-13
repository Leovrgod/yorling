//! macOS keyboard interceptor using CGEventTap.
//!
//! Runs a CGEventTap on a dedicated thread with its own CFRunLoop.
//! The tap intercepts key events and passes them through the MappingEngine.

use std::os::raw::c_void;
use std::ptr;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
    mpsc,
};
use std::thread;

use crate::mouse_motion::{
    DisplayBounds, MouseMotionController, MouseMoveIntent, set_input_thread_priority,
};
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::rc::autoreleasepool;
use yorling_core::keycode::{self as core_keycode, VirtualKeyCode};
use yorling_engine::engine::{EngineAction, MappingEngine, SyntheticKey, SystemAction};

/// Wrapper to allow sending raw CFRunLoopRef across threads.
/// SAFETY: CFRunLoopRef is safe to send between threads; CFRunLoopStop
/// is documented as thread-safe by Apple.
struct SendablePtr(*mut c_void);
unsafe impl Send for SendablePtr {}
unsafe impl Sync for SendablePtr {}

// ── CGEvent constants ──

const K_CG_HID_EVENT_TAP: u32 = 0; // kCGHIDEventTap
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0; // kCGHeadInsertEventTap
const K_CG_EVENT_TAP_OPTION_DEFAULT: u32 = 0x00000000;

const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
const K_CG_EVENT_SYSTEM_DEFINED: u32 = 14;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFFFFFE;

const K_CG_EVENT_LEFT_MOUSE_DOWN: u32 = 1;
const K_CG_EVENT_RIGHT_MOUSE_DOWN: u32 = 3;
const K_CG_EVENT_OTHER_MOUSE_DOWN: u32 = 25;

// CGEventField
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
const K_CG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8; // CGEventTypes.h; 11 is scroll delta
const K_CG_EVENT_SOURCE_USER_DATA: u32 = 42;
// Undocumented CGEvent payload slots used by system-defined/media-key events.
const K_CG_EVENT_DATA1: u32 = 149;

// Self-injection marker to prevent processing our own synthetic events
const SELF_INJECTED_TAG: i64 = 0x594F524C; // "YORL"
const K_MUSIC_SYSTEM_KEY_STATE_UP: u8 = 0xB;

// ── CoreGraphics FFI ──

type CGEventTapProxy = *mut c_void;
type CGEventRef = *mut c_void;
type CFMachPortRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFStringRef = *const c_void;

type CGEventTapCallBack = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);

    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventSetIntegerValueField(event: CGEventRef, field: u32, value: i64);

    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);

    fn CGEventCreateKeyboardEvent(
        source: *const c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;

    fn CGEventPost(tap_location: u32, event: CGEventRef);
    fn CGEventTapPostEvent(proxy: CGEventTapProxy, event: CGEventRef);
    fn CGEventGetTimestamp(event: CGEventRef) -> u64;
    fn CGEventSetTimestamp(event: CGEventRef, timestamp: u64);
    fn CGEventSetType(event: CGEventRef, event_type: u32);

    // Mouse event support
    fn CGEventCreate(source: *const c_void) -> CGEventRef;
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn CGEventCreateMouseEvent(
        source: *const c_void,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    fn CGGetActiveDisplayList(max_displays: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

// Mouse event type constants
const K_CG_EVENT_MOUSE_MOVED: u32 = 5;
const K_CG_EVENT_LEFT_MOUSE_UP: u32 = 2;
const K_CG_EVENT_RIGHT_MOUSE_UP: u32 = 4;
const K_CG_EVENT_OTHER_MOUSE_UP: u32 = 26;

// Scroll wheel event constants
const K_CG_EVENT_SCROLL_WHEEL: u32 = 22;
const K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1: u32 = 11;

/// CGPoint for mouse position
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

pub(crate) fn get_active_display_bounds() -> Vec<DisplayBounds> {
    let mut displays = [0_u32; 32];
    let mut count = 0;
    // SAFETY: buffer and count are valid, and the buffer length matches max_displays.
    let result =
        unsafe { CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) };
    if result != 0 {
        return Vec::new();
    }
    displays[..(count as usize).min(displays.len())]
        .iter()
        .filter_map(|&id| {
            let bounds = unsafe { CGDisplayBounds(id) };
            if bounds.size.width < 1.0 || bounds.size.height < 1.0 {
                return None;
            }
            Some(DisplayBounds {
                id,
                min_x: bounds.origin.x,
                min_y: bounds.origin.y,
                max_x: bounds.origin.x + bounds.size.width - 1.0,
                max_y: bounds.origin.y + bounds.size.height - 1.0,
            })
        })
        .collect()
}

/// Lines to scroll per key event.
const MOUSE_SCROLL_LINES: i32 = 3;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: *const c_void,
        port: CFMachPortRef,
        order: i64,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);

    static kCFRunLoopCommonModes: CFStringRef;
    static kCFAllocatorDefault: *const c_void;
}

// ── Accessibility check ──

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> u8;
}

/// Shared state passed to the CGEventTap callback via user_info pointer.
struct TapContext {
    engine: Mutex<MappingEngine>,
    enabled: AtomicBool,
    system_action_handler: Mutex<Option<Arc<dyn Fn(SystemAction) + Send + Sync>>>,
    music_key_handler: Mutex<Option<Arc<dyn Fn(&'static str, bool) + Send + Sync>>>,
    music_native_keys_suppressed: AtomicBool,
    mouse_motion: MouseMotionController,
    /// CFMachPortRef for re-enabling the tap on timeout.
    /// Stored as AtomicPtr so the callback can access it without locking.
    tap_ref: AtomicPtr<c_void>,
}

pub struct KeyboardInterceptor {
    context: Arc<TapContext>,
    running: Arc<AtomicBool>,
    stop_requested: Arc<AtomicBool>,
    event_count: Arc<AtomicU64>,
    run_loop: Arc<Mutex<Option<SendablePtr>>>,
}

impl KeyboardInterceptor {
    pub fn new() -> Self {
        Self {
            context: Arc::new(TapContext {
                engine: Mutex::new(MappingEngine::new()),
                enabled: AtomicBool::new(false),
                system_action_handler: Mutex::new(None),
                music_key_handler: Mutex::new(None),
                music_native_keys_suppressed: AtomicBool::new(false),
                mouse_motion: MouseMotionController::new(),
                tap_ref: AtomicPtr::new(ptr::null_mut()),
            }),
            running: Arc::new(AtomicBool::new(false)),
            stop_requested: Arc::new(AtomicBool::new(false)),
            event_count: Arc::new(AtomicU64::new(0)),
            run_loop: Arc::new(Mutex::new(None)),
        }
    }

    /// Check if Accessibility permission is granted.
    pub fn has_accessibility_permission() -> bool {
        unsafe { AXIsProcessTrusted() != 0 }
    }

    /// Request Accessibility permission by showing the system prompt dialog.
    pub fn request_accessibility_permission() -> bool {
        let prompt_key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
        let prompt_value = CFBoolean::true_value();
        let options = CFDictionary::from_CFType_pairs(&[(prompt_key, prompt_value)]);

        unsafe {
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void) != 0
        }
    }

    /// Start the keyboard interceptor on a dedicated thread.
    /// Blocks until the CGEventTap is successfully created or returns an error.
    pub fn start(&self) -> Result<(), String> {
        self.start_with_enabled(true)
    }

    pub fn start_disabled(&self) -> Result<(), String> {
        self.start_with_enabled(false)
    }

    fn start_with_enabled(&self, initially_enabled: bool) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Interceptor is already running".into());
        }

        if !Self::has_accessibility_permission() {
            return Err("Accessibility permission not granted. Please enable it in System Settings → Privacy & Security → Accessibility.".into());
        }

        let context = Arc::clone(&self.context);
        let running = Arc::clone(&self.running);
        let stop_requested = Arc::clone(&self.stop_requested);
        let event_count = Arc::clone(&self.event_count);
        let run_loop_store = Arc::clone(&self.run_loop);

        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

        running.store(true, Ordering::SeqCst);
        stop_requested.store(false, Ordering::SeqCst);
        context.enabled.store(initially_enabled, Ordering::SeqCst);

        thread::spawn(move || {
            set_input_thread_priority();
            unsafe {
                let mask: u64 = (1 << K_CG_EVENT_KEY_DOWN)
                    | (1 << K_CG_EVENT_KEY_UP)
                    | (1 << K_CG_EVENT_FLAGS_CHANGED)
                    | (1 << K_CG_EVENT_SYSTEM_DEFINED)
                    | (1 << K_CG_EVENT_LEFT_MOUSE_DOWN)
                    | (1 << K_CG_EVENT_RIGHT_MOUSE_DOWN)
                    | (1 << K_CG_EVENT_OTHER_MOUSE_DOWN);

                let user_data = Box::new((Arc::clone(&context), Arc::clone(&event_count)));
                let user_info = Box::into_raw(user_data) as *mut c_void;

                let tap = CGEventTapCreate(
                    K_CG_HID_EVENT_TAP,
                    K_CG_HEAD_INSERT_EVENT_TAP,
                    K_CG_EVENT_TAP_OPTION_DEFAULT,
                    mask,
                    event_tap_callback,
                    user_info,
                );

                if tap.is_null() {
                    running.store(false, Ordering::SeqCst);
                    let _ = Box::from_raw(user_info as *mut (Arc<TapContext>, Arc<AtomicU64>));
                    let _ = startup_tx.send(Err(
                        "Failed to create CGEventTap. Check Accessibility permission.".into(),
                    ));
                    return;
                }

                context.tap_ref.store(tap, Ordering::SeqCst);

                let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0);
                if source.is_null() {
                    running.store(false, Ordering::SeqCst);
                    context.tap_ref.store(ptr::null_mut(), Ordering::SeqCst);
                    CFRelease(tap as *const c_void);
                    let _ = Box::from_raw(user_info as *mut (Arc<TapContext>, Arc<AtomicU64>));
                    let _ = startup_tx
                        .send(Err("Failed to create run loop source for event tap.".into()));
                    return;
                }

                let rl = CFRunLoopGetCurrent();
                CFRetain(rl as *const c_void);
                *run_loop_store.lock().unwrap() = Some(SendablePtr(rl));

                if stop_requested.load(Ordering::SeqCst) {
                    log::info!("Stop requested before entering run loop, aborting");
                    CGEventTapEnable(tap, false);
                    context.tap_ref.store(ptr::null_mut(), Ordering::SeqCst);
                    CFRelease(source as *const c_void);
                    CFRelease(tap as *const c_void);
                    let stored_rl = run_loop_store.lock().unwrap().take();
                    if let Some(rl_ptr) = stored_rl {
                        CFRelease(rl_ptr.0 as *const c_void);
                    }
                    let _ = Box::from_raw(user_info as *mut (Arc<TapContext>, Arc<AtomicU64>));
                    running.store(false, Ordering::SeqCst);
                    let _ = startup_tx.send(Err("Stopped before start completed.".into()));
                    return;
                }

                CFRunLoopAddSource(rl, source, kCFRunLoopCommonModes);
                CGEventTapEnable(tap, true);

                log::info!("Keyboard interceptor started");
                let _ = startup_tx.send(Ok(()));

                CFRunLoopRun();

                // Cleanup
                CGEventTapEnable(tap, false);
                context.tap_ref.store(ptr::null_mut(), Ordering::SeqCst);
                CFRelease(source as *const c_void);
                CFRelease(tap as *const c_void);
                let _ = Box::from_raw(user_info as *mut (Arc<TapContext>, Arc<AtomicU64>));

                let stored_rl = run_loop_store.lock().unwrap().take();
                if let Some(rl_ptr) = stored_rl {
                    CFRelease(rl_ptr.0 as *const c_void);
                }

                running.store(false, Ordering::SeqCst);
                log::info!("Keyboard interceptor stopped");
            }
        });

        // Wait for thread to confirm startup success or failure
        match startup_rx.recv() {
            Ok(Ok(())) => {
                self.context.mouse_motion.start();
                Ok(())
            }
            Ok(Err(err)) => Err(err),
            Err(_) => {
                self.running.store(false, Ordering::SeqCst);
                Err("Interceptor thread exited unexpectedly".into())
            }
        }
    }

    /// Stop the keyboard interceptor.
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
        self.context.enabled.store(false, Ordering::SeqCst);
        self.context.mouse_motion.stop();

        // Release any stuck arrow keys before stopping
        let releases = self
            .context
            .engine
            .lock()
            .map(|mut engine| engine.set_enabled(false))
            .unwrap_or_default();
        for sk in releases {
            emit_synthetic_key(sk.keycode, sk.key_down, sk.extra_flags);
        }

        if let Some(ref rl) = *self.run_loop.lock().unwrap() {
            unsafe {
                CFRunLoopStop(rl.0);
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.context.enabled.store(enabled, Ordering::SeqCst);
        let releases = self
            .context
            .engine
            .lock()
            .map(|mut engine| engine.set_enabled(enabled))
            .unwrap_or_default();
        for sk in releases {
            emit_synthetic_key(sk.keycode, sk.key_down, sk.extra_flags);
        }
        if !enabled {
            self.context.mouse_motion.stop_motion();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.context.enabled.load(Ordering::SeqCst)
    }

    pub fn cancel_alt_tab_session(&self) {
        if let Ok(mut engine) = self.context.engine.lock() {
            engine.cancel_alt_tab_session();
        }
    }

    pub fn set_system_action_handler<F>(&self, handler: F)
    where
        F: Fn(SystemAction) + Send + Sync + 'static,
    {
        *self.context.system_action_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    pub fn set_music_key_handler<F>(&self, handler: F)
    where
        F: Fn(&'static str, bool) + Send + Sync + 'static,
    {
        *self.context.music_key_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    pub fn set_music_native_keys_suppressed(&self, suppressed: bool) {
        self.context
            .music_native_keys_suppressed
            .store(suppressed, Ordering::SeqCst);
    }

    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::SeqCst)
    }
}

impl Default for KeyboardInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

fn modifier_flag_for_keycode(keycode: u16) -> Option<u64> {
    match VirtualKeyCode::from_raw(keycode) {
        Some(VirtualKeyCode::Shift | VirtualKeyCode::RightShift) => Some(core_keycode::FLAG_SHIFT),
        Some(VirtualKeyCode::Control | VirtualKeyCode::RightControl) => {
            Some(core_keycode::FLAG_CONTROL)
        }
        Some(VirtualKeyCode::Option | VirtualKeyCode::RightOption) => {
            Some(core_keycode::FLAG_OPTION)
        }
        Some(VirtualKeyCode::Command | VirtualKeyCode::RightCommand) => {
            Some(core_keycode::FLAG_COMMAND)
        }
        _ => None,
    }
}

/// Create an owned keyboard event. The caller releases it, or transfers it to
/// CoreGraphics by returning it from the tap callback.
fn create_synthetic_key(keycode: u16, key_down: bool, flags: u64) -> CGEventRef {
    unsafe {
        let event = CGEventCreateKeyboardEvent(ptr::null(), keycode, key_down);
        if !event.is_null() {
            CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, SELF_INJECTED_TAG);
            CGEventSetFlags(event, flags);
        }
        event
    }
}

/// Outside a tap callback (e.g. shutdown), inject at the HID entry point.
fn emit_synthetic_key(keycode: u16, key_down: bool, flags: u64) {
    let event = create_synthetic_key(keycode, key_down, flags);
    if !event.is_null() {
        unsafe {
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event.cast());
        }
    }
}

/// Preserve stream order: preceding sequence events go after this tap, and the
/// final event replaces the input. Reposting to the HID head would put mapped
/// key-ups behind already queued modifier releases and revisit upstream taps.
fn emit_keys_from_tap(
    proxy: CGEventTapProxy,
    original: CGEventRef,
    keys: &[SyntheticKey],
    flags: u64,
) -> CGEventRef {
    replace_key_events(original, keys, flags, |event| unsafe {
        CGEventTapPostEvent(proxy, event)
    })
}

fn replace_key_events(
    original: CGEventRef,
    keys: &[SyntheticKey],
    flags: u64,
    mut post_before_return: impl FnMut(CGEventRef),
) -> CGEventRef {
    let mut last_event: CGEventRef = ptr::null_mut();
    for key in keys {
        if !last_event.is_null() {
            post_before_return(last_event);
            unsafe {
                CFRelease(last_event.cast());
            }
        }
        let flags = key.extra_flags | if key.preserve_flags { flags } else { 0 };
        last_event = create_synthetic_key(key.keycode, key.key_down, flags);
        if keys.len() == 1 && !last_event.is_null() {
            unsafe {
                CGEventSetTimestamp(last_event, CGEventGetTimestamp(original));
                if key.key_down {
                    CGEventSetIntegerValueField(
                        last_event,
                        K_CG_KEYBOARD_EVENT_AUTOREPEAT,
                        CGEventGetIntegerValueField(original, K_CG_KEYBOARD_EVENT_AUTOREPEAT),
                    );
                }
            }
        }
    }
    // CoreGraphics releases a newly returned event along with the original.
    last_event
}

/// Get the current mouse cursor position.
pub(crate) fn get_cursor_position() -> CGPoint {
    unsafe {
        let dummy = CGEventCreate(ptr::null());
        if dummy.is_null() {
            return CGPoint { x: 0.0, y: 0.0 };
        }
        let pos = CGEventGetLocation(dummy);
        CFRelease(dummy as *const c_void);
        pos
    }
}

/// Move the mouse cursor to an absolute position.
pub(crate) fn emit_mouse_move_to(position: CGPoint) {
    unsafe {
        let event = CGEventCreateMouseEvent(
            ptr::null(),
            K_CG_EVENT_MOUSE_MOVED,
            position,
            0, // button (ignored for mouse moved)
        );
        if !event.is_null() {
            CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, SELF_INJECTED_TAG);
            CGEventPost(K_CG_HID_EVENT_TAP, event);
            CFRelease(event as *const c_void);
        }
    }
}

/// Emit a mouse click (down + up) at the current cursor position.
fn emit_mouse_click(proxy: CGEventTapProxy, button: yorling_engine::engine::MouseButton) {
    use yorling_engine::engine::MouseButton;
    let pos = get_cursor_position();
    let (down_type, up_type, cg_button) = match button {
        MouseButton::Left => (K_CG_EVENT_LEFT_MOUSE_DOWN, K_CG_EVENT_LEFT_MOUSE_UP, 0u32),
        MouseButton::Right => (K_CG_EVENT_RIGHT_MOUSE_DOWN, K_CG_EVENT_RIGHT_MOUSE_UP, 1u32),
        MouseButton::Back => (K_CG_EVENT_OTHER_MOUSE_DOWN, K_CG_EVENT_OTHER_MOUSE_UP, 3u32),
    };
    unsafe {
        // Mouse down
        let down = CGEventCreateMouseEvent(ptr::null(), down_type, pos, cg_button);
        if !down.is_null() {
            CGEventSetIntegerValueField(down, K_CG_EVENT_SOURCE_USER_DATA, SELF_INJECTED_TAG);
            CGEventTapPostEvent(proxy, down);
            CFRelease(down as *const c_void);
        }
        // Mouse up
        let up = CGEventCreateMouseEvent(ptr::null(), up_type, pos, cg_button);
        if !up.is_null() {
            CGEventSetIntegerValueField(up, K_CG_EVENT_SOURCE_USER_DATA, SELF_INJECTED_TAG);
            CGEventTapPostEvent(proxy, up);
            CFRelease(up as *const c_void);
        }
    }
}

/// Emit a mouse scroll wheel event with the given line delta.
/// Positive = scroll up (page goes up), negative = scroll down (page goes down).
fn emit_mouse_scroll(proxy: CGEventTapProxy, lines: i32) {
    unsafe {
        let event = CGEventCreate(ptr::null());
        if event.is_null() {
            return;
        }
        CGEventSetType(event, K_CG_EVENT_SCROLL_WHEEL);
        CGEventSetIntegerValueField(event, K_CG_SCROLL_WHEEL_EVENT_DELTA_AXIS_1, lines as i64);
        CGEventSetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA, SELF_INJECTED_TAG);
        CGEventTapPostEvent(proxy, event);
        CFRelease(event as *const c_void);
    }
}

unsafe extern "C" fn event_tap_callback(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    autoreleasepool(|_| unsafe { handle_tapped_event(proxy, event_type, event, user_info) })
}

unsafe fn handle_tapped_event(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    // Handle tap disabled (timeout or user-input related) — re-enable it
    if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT || event_type == 0xFFFFFFFF {
        let data = unsafe { &*(user_info as *const (Arc<TapContext>, Arc<AtomicU64>)) };
        let context = &data.0;
        context.mouse_motion.stop_motion();
        let (releases, bracket_action) = context
            .engine
            .lock()
            .map(|mut engine| {
                let bracket = engine.cancel_bracket_mode();
                (engine.reset_input_state(), bracket)
            })
            .unwrap_or_default();
        // Even a tap used only for music, or temporarily disabled mappings, must
        // recover its event stream. The normal handler still honors enabled state.
        let tap = context.tap_ref.load(Ordering::SeqCst);
        if !tap.is_null() {
            log::warn!(
                "CGEventTap interrupted (type=0x{:X}); reset input state and re-enabled",
                event_type
            );
            unsafe { CGEventTapEnable(tap, true) };
        }
        for key in releases {
            let release = create_synthetic_key(key.keycode, false, key.extra_flags);
            if !release.is_null() {
                unsafe {
                    CGEventTapPostEvent(proxy, release);
                    CFRelease(release.cast());
                }
            }
        }
        if let Some(action) = bracket_action {
            forward_system_action(context, action);
        }
        forward_system_action(context, SystemAction::AltTabCancel);
        return event;
    }

    let data = unsafe { &*(user_info as *const (Arc<TapContext>, Arc<AtomicU64>)) };
    let context = &data.0;

    // Mouse-down events: cancel bracket/mouse mode if active, then pass through.
    // Skip self-injected mouse events (from N/M clicks) to avoid self-cancellation.
    if event_type == K_CG_EVENT_LEFT_MOUSE_DOWN
        || event_type == K_CG_EVENT_RIGHT_MOUSE_DOWN
        || event_type == K_CG_EVENT_OTHER_MOUSE_DOWN
    {
        let user_data = unsafe { CGEventGetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA) };
        if user_data == SELF_INJECTED_TAG {
            return event;
        }
        if context.enabled.load(Ordering::Relaxed) {
            if let Ok(mut engine) = context.engine.lock() {
                let bracket_action = engine.cancel_bracket_mode();
                let mouse_action = engine.cancel_mouse_mode();
                drop(engine);
                if let Some(action) = bracket_action {
                    dispatch_system_action(context, proxy, action);
                }
                if let Some(action) = mouse_action {
                    dispatch_system_action(context, proxy, action);
                }
            }
        }
        return event;
    }

    if context.music_native_keys_suppressed.load(Ordering::Relaxed)
        && event_type == K_CG_EVENT_SYSTEM_DEFINED
    {
        if let Some((code, key_down)) = decode_music_system_defined_event(event) {
            if let Some(code) = code {
                dispatch_music_key_event(context, code, key_down);
            }
        }
        return ptr::null_mut();
    }

    // Only process keyboard events and modifier state changes.
    if event_type != K_CG_EVENT_KEY_DOWN
        && event_type != K_CG_EVENT_KEY_UP
        && event_type != K_CG_EVENT_FLAGS_CHANGED
    {
        return event;
    }

    let event_counter = &data.1;

    // Skip self-injected events
    let user_data = unsafe { CGEventGetIntegerValueField(event, K_CG_EVENT_SOURCE_USER_DATA) };
    if user_data == SELF_INJECTED_TAG {
        return event;
    }

    let keycode = unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) } as u16;
    let flags = unsafe { CGEventGetFlags(event) };
    let key_down = match event_type {
        K_CG_EVENT_KEY_DOWN => true,
        K_CG_EVENT_KEY_UP => false,
        K_CG_EVENT_FLAGS_CHANGED => modifier_flag_for_keycode(keycode)
            .map(|modifier_flag| flags & modifier_flag != 0)
            .unwrap_or(true),
        _ => unreachable!(),
    };
    let is_autorepeat = event_type == K_CG_EVENT_KEY_DOWN
        && unsafe { CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT) } != 0;

    if context.music_native_keys_suppressed.load(Ordering::Relaxed) {
        if let Some(code) = music_dom_code_for_keycode(keycode) {
            dispatch_music_key_event(context, code, key_down);
        }
        return ptr::null_mut();
    }

    // Check if enabled
    if !context.enabled.load(Ordering::Relaxed) {
        return event;
    }

    event_counter.fetch_add(1, Ordering::Relaxed);

    // Process through the engine
    let (action, mouse_mode_exited) = {
        let mut engine = match context.engine.lock() {
            Ok(e) => e,
            Err(_) => return event, // Poisoned mutex — pass through
        };
        let was_mouse_mode_active = engine.is_mouse_mode_active();
        let action = engine.process_key(keycode, key_down, is_autorepeat, flags);
        let mouse_mode_exited = was_mouse_mode_active && !engine.is_mouse_mode_active();
        (action, mouse_mode_exited)
    };

    if mouse_mode_exited {
        context.mouse_motion.stop_motion();
    }

    match action {
        EngineAction::PassThrough => event,
        EngineAction::Suppress => ptr::null_mut(),
        EngineAction::Emit(keys) => emit_keys_from_tap(proxy, event, &keys, flags),
        EngineAction::EmitAndSystem(keys, sys_action) => {
            let replacement = emit_keys_from_tap(proxy, event, &keys, flags);
            dispatch_system_action(context, proxy, sys_action);
            replacement
        }
        EngineAction::System(sys_action) => {
            dispatch_system_action(context, proxy, sys_action);
            ptr::null_mut()
        }
    }
}

/// Handle a SystemAction: mouse actions are executed directly here;
/// everything else is forwarded to the external system_action_handler.
fn dispatch_system_action(context: &TapContext, proxy: CGEventTapProxy, action: SystemAction) {
    use yorling_engine::engine::MouseScrollDirection;
    match action {
        SystemAction::MouseMove { x, y, fast } => {
            context
                .mouse_motion
                .update_intent(MouseMoveIntent { x, y, fast });
        }
        SystemAction::MouseClick { button } => {
            emit_mouse_click(proxy, button);
        }
        SystemAction::MouseScroll { direction } => {
            let lines = match direction {
                MouseScrollDirection::Up => MOUSE_SCROLL_LINES,
                MouseScrollDirection::Down => -MOUSE_SCROLL_LINES,
            };
            emit_mouse_scroll(proxy, lines);
        }
        SystemAction::MouseModeChanged { active } => {
            if !active {
                context.mouse_motion.stop_motion();
            }
            forward_system_action(context, SystemAction::MouseModeChanged { active });
        }
        other => {
            forward_system_action(context, other);
        }
    }
}

fn forward_system_action(context: &TapContext, action: SystemAction) {
    let handler = context
        .system_action_handler
        .lock()
        .ok()
        .and_then(|handler| handler.clone());
    if let Some(handler) = handler {
        handler(action);
    }
}

fn music_dom_code_for_keycode(keycode: u16) -> Option<&'static str> {
    match VirtualKeyCode::from_raw(keycode) {
        Some(VirtualKeyCode::F1) => Some("F1"),
        Some(VirtualKeyCode::F2) => Some("F2"),
        Some(VirtualKeyCode::F3) => Some("F3"),
        Some(VirtualKeyCode::F4) => Some("F4"),
        Some(VirtualKeyCode::F5) => Some("F5"),
        Some(VirtualKeyCode::F6) => Some("F6"),
        Some(VirtualKeyCode::F7) => Some("F7"),
        Some(VirtualKeyCode::F8) => Some("F8"),
        Some(VirtualKeyCode::F9) => Some("F9"),
        Some(VirtualKeyCode::F10) => Some("F10"),
        Some(VirtualKeyCode::F11) => Some("F11"),
        Some(VirtualKeyCode::F12) => Some("F12"),
        Some(VirtualKeyCode::Grave) => Some("Backquote"),
        Some(VirtualKeyCode::Key1) => Some("Digit1"),
        Some(VirtualKeyCode::Key2) => Some("Digit2"),
        Some(VirtualKeyCode::Key3) => Some("Digit3"),
        Some(VirtualKeyCode::Key4) => Some("Digit4"),
        Some(VirtualKeyCode::Key5) => Some("Digit5"),
        Some(VirtualKeyCode::Key6) => Some("Digit6"),
        Some(VirtualKeyCode::Key7) => Some("Digit7"),
        Some(VirtualKeyCode::Key8) => Some("Digit8"),
        Some(VirtualKeyCode::Key9) => Some("Digit9"),
        Some(VirtualKeyCode::Key0) => Some("Digit0"),
        Some(VirtualKeyCode::Minus) => Some("Minus"),
        Some(VirtualKeyCode::Equal) => Some("Equal"),
        Some(VirtualKeyCode::Delete) => Some("Backspace"),
        Some(VirtualKeyCode::Tab) => Some("Tab"),
        Some(VirtualKeyCode::Q) => Some("KeyQ"),
        Some(VirtualKeyCode::W) => Some("KeyW"),
        Some(VirtualKeyCode::E) => Some("KeyE"),
        Some(VirtualKeyCode::R) => Some("KeyR"),
        Some(VirtualKeyCode::T) => Some("KeyT"),
        Some(VirtualKeyCode::Y) => Some("KeyY"),
        Some(VirtualKeyCode::U) => Some("KeyU"),
        Some(VirtualKeyCode::I) => Some("KeyI"),
        Some(VirtualKeyCode::O) => Some("KeyO"),
        Some(VirtualKeyCode::P) => Some("KeyP"),
        Some(VirtualKeyCode::LeftBracket) => Some("BracketLeft"),
        Some(VirtualKeyCode::RightBracket) => Some("BracketRight"),
        Some(VirtualKeyCode::Backslash) => Some("Backslash"),
        Some(VirtualKeyCode::CapsLock) => Some("CapsLock"),
        Some(VirtualKeyCode::A) => Some("KeyA"),
        Some(VirtualKeyCode::S) => Some("KeyS"),
        Some(VirtualKeyCode::D) => Some("KeyD"),
        Some(VirtualKeyCode::F) => Some("KeyF"),
        Some(VirtualKeyCode::G) => Some("KeyG"),
        Some(VirtualKeyCode::H) => Some("KeyH"),
        Some(VirtualKeyCode::J) => Some("KeyJ"),
        Some(VirtualKeyCode::K) => Some("KeyK"),
        Some(VirtualKeyCode::L) => Some("KeyL"),
        Some(VirtualKeyCode::Semicolon) => Some("Semicolon"),
        Some(VirtualKeyCode::Return) => Some("Enter"),
        Some(VirtualKeyCode::Shift) => Some("ShiftLeft"),
        Some(VirtualKeyCode::Z) => Some("KeyZ"),
        Some(VirtualKeyCode::X) => Some("KeyX"),
        Some(VirtualKeyCode::C) => Some("KeyC"),
        Some(VirtualKeyCode::V) => Some("KeyV"),
        Some(VirtualKeyCode::B) => Some("KeyB"),
        Some(VirtualKeyCode::N) => Some("KeyN"),
        Some(VirtualKeyCode::M) => Some("KeyM"),
        Some(VirtualKeyCode::Comma) => Some("Comma"),
        Some(VirtualKeyCode::Period) => Some("Period"),
        Some(VirtualKeyCode::Slash) => Some("Slash"),
        Some(VirtualKeyCode::RightShift) => Some("ShiftRight"),
        Some(VirtualKeyCode::Control) => Some("ControlLeft"),
        Some(VirtualKeyCode::Option) => Some("AltLeft"),
        Some(VirtualKeyCode::Command) => Some("MetaLeft"),
        Some(VirtualKeyCode::Space) => Some("Space"),
        Some(VirtualKeyCode::RightCommand) => Some("MetaRight"),
        Some(VirtualKeyCode::RightOption) => Some("AltRight"),
        Some(VirtualKeyCode::RightControl) => Some("ControlRight"),
        Some(VirtualKeyCode::Escape) => Some("Escape"),
        _ => None,
    }
}

fn music_aux_key_code(aux_key_type: u16) -> Option<&'static str> {
    match aux_key_type {
        3 => Some("F1"),                    // NX_KEYTYPE_BRIGHTNESS_DOWN
        2 => Some("F2"),                    // NX_KEYTYPE_BRIGHTNESS_UP
        160 | 193 => Some("F3"),            // Mission Control variants observed in the wild
        13 | 131 | 132 | 194 => Some("F4"), // Launchpad variants observed in the wild
        12 | 22 => Some("F5"),              // Contrast / illumination down variants
        11 | 21 => Some("F6"),              // Contrast / illumination up variants
        18 => Some("F7"),                   // NX_KEYTYPE_PREVIOUS
        16 => Some("F8"),                   // NX_KEYTYPE_PLAY
        17 => Some("F9"),                   // NX_KEYTYPE_NEXT
        7 => Some("F10"),                   // NX_KEYTYPE_MUTE
        1 => Some("F11"),                   // NX_KEYTYPE_SOUND_DOWN
        0 => Some("F12"),                   // NX_KEYTYPE_SOUND_UP
        _ => None,
    }
}

fn decode_music_system_defined_event(event: CGEventRef) -> Option<(Option<&'static str>, bool)> {
    if event.is_null() {
        return None;
    }

    let data1 = unsafe { CGEventGetIntegerValueField(event, K_CG_EVENT_DATA1) };
    if data1 == 0 {
        return None;
    }

    Some(decode_music_system_defined_data1(data1))
}

fn decode_music_system_defined_data1(data1: i64) -> (Option<&'static str>, bool) {
    let data1 = data1 as u32;
    let aux_key_type = ((data1 & 0xFFFF_0000) >> 16) as u16;
    let key_state = ((data1 & 0x0000_FF00) >> 8) as u8;
    let key_down = key_state != K_MUSIC_SYSTEM_KEY_STATE_UP;

    (music_aux_key_code(aux_key_type), key_down)
}

fn dispatch_music_key_event(context: &TapContext, code: &'static str, key_down: bool) {
    let handler = context
        .music_key_handler
        .lock()
        .ok()
        .and_then(|handler| handler.clone());
    if let Some(handler) = handler {
        handler(code, key_down);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_autorepeat_is_suppressed_for_screenshot_shortcut() {
        let interceptor = KeyboardInterceptor::new();
        interceptor.context.enabled.store(true, Ordering::SeqCst);
        let mut data = (
            Arc::clone(&interceptor.context),
            Arc::clone(&interceptor.event_count),
        );
        let flags = core_keycode::FLAG_COMMAND | core_keycode::FLAG_SHIFT;
        let original =
            unsafe { CGEventCreateKeyboardEvent(ptr::null(), VirtualKeyCode::S as u16, true) };
        assert!(!original.is_null());
        unsafe {
            CGEventSetFlags(original, flags);
            CGEventSetIntegerValueField(original, K_CG_EVENT_SOURCE_USER_DATA, 0x594F5254);
            CGEventSetIntegerValueField(original, K_CG_KEYBOARD_EVENT_AUTOREPEAT, 1);
            assert_eq!(
                CGEventGetIntegerValueField(original, K_CG_EVENT_SOURCE_USER_DATA),
                0x594F5254
            );
            assert_eq!(
                CGEventGetIntegerValueField(original, K_CG_KEYBOARD_EVENT_AUTOREPEAT),
                1
            );
            assert_eq!(CGEventGetFlags(original), flags);
            assert!(interceptor.context.engine.lock().unwrap().is_enabled());
            let result = event_tap_callback(
                ptr::null_mut(),
                K_CG_EVENT_KEY_DOWN,
                original,
                &mut data as *mut _ as *mut c_void,
            );
            assert!(
                result.is_null(),
                "a held shortcut must not trigger another screenshot"
            );
            CFRelease(original.cast());
        }
    }

    #[test]
    fn remapped_key_replaces_input_without_reposting_and_preserves_repeat_and_time() {
        let flags = core_keycode::FLAG_COMMAND | core_keycode::FLAG_SHIFT;
        let original = create_synthetic_key(VirtualKeyCode::S as u16, true, flags);
        assert!(!original.is_null());
        unsafe {
            CGEventSetTimestamp(original, 12345);
            CGEventSetIntegerValueField(original, K_CG_KEYBOARD_EVENT_AUTOREPEAT, 1);
        }
        let replacement = replace_key_events(
            original,
            &[SyntheticKey {
                keycode: VirtualKeyCode::Key4 as u16,
                key_down: true,
                preserve_flags: false,
                extra_flags: flags,
            }],
            flags | core_keycode::FLAG_CONTROL,
            |_| panic!("single mappings must stay in the current event stream"),
        );
        assert!(!replacement.is_null());
        unsafe {
            assert_eq!(
                CGEventGetIntegerValueField(replacement, K_CG_KEYBOARD_EVENT_KEYCODE),
                VirtualKeyCode::Key4 as i64
            );
            assert_eq!(
                CGEventGetIntegerValueField(original, K_CG_KEYBOARD_EVENT_KEYCODE),
                VirtualKeyCode::S as i64
            );
            assert_eq!(CGEventGetFlags(replacement), flags);
            assert_eq!(CGEventGetTimestamp(replacement), 12345);
            assert_eq!(
                CGEventGetIntegerValueField(replacement, K_CG_KEYBOARD_EVENT_AUTOREPEAT),
                1
            );
            assert_eq!(
                CGEventGetIntegerValueField(replacement, K_CG_EVENT_SOURCE_USER_DATA),
                SELF_INJECTED_TAG
            );
            CFRelease(replacement.cast());
            CFRelease(original.cast());
        }
    }

    #[test]
    fn sequence_posts_prefix_in_order_and_returns_the_last_event() {
        let original = create_synthetic_key(
            VirtualKeyCode::Space as u16,
            false,
            core_keycode::FLAG_SHIFT,
        );
        assert!(!original.is_null());
        let keys = [
            SyntheticKey {
                keycode: VirtualKeyCode::LeftArrow as u16,
                key_down: true,
                preserve_flags: true,
                extra_flags: 0,
            },
            SyntheticKey {
                keycode: VirtualKeyCode::LeftArrow as u16,
                key_down: false,
                preserve_flags: false,
                extra_flags: 0,
            },
            SyntheticKey {
                keycode: VirtualKeyCode::S as u16,
                key_down: true,
                preserve_flags: false,
                extra_flags: core_keycode::FLAG_COMMAND,
            },
        ];
        let mut observed = Vec::new();
        let replacement =
            replace_key_events(original, &keys, core_keycode::FLAG_SHIFT, |event| unsafe {
                observed.push((
                    CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE),
                    CGEventGetFlags(event),
                ));
            });
        assert_eq!(
            observed,
            [
                (VirtualKeyCode::LeftArrow as i64, core_keycode::FLAG_SHIFT),
                (VirtualKeyCode::LeftArrow as i64, 0)
            ]
        );
        unsafe {
            assert_eq!(
                CGEventGetIntegerValueField(replacement, K_CG_KEYBOARD_EVENT_KEYCODE),
                VirtualKeyCode::S as i64
            );
            assert_eq!(CGEventGetFlags(replacement), core_keycode::FLAG_COMMAND);
            CFRelease(replacement.cast());
            CFRelease(original.cast());
        }
    }

    #[test]
    fn event_tap_interruption_clears_pending_layer_without_disabling_mapping() {
        let interceptor = KeyboardInterceptor::new();
        interceptor.context.engine.lock().unwrap().process_key(
            VirtualKeyCode::Space as u16,
            true,
            false,
            0,
        );
        let mut data = (
            Arc::clone(&interceptor.context),
            Arc::clone(&interceptor.event_count),
        );
        unsafe {
            event_tap_callback(
                ptr::null_mut(),
                K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT,
                ptr::null_mut(),
                &mut data as *mut _ as *mut c_void,
            );
        }
        let mut engine = interceptor.context.engine.lock().unwrap();
        assert!(engine.is_enabled());
        assert!(matches!(
            engine.process_key(VirtualKeyCode::J as u16, true, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn event_tap_interruption_cancels_a_stale_mouse_session() {
        for event_type in [K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT, 0xFFFFFFFF] {
            let interceptor = KeyboardInterceptor::new();
            {
                let mut engine = interceptor.context.engine.lock().unwrap();
                engine.process_key(VirtualKeyCode::Tab as u16, true, false, 0);
                engine.process_key(VirtualKeyCode::J as u16, true, false, 0);
                assert!(engine.is_mouse_mode_active());
            }
            let mut data = (
                Arc::clone(&interceptor.context),
                Arc::clone(&interceptor.event_count),
            );
            // A disabled-tap notification has no key event and our test has no
            // native tap to enable. This never injects input into the desktop.
            unsafe {
                event_tap_callback(
                    ptr::null_mut(),
                    event_type,
                    ptr::null_mut(),
                    &mut data as *mut _ as *mut c_void,
                );
            }
            assert!(
                !interceptor
                    .context
                    .engine
                    .lock()
                    .unwrap()
                    .is_mouse_mode_active()
            );
        }
    }

    #[test]
    fn maps_standard_music_keycodes_to_dom_codes() {
        assert_eq!(
            music_dom_code_for_keycode(VirtualKeyCode::A as u16),
            Some("KeyA")
        );
        assert_eq!(
            music_dom_code_for_keycode(VirtualKeyCode::Command as u16),
            Some("MetaLeft")
        );
        assert_eq!(
            music_dom_code_for_keycode(VirtualKeyCode::F8 as u16),
            Some("F8")
        );
        assert_eq!(
            music_dom_code_for_keycode(VirtualKeyCode::Delete as u16),
            Some("Backspace")
        );
    }

    #[test]
    fn maps_aux_system_keys_to_function_row_codes() {
        assert_eq!(music_aux_key_code(3), Some("F1"));
        assert_eq!(music_aux_key_code(2), Some("F2"));
        assert_eq!(music_aux_key_code(22), Some("F5"));
        assert_eq!(music_aux_key_code(21), Some("F6"));
        assert_eq!(music_aux_key_code(18), Some("F7"));
        assert_eq!(music_aux_key_code(16), Some("F8"));
        assert_eq!(music_aux_key_code(7), Some("F10"));
        assert_eq!(music_aux_key_code(0), Some("F12"));
    }

    #[test]
    fn decodes_system_defined_data1_payloads_without_nsevent() {
        assert_eq!(
            decode_music_system_defined_data1(((3_i64) << 16) | 0x0A00),
            (Some("F1"), true)
        );
        assert_eq!(
            decode_music_system_defined_data1(((16_i64) << 16) | 0x0B00),
            (Some("F8"), false)
        );
        assert_eq!(
            decode_music_system_defined_data1(0x0A00),
            (Some("F12"), true)
        );
        assert_eq!(
            decode_music_system_defined_data1(((200_i64) << 16) | 0x0A00),
            (None, true)
        );
    }
}
