#![cfg(target_os = "windows")]

use std::collections::HashSet;
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::{
    atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering},
    mpsc, Arc, Mutex, OnceLock,
};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
    MOUSEEVENTF_XUP, MOUSEINPUT, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9, VK_A,
    VK_B, VK_BACK, VK_C, VK_CONTROL, VK_D, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F, VK_G, VK_H,
    VK_HOME, VK_I, VK_J, VK_K, VK_L, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_M,
    VK_MENU, VK_N, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_COMMA,
    VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS, VK_P, VK_Q, VK_R, VK_RCONTROL, VK_RETURN, VK_RIGHT,
    VK_RMENU, VK_RSHIFT, VK_RWIN, VK_S, VK_SHIFT, VK_SPACE, VK_T, VK_TAB, VK_U, VK_UP, VK_V, VK_W,
    VK_X, VK_Y, VK_Z,
};
use windows_sys::Win32::UI::Shell::IsUserAnAdmin;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_ALTDOWN, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN,
    WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_USER, WM_XBUTTONDOWN, XBUTTON1,
};
use yorling_core::keycode::{self, VirtualKeyCode};
use yorling_engine::engine::{
    EngineAction, MappingEngine, MappingPlatform, MouseButton, MouseScrollDirection, SyntheticKey,
    SystemAction,
};

const SELF_INJECTED_TAG: usize = 0x594F524C;
const WHEEL_DELTA: i32 = 120;
const MOUSE_SCROLL_LINES: i32 = 3;
const MOUSE_MOVE_SPEED_FAST: f64 = 1400.0;
const MOUSE_MOVE_SPEED_SLOW: f64 = 360.0;
const MOUSE_MOVE_TICK: Duration = Duration::from_micros(8_333);
const MOUSE_MOVE_RESPONSE: f64 = 18.0;

static HOOK_CONTEXT: OnceLock<Mutex<Option<Arc<HookContext>>>> = OnceLock::new();

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MouseMoveIntent {
    x: i8,
    y: i8,
    fast: bool,
}

#[derive(Debug, Default)]
struct SmoothMouseMotion {
    velocity_x: f64,
    velocity_y: f64,
}

impl SmoothMouseMotion {
    fn step(&mut self, intent: MouseMoveIntent, dt: Duration) -> Option<(f64, f64)> {
        if intent.x == 0 && intent.y == 0 {
            self.velocity_x = 0.0;
            self.velocity_y = 0.0;
            return None;
        }

        let dt_secs = dt.as_secs_f64();
        if dt_secs <= f64::EPSILON {
            return None;
        }

        let magnitude = f64::from(intent.x.pow(2) + intent.y.pow(2)).sqrt();
        let speed = if intent.fast {
            MOUSE_MOVE_SPEED_FAST
        } else {
            MOUSE_MOVE_SPEED_SLOW
        };
        let target_vx = f64::from(intent.x) / magnitude * speed;
        let target_vy = f64::from(intent.y) / magnitude * speed;
        let blend = 1.0 - (-MOUSE_MOVE_RESPONSE * dt_secs).exp();
        self.velocity_x += (target_vx - self.velocity_x) * blend;
        self.velocity_y += (target_vy - self.velocity_y) * blend;

        Some((self.velocity_x * dt_secs, self.velocity_y * dt_secs))
    }
}

struct MouseMotionController {
    intent: Arc<Mutex<MouseMoveIntent>>,
    running: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl MouseMotionController {
    fn new() -> Self {
        Self {
            intent: Arc::new(Mutex::new(MouseMoveIntent::default())),
            running: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
        }
    }

    fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        let intent = Arc::clone(&self.intent);
        let running = Arc::clone(&self.running);
        let handle = thread::spawn(move || {
            let mut motion = SmoothMouseMotion::default();
            let mut last_tick = Instant::now();

            while running.load(Ordering::SeqCst) {
                let now = Instant::now();
                let dt = now.saturating_duration_since(last_tick);
                last_tick = now;

                let current_intent = *intent.lock().unwrap();
                if let Some((dx, dy)) = motion.step(current_intent, dt) {
                    emit_mouse_move(dx, dy);
                }

                thread::sleep(MOUSE_MOVE_TICK);
            }
        });

        *self.worker.lock().unwrap() = Some(handle);
    }

    fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.stop_motion();

        if let Some(handle) = self.worker.lock().unwrap().take() {
            let _ = handle.join();
        }
    }

    fn update_intent(&self, intent: MouseMoveIntent) {
        *self.intent.lock().unwrap() = intent;
    }

    fn stop_motion(&self) {
        self.update_intent(MouseMoveIntent::default());
    }
}

struct HookContext {
    engine: Mutex<MappingEngine>,
    enabled: AtomicBool,
    event_count: AtomicU64,
    physical_keys: Mutex<HashSet<u16>>,
    synthetic_restored_modifiers: Mutex<HashSet<u16>>,
    system_action_handler: Mutex<Option<Arc<dyn Fn(SystemAction) + Send + Sync>>>,
    mouse_motion: MouseMotionController,
}

impl HookContext {
    fn new() -> Self {
        Self {
            engine: Mutex::new(MappingEngine::new_for_platform(MappingPlatform::Windows)),
            enabled: AtomicBool::new(false),
            event_count: AtomicU64::new(0),
            physical_keys: Mutex::new(HashSet::new()),
            synthetic_restored_modifiers: Mutex::new(HashSet::new()),
            system_action_handler: Mutex::new(None),
            mouse_motion: MouseMotionController::new(),
        }
    }
}

pub struct WindowsKeyboardInterceptor {
    context: Arc<HookContext>,
    running: Arc<AtomicBool>,
    hook_handle: Arc<AtomicPtr<c_void>>,
    mouse_hook_handle: Arc<AtomicPtr<c_void>>,
    hook_thread_id: Arc<AtomicU32>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl WindowsKeyboardInterceptor {
    pub fn new() -> Self {
        Self {
            context: Arc::new(HookContext::new()),
            running: Arc::new(AtomicBool::new(false)),
            hook_handle: Arc::new(AtomicPtr::new(ptr::null_mut())),
            mouse_hook_handle: Arc::new(AtomicPtr::new(ptr::null_mut())),
            hook_thread_id: Arc::new(AtomicU32::new(0)),
            worker: Mutex::new(None),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        self.start_with_enabled(true)
    }

    fn start_with_enabled(&self, initially_enabled: bool) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("Windows keyboard hook is already running".into());
        }

        {
            let mut engine = self.context.engine.lock().unwrap();
            *engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
            engine.set_enabled(initially_enabled);
        }
        self.context.physical_keys.lock().unwrap().clear();
        self.context
            .synthetic_restored_modifiers
            .lock()
            .unwrap()
            .clear();
        self.context.event_count.store(0, Ordering::SeqCst);
        self.context
            .enabled
            .store(initially_enabled, Ordering::SeqCst);

        let context = Arc::clone(&self.context);
        let running = Arc::clone(&self.running);
        let hook_handle = Arc::clone(&self.hook_handle);
        let mouse_hook_handle = Arc::clone(&self.mouse_hook_handle);
        let hook_thread_id = Arc::clone(&self.hook_thread_id);
        let (startup_tx, startup_rx) = mpsc::channel::<Result<(), String>>();

        let handle = thread::spawn(move || unsafe {
            let thread_id = GetCurrentThreadId();
            hook_thread_id.store(thread_id, Ordering::SeqCst);

            let mut bootstrap_msg: MSG = mem::zeroed();
            PeekMessageW(
                &mut bootstrap_msg,
                ptr::null_mut(),
                WM_USER,
                WM_USER,
                PM_NOREMOVE,
            );

            set_hook_context(Some(Arc::clone(&context)));
            let module = GetModuleHandleW(ptr::null());
            let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0);

            if hook.is_null() {
                set_hook_context(None);
                hook_thread_id.store(0, Ordering::SeqCst);
                let _ = startup_tx.send(Err(
                    "Failed to install the Windows low-level keyboard hook.".into(),
                ));
                return;
            }

            hook_handle.store(hook, Ordering::SeqCst);
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(low_level_mouse_proc), module, 0);
            mouse_hook_handle.store(mouse_hook, Ordering::SeqCst);
            running.store(true, Ordering::SeqCst);
            let _ = startup_tx.send(Ok(()));

            let mut msg: MSG = mem::zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            context.enabled.store(false, Ordering::SeqCst);
            context.mouse_motion.stop();
            if let Ok(mut engine) = context.engine.lock() {
                let flags = current_physical_flags(&context);
                for key in engine.set_enabled(false) {
                    emit_synthetic_key(key.keycode, key.key_down, key.extra_flags, flags);
                }
            }

            UnhookWindowsHookEx(hook);
            if !mouse_hook.is_null() {
                UnhookWindowsHookEx(mouse_hook);
            }
            set_hook_context(None);
            hook_handle.store(ptr::null_mut(), Ordering::SeqCst);
            mouse_hook_handle.store(ptr::null_mut(), Ordering::SeqCst);
            hook_thread_id.store(0, Ordering::SeqCst);
            running.store(false, Ordering::SeqCst);
        });

        *self.worker.lock().unwrap() = Some(handle);

        match startup_rx.recv() {
            Ok(Ok(())) => {
                self.context.mouse_motion.start();
                Ok(())
            }
            Ok(Err(error)) => {
                if let Some(handle) = self.worker.lock().unwrap().take() {
                    let _ = handle.join();
                }
                Err(error)
            }
            Err(_) => {
                if let Some(handle) = self.worker.lock().unwrap().take() {
                    let _ = handle.join();
                }
                Err("Windows keyboard hook thread exited before startup completed.".into())
            }
        }
    }

    pub fn stop(&self) {
        self.context.enabled.store(false, Ordering::SeqCst);
        self.context.mouse_motion.stop_motion();
        release_synthetic_restored_modifiers(&self.context);

        if let Ok(mut engine) = self.context.engine.lock() {
            let flags = current_physical_flags(&self.context);
            for key in engine.set_enabled(false) {
                emit_synthetic_key(key.keycode, key.key_down, key.extra_flags, flags);
            }
        }
        self.context.physical_keys.lock().unwrap().clear();

        let thread_id = self.hook_thread_id.load(Ordering::SeqCst);
        if thread_id != 0 {
            unsafe {
                PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
            }
        }

        if let Some(handle) = self.worker.lock().unwrap().take() {
            let _ = handle.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.context.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            release_synthetic_restored_modifiers(&self.context);
        }

        if let Ok(mut engine) = self.context.engine.lock() {
            let flags = current_physical_flags(&self.context);
            for key in engine.set_enabled(enabled) {
                emit_synthetic_key(key.keycode, key.key_down, key.extra_flags, flags);
            }
        }

        if !enabled {
            self.context.mouse_motion.stop_motion();
            self.context.physical_keys.lock().unwrap().clear();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.context.enabled.load(Ordering::SeqCst)
    }

    pub fn set_system_action_handler<F>(&self, handler: F)
    where
        F: Fn(SystemAction) + Send + Sync + 'static,
    {
        *self.context.system_action_handler.lock().unwrap() = Some(Arc::new(handler));
    }

    pub fn event_count(&self) -> u64 {
        self.context.event_count.load(Ordering::SeqCst)
    }

    pub fn is_running_elevated() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }
}

impl Drop for WindowsKeyboardInterceptor {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Default for WindowsKeyboardInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

fn hook_context_slot() -> &'static Mutex<Option<Arc<HookContext>>> {
    HOOK_CONTEXT.get_or_init(|| Mutex::new(None))
}

fn set_hook_context(context: Option<Arc<HookContext>>) {
    *hook_context_slot().lock().unwrap() = context;
}

fn current_hook_context() -> Option<Arc<HookContext>> {
    hook_context_slot().lock().unwrap().clone()
}

unsafe extern "system" fn low_level_mouse_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    let mouse = unsafe { &*(l_param as *const MSLLHOOKSTRUCT) };
    if mouse.dwExtraInfo == SELF_INJECTED_TAG {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    if matches!(
        w_param as u32,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
    ) {
        if let Some(context) = current_hook_context() {
            release_synthetic_restored_modifiers(&context);
            cancel_transient_modes_for_mouse_down(&context);
        }
    }

    unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) }
}

unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code != HC_ACTION as i32 {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    let keyboard = unsafe { &*(l_param as *const KBDLLHOOKSTRUCT) };
    if keyboard.dwExtraInfo == SELF_INJECTED_TAG {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    let key_down = match w_param as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => true,
        WM_KEYUP | WM_SYSKEYUP => false,
        _ => return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) },
    };

    let Some(context) = current_hook_context() else {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    };

    if is_native_alt_tab_event(&context, keyboard.vkCode as u16, key_down, keyboard.flags) {
        clear_native_alt_tab_state(&context);
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    let Some(keycode) = windows_vk_to_internal(keyboard.vkCode as u16) else {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    };

    let (is_autorepeat, flags) = update_physical_key_state(&context, keycode, key_down);

    if !context.enabled.load(Ordering::Relaxed) {
        return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) };
    }

    context.event_count.fetch_add(1, Ordering::Relaxed);

    let (action, mouse_mode_exited) = {
        let mut engine = match context.engine.lock() {
            Ok(engine) => engine,
            Err(_) => return unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) },
        };
        let was_mouse_mode_active = engine.is_mouse_mode_active();
        let action = engine.process_key(keycode, key_down, is_autorepeat, flags);
        let mouse_mode_exited = was_mouse_mode_active && !engine.is_mouse_mode_active();
        (action, mouse_mode_exited)
    };

    if mouse_mode_exited {
        context.mouse_motion.stop_motion();
    }

    if dispatch_engine_action(&context, action, flags) {
        1
    } else {
        unsafe { CallNextHookEx(ptr::null_mut(), n_code, w_param, l_param) }
    }
}

fn is_native_alt_tab_event(
    context: &HookContext,
    vk_code: u16,
    key_down: bool,
    keyboard_flags: u32,
) -> bool {
    if !key_down || vk_code != VK_TAB {
        return false;
    }

    if keyboard_flags & LLKHF_ALTDOWN != 0 {
        return true;
    }

    let physical_keys = context.physical_keys.lock().unwrap();
    physical_keys.contains(&(VirtualKeyCode::Option as u16))
        || physical_keys.contains(&(VirtualKeyCode::RightOption as u16))
}

fn clear_native_alt_tab_state(context: &HookContext) {
    {
        let mut physical_keys = context.physical_keys.lock().unwrap();
        physical_keys.remove(&(VirtualKeyCode::Option as u16));
        physical_keys.remove(&(VirtualKeyCode::RightOption as u16));
        physical_keys.remove(&(VirtualKeyCode::Tab as u16));
    }

    context.mouse_motion.stop_motion();

    let (bracket_action, mouse_action) = match context.engine.lock() {
        Ok(mut engine) => {
            engine.cancel_alt_tab_session();
            (engine.cancel_bracket_mode(), engine.cancel_mouse_mode())
        }
        Err(_) => return,
    };

    if let Some(action) = bracket_action {
        dispatch_system_action(context, action);
    }
    if let Some(action) = mouse_action {
        dispatch_system_action(context, action);
    }
}

fn cancel_transient_modes_for_mouse_down(context: &HookContext) {
    if !context.enabled.load(Ordering::Relaxed) {
        return;
    }

    let (bracket_action, mouse_action) = match context.engine.lock() {
        Ok(mut engine) => (engine.cancel_bracket_mode(), engine.cancel_mouse_mode()),
        Err(_) => return,
    };

    if let Some(action) = bracket_action {
        dispatch_system_action(context, action);
    }
    if let Some(action) = mouse_action {
        dispatch_system_action(context, action);
    }
}

fn update_physical_key_state(context: &HookContext, keycode: u16, key_down: bool) -> (bool, u64) {
    let mut physical_keys = context.physical_keys.lock().unwrap();
    let was_down = physical_keys.contains(&keycode);

    if key_down {
        physical_keys.insert(keycode);
        remove_restored_modifier(&context.synthetic_restored_modifiers, keycode);
    } else {
        physical_keys.remove(&keycode);
        remove_restored_modifier(&context.synthetic_restored_modifiers, keycode);
    }

    let flags = modifier_flags_from_keys(&physical_keys);
    (key_down && was_down, flags)
}

fn remove_restored_modifier(restored_modifiers: &Mutex<HashSet<u16>>, keycode: u16) {
    if !is_modifier_keycode(keycode) {
        return;
    }

    let mut restored_modifiers = restored_modifiers.lock().unwrap();
    restored_modifiers.remove(&canonical_modifier_keycode(keycode));
}

fn current_physical_flags(context: &HookContext) -> u64 {
    modifier_flags_from_keys(&context.physical_keys.lock().unwrap())
}

fn modifier_flags_from_keys(keys: &HashSet<u16>) -> u64 {
    let mut flags = 0;

    for keycode in keys {
        match VirtualKeyCode::from_raw(*keycode) {
            Some(VirtualKeyCode::Shift | VirtualKeyCode::RightShift) => {
                flags |= keycode::FLAG_SHIFT;
            }
            Some(VirtualKeyCode::Control | VirtualKeyCode::RightControl) => {
                flags |= keycode::FLAG_CONTROL;
            }
            Some(VirtualKeyCode::Option | VirtualKeyCode::RightOption) => {
                flags |= keycode::FLAG_OPTION;
            }
            Some(VirtualKeyCode::Command | VirtualKeyCode::RightCommand) => {
                flags |= keycode::FLAG_COMMAND;
            }
            _ => {}
        }
    }

    flags
}

fn is_modifier_keycode(keycode: u16) -> bool {
    matches!(
        VirtualKeyCode::from_raw(keycode),
        Some(
            VirtualKeyCode::Shift
                | VirtualKeyCode::RightShift
                | VirtualKeyCode::Control
                | VirtualKeyCode::RightControl
                | VirtualKeyCode::Option
                | VirtualKeyCode::RightOption
                | VirtualKeyCode::Command
                | VirtualKeyCode::RightCommand
        )
    )
}

fn canonical_modifier_keycode(keycode: u16) -> u16 {
    match VirtualKeyCode::from_raw(keycode) {
        Some(VirtualKeyCode::RightShift) => VirtualKeyCode::Shift as u16,
        Some(VirtualKeyCode::RightControl) => VirtualKeyCode::Control as u16,
        Some(VirtualKeyCode::RightOption) => VirtualKeyCode::Option as u16,
        Some(VirtualKeyCode::RightCommand) => VirtualKeyCode::Command as u16,
        _ => keycode,
    }
}

fn modifier_keycodes_for_flags(flags: u64) -> impl Iterator<Item = u16> {
    [
        (keycode::FLAG_COMMAND, VirtualKeyCode::Command as u16),
        (keycode::FLAG_CONTROL, VirtualKeyCode::Control as u16),
        (keycode::FLAG_SHIFT, VirtualKeyCode::Shift as u16),
        (keycode::FLAG_OPTION, VirtualKeyCode::Option as u16),
    ]
    .into_iter()
    .filter_map(move |(flag, keycode)| {
        if flags & flag != 0 {
            Some(keycode)
        } else {
            None
        }
    })
}

fn remove_modifier_family(keys: &mut HashSet<u16>, keycode: u16) {
    match VirtualKeyCode::from_raw(canonical_modifier_keycode(keycode)) {
        Some(VirtualKeyCode::Shift) => {
            keys.remove(&(VirtualKeyCode::Shift as u16));
            keys.remove(&(VirtualKeyCode::RightShift as u16));
        }
        Some(VirtualKeyCode::Control) => {
            keys.remove(&(VirtualKeyCode::Control as u16));
            keys.remove(&(VirtualKeyCode::RightControl as u16));
        }
        Some(VirtualKeyCode::Option) => {
            keys.remove(&(VirtualKeyCode::Option as u16));
            keys.remove(&(VirtualKeyCode::RightOption as u16));
        }
        Some(VirtualKeyCode::Command) => {
            keys.remove(&(VirtualKeyCode::Command as u16));
            keys.remove(&(VirtualKeyCode::RightCommand as u16));
        }
        _ => {
            keys.remove(&keycode);
        }
    }
}

fn record_synthetic_modifier_restores(context: &HookContext, restored_flags: u64) {
    if restored_flags == 0 {
        return;
    }

    let mut restored_modifiers = context.synthetic_restored_modifiers.lock().unwrap();
    for keycode in modifier_keycodes_for_flags(restored_flags) {
        restored_modifiers.insert(keycode);
    }
}

fn release_synthetic_restored_modifiers(context: &HookContext) {
    let restored_modifiers: Vec<u16> = context
        .synthetic_restored_modifiers
        .lock()
        .unwrap()
        .drain()
        .collect();

    if restored_modifiers.is_empty() {
        return;
    }

    {
        let mut physical_keys = context.physical_keys.lock().unwrap();
        for keycode in &restored_modifiers {
            remove_modifier_family(&mut physical_keys, *keycode);
        }
    }

    context.mouse_motion.stop_motion();
    for keycode in restored_modifiers {
        emit_synthetic_key(keycode, false, 0, 0);
    }
}

fn dispatch_engine_action(
    context: &HookContext,
    action: EngineAction,
    physical_flags: u64,
) -> bool {
    match action {
        EngineAction::PassThrough => false,
        EngineAction::Suppress => true,
        EngineAction::Emit(keys) => {
            dispatch_synthetic_keys(context, keys, physical_flags);
            true
        }
        EngineAction::EmitAndSystem(keys, system_action) => {
            dispatch_synthetic_keys(context, keys, physical_flags);
            dispatch_system_action(context, system_action);
            true
        }
        EngineAction::System(system_action) => {
            dispatch_system_action(context, system_action);
            true
        }
    }
}

fn dispatch_synthetic_keys(context: &HookContext, keys: Vec<SyntheticKey>, physical_flags: u64) {
    let mut index = 0;
    while index < keys.len() {
        let key = &keys[index];
        let desired_flags = desired_flags_for_synthetic_key(key, physical_flags);
        record_synthetic_modifier_restores(context, physical_flags & !desired_flags);

        if key.key_down && index + 1 < keys.len() {
            let next = &keys[index + 1];
            let next_desired_flags = desired_flags_for_synthetic_key(next, physical_flags);
            if !next.key_down && next.keycode == key.keycode && next_desired_flags == desired_flags
            {
                emit_synthetic_key_press(key.keycode, desired_flags, physical_flags);
                index += 2;
                continue;
            }
        }

        emit_synthetic_key(key.keycode, key.key_down, desired_flags, physical_flags);
        index += 1;
    }
}

fn desired_flags_for_synthetic_key(key: &SyntheticKey, physical_flags: u64) -> u64 {
    if key.preserve_flags {
        physical_flags | key.extra_flags
    } else {
        key.extra_flags
    }
}

fn dispatch_system_action(context: &HookContext, action: SystemAction) {
    match action {
        SystemAction::MouseMove { x, y, fast } => {
            context
                .mouse_motion
                .update_intent(MouseMoveIntent { x, y, fast });
        }
        SystemAction::MouseClick { button } => {
            emit_mouse_click(button);
        }
        SystemAction::MouseScroll { direction } => {
            let lines = match direction {
                MouseScrollDirection::Up => MOUSE_SCROLL_LINES,
                MouseScrollDirection::Down => -MOUSE_SCROLL_LINES,
            };
            emit_mouse_scroll(lines);
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

fn forward_system_action(context: &HookContext, action: SystemAction) {
    let handler = context
        .system_action_handler
        .lock()
        .ok()
        .and_then(|handler| handler.clone());

    if let Some(handler) = handler {
        handler(action);
    }
}

fn emit_synthetic_key(keycode: u16, key_down: bool, desired_flags: u64, physical_flags: u64) {
    let Some(vk) = internal_to_windows_vk(keycode) else {
        return;
    };

    let modifiers_to_release = physical_flags & !desired_flags;
    let modifiers_to_press = desired_flags & !physical_flags;
    let mut inputs = Vec::new();

    push_modifier_inputs(&mut inputs, modifiers_to_release, false);
    push_modifier_inputs(&mut inputs, modifiers_to_press, true);
    push_keyboard_input(&mut inputs, vk, key_down);
    push_modifier_inputs(&mut inputs, modifiers_to_press, false);
    push_modifier_inputs(&mut inputs, modifiers_to_release, true);
    send_inputs(&mut inputs);
}

fn emit_synthetic_key_press(keycode: u16, desired_flags: u64, physical_flags: u64) {
    let Some(vk) = internal_to_windows_vk(keycode) else {
        return;
    };

    let modifiers_to_release = physical_flags & !desired_flags;
    let modifiers_to_press = desired_flags & !physical_flags;
    let mut inputs = Vec::new();

    push_modifier_inputs(&mut inputs, modifiers_to_release, false);
    push_modifier_inputs(&mut inputs, modifiers_to_press, true);
    push_keyboard_input(&mut inputs, vk, true);
    push_keyboard_input(&mut inputs, vk, false);
    push_modifier_inputs(&mut inputs, modifiers_to_press, false);
    push_modifier_inputs(&mut inputs, modifiers_to_release, true);
    send_inputs(&mut inputs);
}

fn push_modifier_inputs(inputs: &mut Vec<INPUT>, flags: u64, key_down: bool) {
    const MODIFIER_KEYS: &[(u64, u16)] = &[
        (keycode::FLAG_COMMAND, VK_LWIN),
        (keycode::FLAG_CONTROL, VK_CONTROL),
        (keycode::FLAG_SHIFT, VK_SHIFT),
        (keycode::FLAG_OPTION, VK_MENU),
    ];

    if key_down {
        for (flag, vk) in MODIFIER_KEYS {
            if flags & *flag != 0 {
                push_keyboard_input(inputs, *vk, true);
            }
        }
    } else {
        for (flag, vk) in MODIFIER_KEYS.iter().rev() {
            if flags & *flag != 0 {
                push_keyboard_input(inputs, *vk, false);
            }
        }
    }
}

fn push_keyboard_input(inputs: &mut Vec<INPUT>, vk: u16, key_down: bool) {
    let mut event_flags = if key_down { 0 } else { KEYEVENTF_KEYUP };
    if is_extended_key(vk) {
        event_flags |= KEYEVENTF_EXTENDEDKEY;
    }

    inputs.push(INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: event_flags,
                time: 0,
                dwExtraInfo: SELF_INJECTED_TAG,
            },
        },
    });
}

fn is_extended_key(vk: u16) -> bool {
    matches!(
        vk,
        VK_HOME | VK_END | VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_RCONTROL | VK_RMENU | VK_RWIN
    )
}

fn push_mouse_input(inputs: &mut Vec<INPUT>, dx: i32, dy: i32, data: u32, flags: u32) {
    inputs.push(INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SELF_INJECTED_TAG,
            },
        },
    });
}

fn send_inputs(inputs: &mut [INPUT]) {
    if inputs.is_empty() {
        return;
    }

    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            mem::size_of::<INPUT>() as i32,
        );
    }
}

fn emit_mouse_move(dx: f64, dy: f64) {
    let mut inputs = Vec::new();
    push_mouse_input(
        &mut inputs,
        dx.round() as i32,
        dy.round() as i32,
        0,
        MOUSEEVENTF_MOVE,
    );
    send_inputs(&mut inputs);
}

fn emit_mouse_click(button: MouseButton) {
    let mut inputs = Vec::new();
    match button {
        MouseButton::Left => {
            push_mouse_input(&mut inputs, 0, 0, 0, MOUSEEVENTF_LEFTDOWN);
            push_mouse_input(&mut inputs, 0, 0, 0, MOUSEEVENTF_LEFTUP);
        }
        MouseButton::Right => {
            push_mouse_input(&mut inputs, 0, 0, 0, MOUSEEVENTF_RIGHTDOWN);
            push_mouse_input(&mut inputs, 0, 0, 0, MOUSEEVENTF_RIGHTUP);
        }
        MouseButton::Back => {
            push_mouse_input(&mut inputs, 0, 0, XBUTTON1 as u32, MOUSEEVENTF_XDOWN);
            push_mouse_input(&mut inputs, 0, 0, XBUTTON1 as u32, MOUSEEVENTF_XUP);
        }
    }
    send_inputs(&mut inputs);
}

fn emit_mouse_scroll(lines: i32) {
    let mut inputs = Vec::new();
    push_mouse_input(
        &mut inputs,
        0,
        0,
        (lines * WHEEL_DELTA) as u32,
        MOUSEEVENTF_WHEEL,
    );
    send_inputs(&mut inputs);
}

fn windows_vk_to_internal(vk: u16) -> Option<u16> {
    match vk {
        VK_A => Some(VirtualKeyCode::A as u16),
        VK_B => Some(VirtualKeyCode::B as u16),
        VK_C => Some(VirtualKeyCode::C as u16),
        VK_D => Some(VirtualKeyCode::D as u16),
        VK_E => Some(VirtualKeyCode::E as u16),
        VK_F => Some(VirtualKeyCode::F as u16),
        VK_G => Some(VirtualKeyCode::G as u16),
        VK_H => Some(VirtualKeyCode::H as u16),
        VK_I => Some(VirtualKeyCode::I as u16),
        VK_J => Some(VirtualKeyCode::J as u16),
        VK_K => Some(VirtualKeyCode::K as u16),
        VK_L => Some(VirtualKeyCode::L as u16),
        VK_M => Some(VirtualKeyCode::M as u16),
        VK_N => Some(VirtualKeyCode::N as u16),
        VK_O => Some(VirtualKeyCode::O as u16),
        VK_P => Some(VirtualKeyCode::P as u16),
        VK_Q => Some(VirtualKeyCode::Q as u16),
        VK_R => Some(VirtualKeyCode::R as u16),
        VK_S => Some(VirtualKeyCode::S as u16),
        VK_T => Some(VirtualKeyCode::T as u16),
        VK_U => Some(VirtualKeyCode::U as u16),
        VK_V => Some(VirtualKeyCode::V as u16),
        VK_W => Some(VirtualKeyCode::W as u16),
        VK_X => Some(VirtualKeyCode::X as u16),
        VK_Y => Some(VirtualKeyCode::Y as u16),
        VK_Z => Some(VirtualKeyCode::Z as u16),
        VK_0 => Some(VirtualKeyCode::Key0 as u16),
        VK_1 => Some(VirtualKeyCode::Key1 as u16),
        VK_2 => Some(VirtualKeyCode::Key2 as u16),
        VK_3 => Some(VirtualKeyCode::Key3 as u16),
        VK_4 => Some(VirtualKeyCode::Key4 as u16),
        VK_5 => Some(VirtualKeyCode::Key5 as u16),
        VK_6 => Some(VirtualKeyCode::Key6 as u16),
        VK_7 => Some(VirtualKeyCode::Key7 as u16),
        VK_8 => Some(VirtualKeyCode::Key8 as u16),
        VK_9 => Some(VirtualKeyCode::Key9 as u16),
        VK_OEM_3 => Some(VirtualKeyCode::Grave as u16),
        VK_OEM_MINUS => Some(VirtualKeyCode::Minus as u16),
        VK_OEM_PLUS => Some(VirtualKeyCode::Equal as u16),
        VK_OEM_4 => Some(VirtualKeyCode::LeftBracket as u16),
        VK_OEM_6 => Some(VirtualKeyCode::RightBracket as u16),
        VK_OEM_5 => Some(VirtualKeyCode::Backslash as u16),
        VK_OEM_1 => Some(VirtualKeyCode::Semicolon as u16),
        VK_OEM_COMMA => Some(VirtualKeyCode::Comma as u16),
        VK_OEM_PERIOD => Some(VirtualKeyCode::Period as u16),
        VK_OEM_2 => Some(VirtualKeyCode::Slash as u16),
        VK_RETURN => Some(VirtualKeyCode::Return as u16),
        VK_TAB => Some(VirtualKeyCode::Tab as u16),
        VK_SPACE => Some(VirtualKeyCode::Space as u16),
        VK_BACK => Some(VirtualKeyCode::Delete as u16),
        VK_ESCAPE => Some(VirtualKeyCode::Escape as u16),
        VK_LEFT => Some(VirtualKeyCode::LeftArrow as u16),
        VK_RIGHT => Some(VirtualKeyCode::RightArrow as u16),
        VK_DOWN => Some(VirtualKeyCode::DownArrow as u16),
        VK_UP => Some(VirtualKeyCode::UpArrow as u16),
        VK_HOME => Some(VirtualKeyCode::Home as u16),
        VK_END => Some(VirtualKeyCode::End as u16),
        VK_SHIFT | VK_LSHIFT => Some(VirtualKeyCode::Shift as u16),
        VK_RSHIFT => Some(VirtualKeyCode::RightShift as u16),
        VK_CONTROL | VK_LCONTROL => Some(VirtualKeyCode::Control as u16),
        VK_RCONTROL => Some(VirtualKeyCode::RightControl as u16),
        VK_MENU | VK_LMENU => Some(VirtualKeyCode::Option as u16),
        VK_RMENU => Some(VirtualKeyCode::RightOption as u16),
        VK_LWIN => Some(VirtualKeyCode::Command as u16),
        VK_RWIN => Some(VirtualKeyCode::RightCommand as u16),
        _ => None,
    }
}

fn internal_to_windows_vk(keycode: u16) -> Option<u16> {
    match VirtualKeyCode::from_raw(keycode) {
        Some(VirtualKeyCode::A) => Some(VK_A),
        Some(VirtualKeyCode::B) => Some(VK_B),
        Some(VirtualKeyCode::C) => Some(VK_C),
        Some(VirtualKeyCode::D) => Some(VK_D),
        Some(VirtualKeyCode::E) => Some(VK_E),
        Some(VirtualKeyCode::F) => Some(VK_F),
        Some(VirtualKeyCode::G) => Some(VK_G),
        Some(VirtualKeyCode::H) => Some(VK_H),
        Some(VirtualKeyCode::I) => Some(VK_I),
        Some(VirtualKeyCode::J) => Some(VK_J),
        Some(VirtualKeyCode::K) => Some(VK_K),
        Some(VirtualKeyCode::L) => Some(VK_L),
        Some(VirtualKeyCode::M) => Some(VK_M),
        Some(VirtualKeyCode::N) => Some(VK_N),
        Some(VirtualKeyCode::O) => Some(VK_O),
        Some(VirtualKeyCode::P) => Some(VK_P),
        Some(VirtualKeyCode::Q) => Some(VK_Q),
        Some(VirtualKeyCode::R) => Some(VK_R),
        Some(VirtualKeyCode::S) => Some(VK_S),
        Some(VirtualKeyCode::T) => Some(VK_T),
        Some(VirtualKeyCode::U) => Some(VK_U),
        Some(VirtualKeyCode::V) => Some(VK_V),
        Some(VirtualKeyCode::W) => Some(VK_W),
        Some(VirtualKeyCode::X) => Some(VK_X),
        Some(VirtualKeyCode::Y) => Some(VK_Y),
        Some(VirtualKeyCode::Z) => Some(VK_Z),
        Some(VirtualKeyCode::Key0) => Some(VK_0),
        Some(VirtualKeyCode::Key1) => Some(VK_1),
        Some(VirtualKeyCode::Key2) => Some(VK_2),
        Some(VirtualKeyCode::Key3) => Some(VK_3),
        Some(VirtualKeyCode::Key4) => Some(VK_4),
        Some(VirtualKeyCode::Key5) => Some(VK_5),
        Some(VirtualKeyCode::Key6) => Some(VK_6),
        Some(VirtualKeyCode::Key7) => Some(VK_7),
        Some(VirtualKeyCode::Key8) => Some(VK_8),
        Some(VirtualKeyCode::Key9) => Some(VK_9),
        Some(VirtualKeyCode::Grave) => Some(VK_OEM_3),
        Some(VirtualKeyCode::Minus) => Some(VK_OEM_MINUS),
        Some(VirtualKeyCode::Equal) => Some(VK_OEM_PLUS),
        Some(VirtualKeyCode::LeftBracket) => Some(VK_OEM_4),
        Some(VirtualKeyCode::RightBracket) => Some(VK_OEM_6),
        Some(VirtualKeyCode::Backslash) => Some(VK_OEM_5),
        Some(VirtualKeyCode::Semicolon) => Some(VK_OEM_1),
        Some(VirtualKeyCode::Comma) => Some(VK_OEM_COMMA),
        Some(VirtualKeyCode::Period) => Some(VK_OEM_PERIOD),
        Some(VirtualKeyCode::Slash) => Some(VK_OEM_2),
        Some(VirtualKeyCode::Return) => Some(VK_RETURN),
        Some(VirtualKeyCode::Tab) => Some(VK_TAB),
        Some(VirtualKeyCode::Space) => Some(VK_SPACE),
        Some(VirtualKeyCode::Delete) => Some(VK_BACK),
        Some(VirtualKeyCode::Escape) => Some(VK_ESCAPE),
        Some(VirtualKeyCode::LeftArrow) => Some(VK_LEFT),
        Some(VirtualKeyCode::RightArrow) => Some(VK_RIGHT),
        Some(VirtualKeyCode::DownArrow) => Some(VK_DOWN),
        Some(VirtualKeyCode::UpArrow) => Some(VK_UP),
        Some(VirtualKeyCode::Home) => Some(VK_HOME),
        Some(VirtualKeyCode::End) => Some(VK_END),
        Some(VirtualKeyCode::Shift) => Some(VK_SHIFT),
        Some(VirtualKeyCode::RightShift) => Some(VK_RSHIFT),
        Some(VirtualKeyCode::Control) => Some(VK_CONTROL),
        Some(VirtualKeyCode::RightControl) => Some(VK_RCONTROL),
        Some(VirtualKeyCode::Option) => Some(VK_MENU),
        Some(VirtualKeyCode::RightOption) => Some(VK_RMENU),
        Some(VirtualKeyCode::Command) => Some(VK_LWIN),
        Some(VirtualKeyCode::RightCommand) => Some(VK_RWIN),
        _ => None,
    }
}
