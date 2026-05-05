use std::collections::{HashMap, HashSet};
use yorling_core::keycode::{self, VirtualKeyCode};

/// Action the interceptor should take for a given key event.
#[derive(Debug, Clone)]
pub enum EngineAction {
    /// Pass the original event through unmodified.
    PassThrough,
    /// Suppress the event (don't forward it).
    Suppress,
    /// Emit one or more synthetic key events instead.
    Emit(Vec<SyntheticKey>),
    /// Dispatch a non-keyboard system action to the platform layer.
    System(SystemAction),
    /// Emit synthetic key events and dispatch a system action together.
    EmitAndSystem(Vec<SyntheticKey>, SystemAction),
}

#[derive(Debug, Clone)]
pub struct SyntheticKey {
    pub keycode: u16,
    pub key_down: bool,
    /// Whether to OR-in modifier flags from the original event.
    /// True for navigation keys (Shift+arrow selection should work),
    /// false for action keys and fallback space taps.
    pub preserve_flags: bool,
    /// Additional modifier flags to set on the synthetic event.
    /// OR'd with preserved flags when preserve_flags is true.
    pub extra_flags: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketPair {
    Round,
    Square,
    Curly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseMoveDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseScrollDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemAction {
    AltTabCycle {
        reverse: bool,
    },
    AltTabCommit,
    AltTabCancel,
    BracketModeChanged {
        visible: bool,
        pending_pair: Option<BracketPair>,
    },
    /// Current mouse movement vector while mouse mode is active.
    /// `x` and `y` are normalized to -1, 0, or 1 so the platform layer can
    /// drive smooth continuous motion independently of keyboard autorepeat.
    MouseMove {
        x: i8,
        y: i8,
        fast: bool,
    },
    MouseClick {
        button: MouseButton,
    },
    MouseScroll {
        direction: MouseScrollDirection,
    },
    MouseModeChanged {
        active: bool,
    },
}

/// Defines how a source key is remapped when a hold modifier is active.
#[derive(Debug, Clone)]
enum KeyMapping {
    /// Single key remap (holdable: down on source-down, up on source-up).
    Single {
        target_keycode: u16,
        extra_flags: u64,
        preserve_flags: bool,
    },
    /// Macro sequence: emit all steps (each as down+up) on source key-down.
    /// Nothing emitted on source key-up.
    Sequence(Vec<SequenceStep>),
}

/// A single step in a macro sequence.
#[derive(Debug, Clone)]
struct SequenceStep {
    keycode: u16,
    extra_flags: u64,
}

#[derive(Debug, Clone)]
struct HoldLayer {
    tap_target_keycode: u16,
    tap_preserve_flags: bool,
    mappings: HashMap<u16, KeyMapping>,
}

/// Tap-hold state machine for the active hold modifier.
///
/// States:
///   Idle                           → Modifier↓ → HoldPending(modifier)
///   HoldPending(modifier)          → MappedKey↓ → Holding(modifier)
///   HoldPending(modifier)          → Modifier↑ (unused) → Idle (emit tap)
///   HoldPending(modifier)          → NonMappedKey↓ → AwaitModifierRelease(modifier)
///   Holding(modifier)              → Modifier↑ → Idle (release held remaps)
///   Holding(modifier)              → MappedKey↓ → Holding(modifier)
///   Holding(modifier)              → MappedKey↑ (tracked) → Holding(modifier)
///   AwaitModifierRelease(modifier) → Modifier↑ → Idle (suppress orphaned release)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldState {
    Idle,
    HoldPending(u16),
    Holding(u16),
    AwaitModifierRelease(u16),
}

/// Tracks a held key for proper release.
#[derive(Debug, Clone)]
enum HeldKeyInfo {
    /// Single mapping: emit key-up on release.
    Single {
        target_keycode: u16,
        extra_flags: u64,
    },
    /// Sequence mapping: just suppress the key-up, no release event needed.
    Sequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BracketModeState {
    Inactive,
    WaitingForPrefix,
    WaitingForConfirm {
        pending_pair: BracketPair,
        prefix_keycode: u16,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseModeState {
    /// Mouse mode not active.
    Inactive,
    /// Tab held, waiting for a mouse-layer key to confirm mouse mode.
    Pending,
    /// Mouse mode confirmed — JKLI moves cursor, R/N/M trigger mouse actions.
    Active,
}

/// Result of processing a key in mouse mode.
enum MouseModeResult {
    /// Key was handled; return this action.
    Handled(EngineAction),
    /// Mouse mode exited; reprocess the key through normal engine logic.
    ExitAndReprocess,
}

#[derive(Debug, Clone, Copy)]
struct ReplayKeyInfo {
    preserve_flags: bool,
    extra_flags: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingPlatform {
    Macos,
    Windows,
}

pub struct MappingEngine {
    hold_state: HoldState,
    /// Hold modifier keycode → layer definition
    hold_layers: HashMap<u16, HoldLayer>,
    /// Currently held mapped keys: source keycode → held key info.
    /// Used to emit correct key-ups when a hold modifier is released or the engine is disabled.
    held_keys: HashMap<u16, HeldKeyInfo>,
    /// Synthetic replays emitted while resolving transient modes such as bracket mode.
    replayed_keys: HashMap<u16, ReplayKeyInfo>,
    /// Key releases that should be swallowed because their key-down was fully consumed.
    suppressed_releases: HashSet<u16>,
    /// Keycodes where Control modifier should be remapped to Command.
    /// Enables Windows-style Ctrl+key shortcuts on Mac (e.g. Ctrl+C → Cmd+C).
    ctrl_to_cmd_keys: HashSet<u16>,
    /// Currently held Ctrl→Cmd remapped keys, for correct key-up handling
    /// even when Ctrl is released before the letter key.
    ctrl_to_cmd_held: HashSet<u16>,
    /// Currently held Win+Shift+S remap source keys so key-up can still emit
    /// Cmd+Shift+4 even if Command/Shift are released first.
    win_shift_screenshot_held: HashSet<u16>,
    mac_compatibility_shortcuts_enabled: bool,
    alt_tab_shortcut_enabled: bool,
    mouse_sidebar_shortcut_keycode: u16,
    mouse_sidebar_shortcut_flags: u64,
    alt_tab_active: bool,
    bracket_mode: BracketModeState,
    mouse_mode: MouseModeState,
    /// Keys currently held in mouse mode, added to suppressed_releases on exit.
    mouse_held_keys: HashSet<u16>,
    /// Whether Tab is physically held while in Active mouse mode (for fast movement).
    tab_held_in_mouse_mode: bool,
    enabled: bool,
    event_count: u64,
}

impl MappingEngine {
    pub fn new() -> Self {
        Self::new_for_platform(MappingPlatform::Macos)
    }

    pub fn new_for_platform(platform: MappingPlatform) -> Self {
        let is_windows = platform == MappingPlatform::Windows;
        let mut space_mappings = HashMap::new();
        let (line_start_keycode, line_start_flags) = if is_windows {
            (VirtualKeyCode::Home as u16, 0)
        } else {
            (VirtualKeyCode::LeftArrow as u16, keycode::FLAG_COMMAND)
        };
        let (line_end_keycode, line_end_flags) = if is_windows {
            (VirtualKeyCode::End as u16, 0)
        } else {
            (VirtualKeyCode::RightArrow as u16, keycode::FLAG_COMMAND)
        };
        let word_jump_flags = if is_windows {
            keycode::FLAG_CONTROL
        } else {
            keycode::FLAG_OPTION
        };
        let delete_word_flags = word_jump_flags;
        let (mouse_sidebar_shortcut_keycode, mouse_sidebar_shortcut_flags) = if is_windows {
            (VirtualKeyCode::B as u16, keycode::FLAG_CONTROL)
        } else {
            (VirtualKeyCode::LeftBracket as u16, keycode::FLAG_COMMAND)
        };

        // === Navigation: Arrow keys ===
        // Space + J → Left Arrow
        space_mappings.insert(
            VirtualKeyCode::J as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::LeftArrow as u16,
                extra_flags: 0,
                preserve_flags: true,
            },
        );
        // Space + K → Down Arrow
        space_mappings.insert(
            VirtualKeyCode::K as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::DownArrow as u16,
                extra_flags: 0,
                preserve_flags: true,
            },
        );
        // Space + I → Up Arrow
        space_mappings.insert(
            VirtualKeyCode::I as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::UpArrow as u16,
                extra_flags: 0,
                preserve_flags: true,
            },
        );
        // Space + L → Right Arrow
        space_mappings.insert(
            VirtualKeyCode::L as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::RightArrow as u16,
                extra_flags: 0,
                preserve_flags: true,
            },
        );

        // === Navigation: Word/Line jump ===
        // Space + H → line start (Cmd+Left on macOS, Home on Windows)
        space_mappings.insert(
            VirtualKeyCode::H as u16,
            KeyMapping::Single {
                target_keycode: line_start_keycode,
                extra_flags: line_start_flags,
                preserve_flags: true,
            },
        );
        // Space + N → line end (Cmd+Right on macOS, End on Windows)
        space_mappings.insert(
            VirtualKeyCode::N as u16,
            KeyMapping::Single {
                target_keycode: line_end_keycode,
                extra_flags: line_end_flags,
                preserve_flags: true,
            },
        );
        // Space + U → word left (Option+Left on macOS, Ctrl+Left on Windows)
        space_mappings.insert(
            VirtualKeyCode::U as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::LeftArrow as u16,
                extra_flags: word_jump_flags,
                preserve_flags: true,
            },
        );
        // Space + O → word right (Option+Right on macOS, Ctrl+Right on Windows)
        space_mappings.insert(
            VirtualKeyCode::O as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::RightArrow as u16,
                extra_flags: word_jump_flags,
                preserve_flags: true,
            },
        );

        // === Editing ===
        // Space + Q → 4 × Space (indent with spaces)
        space_mappings.insert(
            VirtualKeyCode::Q as u16,
            KeyMapping::Sequence(vec![
                SequenceStep {
                    keycode: VirtualKeyCode::Space as u16,
                    extra_flags: 0,
                },
                SequenceStep {
                    keycode: VirtualKeyCode::Space as u16,
                    extra_flags: 0,
                },
                SequenceStep {
                    keycode: VirtualKeyCode::Space as u16,
                    extra_flags: 0,
                },
                SequenceStep {
                    keycode: VirtualKeyCode::Space as u16,
                    extra_flags: 0,
                },
            ]),
        );
        // Space + W → Delete line using each platform's native line-start/end shortcuts.
        space_mappings.insert(
            VirtualKeyCode::W as u16,
            KeyMapping::Sequence(vec![
                SequenceStep {
                    keycode: line_start_keycode,
                    extra_flags: line_start_flags,
                },
                SequenceStep {
                    keycode: line_end_keycode,
                    extra_flags: keycode::FLAG_SHIFT | line_end_flags,
                },
                SequenceStep {
                    keycode: VirtualKeyCode::Delete as u16,
                    extra_flags: 0,
                },
            ]),
        );
        // Space + E → Backspace (Delete key on macOS)
        space_mappings.insert(
            VirtualKeyCode::E as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Delete as u16,
                extra_flags: 0,
                preserve_flags: false,
            },
        );
        // Space + R → delete word backward (Option+Delete on macOS, Ctrl+Backspace on Windows)
        space_mappings.insert(
            VirtualKeyCode::R as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Delete as u16,
                extra_flags: delete_word_flags,
                preserve_flags: false,
            },
        );
        // Space + A → Return (Enter)
        space_mappings.insert(
            VirtualKeyCode::A as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Return as u16,
                extra_flags: 0,
                preserve_flags: false,
            },
        );
        // Space + S → Escape
        space_mappings.insert(
            VirtualKeyCode::S as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Escape as u16,
                extra_flags: 0,
                preserve_flags: false,
            },
        );

        // === System: Tab switching ===
        // Space + Z → Ctrl+Shift+Tab (previous tab)
        space_mappings.insert(
            VirtualKeyCode::Z as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Tab as u16,
                extra_flags: keycode::FLAG_CONTROL | keycode::FLAG_SHIFT,
                preserve_flags: false,
            },
        );
        // Space + V → Ctrl+Tab (next tab)
        space_mappings.insert(
            VirtualKeyCode::V as u16,
            KeyMapping::Single {
                target_keycode: VirtualKeyCode::Tab as u16,
                extra_flags: keycode::FLAG_CONTROL,
                preserve_flags: false,
            },
        );

        let mut digit_mappings = HashMap::new();

        for (source_keycode, target_keycode) in [
            (VirtualKeyCode::N as u16, VirtualKeyCode::Key1 as u16),
            (VirtualKeyCode::M as u16, VirtualKeyCode::Key2 as u16),
            (VirtualKeyCode::Comma as u16, VirtualKeyCode::Key3 as u16),
            (VirtualKeyCode::J as u16, VirtualKeyCode::Key4 as u16),
            (VirtualKeyCode::K as u16, VirtualKeyCode::Key5 as u16),
            (VirtualKeyCode::L as u16, VirtualKeyCode::Key6 as u16),
            (VirtualKeyCode::U as u16, VirtualKeyCode::Key7 as u16),
            (VirtualKeyCode::I as u16, VirtualKeyCode::Key8 as u16),
            (VirtualKeyCode::O as u16, VirtualKeyCode::Key9 as u16),
            (VirtualKeyCode::B as u16, VirtualKeyCode::Key0 as u16),
            (VirtualKeyCode::H as u16, VirtualKeyCode::Key0 as u16),
        ] {
            digit_mappings.insert(
                source_keycode,
                KeyMapping::Single {
                    target_keycode,
                    extra_flags: 0,
                    preserve_flags: false,
                },
            );
        }

        let mut semicolon_mappings = HashMap::new();

        for (source_keycode, target_keycode, extra_flags) in [
            (VirtualKeyCode::A as u16, VirtualKeyCode::Minus as u16, 0),
            (
                VirtualKeyCode::B as u16,
                VirtualKeyCode::Key5 as u16,
                keycode::FLAG_SHIFT,
            ),
            (VirtualKeyCode::C as u16, VirtualKeyCode::Period as u16, 0),
            (VirtualKeyCode::D as u16, VirtualKeyCode::Equal as u16, 0),
            (
                VirtualKeyCode::E as u16,
                VirtualKeyCode::Key6 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::F as u16,
                VirtualKeyCode::Period as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::G as u16,
                VirtualKeyCode::Key1 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::H as u16,
                VirtualKeyCode::Equal as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::I as u16,
                VirtualKeyCode::Semicolon as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::J as u16,
                VirtualKeyCode::Semicolon as u16,
                0,
            ),
            (VirtualKeyCode::K as u16, VirtualKeyCode::Grave as u16, 0),
            (VirtualKeyCode::N as u16, VirtualKeyCode::Slash as u16, 0),
            (VirtualKeyCode::Q as u16, VirtualKeyCode::Minus as u16, 0),
            (
                VirtualKeyCode::R as u16,
                VirtualKeyCode::Key7 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::S as u16,
                VirtualKeyCode::Comma as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::T as u16,
                VirtualKeyCode::Grave as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::U as u16,
                VirtualKeyCode::Key4 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::V as u16,
                VirtualKeyCode::Backslash as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::W as u16,
                VirtualKeyCode::Key3 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::X as u16,
                VirtualKeyCode::Minus as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::Y as u16,
                VirtualKeyCode::Key2 as u16,
                keycode::FLAG_SHIFT,
            ),
            (
                VirtualKeyCode::Z as u16,
                VirtualKeyCode::Backslash as u16,
                0,
            ),
        ] {
            semicolon_mappings.insert(
                source_keycode,
                KeyMapping::Single {
                    target_keycode,
                    extra_flags,
                    preserve_flags: false,
                },
            );
        }

        let mut hold_layers = HashMap::new();
        hold_layers.insert(
            VirtualKeyCode::Space as u16,
            HoldLayer {
                tap_target_keycode: VirtualKeyCode::Space as u16,
                tap_preserve_flags: false,
                mappings: space_mappings,
            },
        );
        hold_layers.insert(
            VirtualKeyCode::Key3 as u16,
            HoldLayer {
                tap_target_keycode: VirtualKeyCode::Key3 as u16,
                tap_preserve_flags: true,
                mappings: digit_mappings,
            },
        );
        hold_layers.insert(
            VirtualKeyCode::Semicolon as u16,
            HoldLayer {
                tap_target_keycode: VirtualKeyCode::Semicolon as u16,
                tap_preserve_flags: true,
                mappings: semicolon_mappings,
            },
        );

        // === macOS compatibility for Windows muscle memory: Ctrl→Cmd remapping ===
        let ctrl_to_cmd_keys: HashSet<u16> = if is_windows {
            HashSet::new()
        } else {
            [
                VirtualKeyCode::C, // Copy
                VirtualKeyCode::V, // Paste
                VirtualKeyCode::X, // Cut
                VirtualKeyCode::S, // Save
                VirtualKeyCode::Z, // Undo
                VirtualKeyCode::A, // Select All
            ]
            .iter()
            .map(|k| *k as u16)
            .collect()
        };

        Self {
            hold_state: HoldState::Idle,
            hold_layers,
            held_keys: HashMap::new(),
            replayed_keys: HashMap::new(),
            suppressed_releases: HashSet::new(),
            ctrl_to_cmd_keys,
            ctrl_to_cmd_held: HashSet::new(),
            win_shift_screenshot_held: HashSet::new(),
            mac_compatibility_shortcuts_enabled: !is_windows,
            alt_tab_shortcut_enabled: !is_windows,
            mouse_sidebar_shortcut_keycode,
            mouse_sidebar_shortcut_flags,
            alt_tab_active: false,
            bracket_mode: BracketModeState::Inactive,
            mouse_mode: MouseModeState::Inactive,
            mouse_held_keys: HashSet::new(),
            tab_held_in_mouse_mode: false,
            enabled: true,
            event_count: 0,
        }
    }

    /// Set enabled state. Returns any synthetic key-ups that must be emitted
    /// to release stuck remapped keys (e.g. when disabling mid-hold).
    pub fn set_enabled(&mut self, enabled: bool) -> Vec<SyntheticKey> {
        self.enabled = enabled;
        if !enabled {
            self.hold_state = HoldState::Idle;
            self.alt_tab_active = false;
            self.bracket_mode = BracketModeState::Inactive;
            self.mouse_mode = MouseModeState::Inactive;
            self.mouse_held_keys.clear();
            self.tab_held_in_mouse_mode = false;
            self.suppressed_releases.clear();
            self.win_shift_screenshot_held.clear();
            let mut releases = self.drain_held_keys();
            releases.extend(self.drain_replayed_keys());
            releases
        } else {
            Vec::new()
        }
    }

    pub fn cancel_alt_tab_session(&mut self) {
        self.alt_tab_active = false;
    }

    /// Cancel bracket mode (e.g. on mouse click).  Returns the system action
    /// that must be dispatched to hide the overlay, or `None` if already inactive.
    pub fn cancel_bracket_mode(&mut self) -> Option<SystemAction> {
        if self.bracket_mode != BracketModeState::Inactive {
            self.bracket_mode = BracketModeState::Inactive;
            Some(Self::bracket_mode_action(false, None))
        } else {
            None
        }
    }

    /// Cancel mouse mode (e.g. on physical mouse click). Returns the system action
    /// that must be dispatched to notify the UI, or `None` if already inactive.
    pub fn cancel_mouse_mode(&mut self) -> Option<SystemAction> {
        if self.mouse_mode != MouseModeState::Inactive {
            self.exit_mouse_mode();
            Some(SystemAction::MouseModeChanged { active: false })
        } else {
            None
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_mouse_mode_active(&self) -> bool {
        self.mouse_mode != MouseModeState::Inactive
    }

    pub fn event_count(&self) -> u64 {
        self.event_count
    }

    /// Drain all held keys, returning key-up events for Single mappings.
    fn drain_held_keys(&mut self) -> Vec<SyntheticKey> {
        let releases: Vec<SyntheticKey> = self
            .held_keys
            .values()
            .filter_map(|info| match info {
                HeldKeyInfo::Single {
                    target_keycode,
                    extra_flags,
                } => Some(SyntheticKey {
                    keycode: *target_keycode,
                    key_down: false,
                    preserve_flags: false,
                    extra_flags: *extra_flags,
                }),
                HeldKeyInfo::Sequence => None,
            })
            .collect();
        self.held_keys.clear();
        releases
    }

    fn drain_replayed_keys(&mut self) -> Vec<SyntheticKey> {
        let releases: Vec<SyntheticKey> = self
            .replayed_keys
            .iter()
            .map(|(keycode, info)| SyntheticKey {
                keycode: *keycode,
                key_down: false,
                preserve_flags: info.preserve_flags,
                extra_flags: info.extra_flags,
            })
            .collect();
        self.replayed_keys.clear();
        releases
    }

    fn current_modifier_keycode(&self) -> Option<u16> {
        match self.hold_state {
            HoldState::Idle => None,
            HoldState::HoldPending(modifier_keycode)
            | HoldState::Holding(modifier_keycode)
            | HoldState::AwaitModifierRelease(modifier_keycode) => Some(modifier_keycode),
        }
    }

    fn layer_mapping(&self, modifier_keycode: u16, source_keycode: u16) -> Option<&KeyMapping> {
        self.hold_layers
            .get(&modifier_keycode)
            .and_then(|layer| layer.mappings.get(&source_keycode))
    }

    fn emit_modifier_tap(&self, modifier_keycode: u16) -> Vec<SyntheticKey> {
        let layer = self
            .hold_layers
            .get(&modifier_keycode)
            .expect("hold layer should exist for active modifier");

        vec![
            SyntheticKey {
                keycode: layer.tap_target_keycode,
                key_down: true,
                preserve_flags: layer.tap_preserve_flags,
                extra_flags: 0,
            },
            SyntheticKey {
                keycode: layer.tap_target_keycode,
                key_down: false,
                preserve_flags: layer.tap_preserve_flags,
                extra_flags: 0,
            },
        ]
    }

    fn emit_fallback_tap_and_key(&self, modifier_keycode: u16, keycode: u16) -> EngineAction {
        let mut events = self.emit_modifier_tap(modifier_keycode);
        events.push(SyntheticKey {
            keycode,
            key_down: true,
            preserve_flags: true,
            extra_flags: 0,
        });
        EngineAction::Emit(events)
    }

    fn bracket_mode_action(visible: bool, pending_pair: Option<BracketPair>) -> SystemAction {
        SystemAction::BracketModeChanged {
            visible,
            pending_pair,
        }
    }

    fn emit_replayed_key_down_and_system(
        &mut self,
        keycode: u16,
        system: SystemAction,
    ) -> EngineAction {
        self.replayed_keys.insert(
            keycode,
            ReplayKeyInfo {
                preserve_flags: true,
                extra_flags: 0,
            },
        );
        EngineAction::EmitAndSystem(
            vec![SyntheticKey {
                keycode,
                key_down: true,
                preserve_flags: true,
                extra_flags: 0,
            }],
            system,
        )
    }

    fn emit_bracket_prefix_fallback(
        &mut self,
        prefix_keycode: u16,
        keycode: u16,
        system: SystemAction,
    ) -> EngineAction {
        self.replayed_keys.insert(
            keycode,
            ReplayKeyInfo {
                preserve_flags: true,
                extra_flags: 0,
            },
        );
        EngineAction::EmitAndSystem(
            vec![
                SyntheticKey {
                    keycode: prefix_keycode,
                    key_down: true,
                    preserve_flags: true,
                    extra_flags: 0,
                },
                SyntheticKey {
                    keycode: prefix_keycode,
                    key_down: false,
                    preserve_flags: true,
                    extra_flags: 0,
                },
                SyntheticKey {
                    keycode,
                    key_down: true,
                    preserve_flags: true,
                    extra_flags: 0,
                },
            ],
            system,
        )
    }

    fn emit_bracket_pair(&self, pair: BracketPair) -> Vec<SyntheticKey> {
        let (open_keycode, open_flags, close_keycode, close_flags) = match pair {
            BracketPair::Round => (
                VirtualKeyCode::Key9 as u16,
                keycode::FLAG_SHIFT,
                VirtualKeyCode::Key0 as u16,
                keycode::FLAG_SHIFT,
            ),
            BracketPair::Square => (
                VirtualKeyCode::LeftBracket as u16,
                0,
                VirtualKeyCode::RightBracket as u16,
                0,
            ),
            BracketPair::Curly => (
                VirtualKeyCode::LeftBracket as u16,
                keycode::FLAG_SHIFT,
                VirtualKeyCode::RightBracket as u16,
                keycode::FLAG_SHIFT,
            ),
        };

        vec![
            SyntheticKey {
                keycode: open_keycode,
                key_down: true,
                preserve_flags: false,
                extra_flags: open_flags,
            },
            SyntheticKey {
                keycode: open_keycode,
                key_down: false,
                preserve_flags: false,
                extra_flags: open_flags,
            },
            SyntheticKey {
                keycode: close_keycode,
                key_down: true,
                preserve_flags: false,
                extra_flags: close_flags,
            },
            SyntheticKey {
                keycode: close_keycode,
                key_down: false,
                preserve_flags: false,
                extra_flags: close_flags,
            },
        ]
    }

    fn bracket_pair_for_prefix(keycode: u16) -> Option<BracketPair> {
        match VirtualKeyCode::from_raw(keycode) {
            Some(VirtualKeyCode::X) => Some(BracketPair::Round),
            Some(VirtualKeyCode::Z) => Some(BracketPair::Square),
            Some(VirtualKeyCode::D) => Some(BracketPair::Curly),
            _ => None,
        }
    }

    // ── Mouse mode helpers ──

    fn is_mouse_move_key(keycode: u16) -> Option<MouseMoveDirection> {
        match VirtualKeyCode::from_raw(keycode) {
            Some(VirtualKeyCode::J) => Some(MouseMoveDirection::Left),
            Some(VirtualKeyCode::K) => Some(MouseMoveDirection::Down),
            Some(VirtualKeyCode::L) => Some(MouseMoveDirection::Right),
            Some(VirtualKeyCode::I) => Some(MouseMoveDirection::Up),
            _ => None,
        }
    }

    fn mouse_move_vector_for_direction(direction: MouseMoveDirection) -> (i8, i8) {
        match direction {
            MouseMoveDirection::Left => (-1, 0),
            MouseMoveDirection::Right => (1, 0),
            MouseMoveDirection::Up => (0, -1),
            MouseMoveDirection::Down => (0, 1),
        }
    }

    fn has_mouse_move_keys_held(&self) -> bool {
        self.mouse_held_keys
            .iter()
            .any(|keycode| Self::is_mouse_move_key(*keycode).is_some())
    }

    fn current_mouse_move_action(&self) -> SystemAction {
        let (mut x, mut y) = (0i8, 0i8);
        for keycode in &self.mouse_held_keys {
            if let Some(direction) = Self::is_mouse_move_key(*keycode) {
                let (dx, dy) = Self::mouse_move_vector_for_direction(direction);
                x += dx;
                y += dy;
            }
        }

        SystemAction::MouseMove {
            x: x.clamp(-1, 1),
            y: y.clamp(-1, 1),
            fast: self.tab_held_in_mouse_mode,
        }
    }

    fn is_mouse_scroll_key(keycode: u16) -> Option<MouseScrollDirection> {
        match VirtualKeyCode::from_raw(keycode) {
            Some(VirtualKeyCode::U) => Some(MouseScrollDirection::Up),
            Some(VirtualKeyCode::O) => Some(MouseScrollDirection::Down),
            _ => None,
        }
    }

    fn shortcut_action(target_keycode: u16, extra_flags: u64) -> EngineAction {
        EngineAction::Emit(vec![
            SyntheticKey {
                keycode: target_keycode,
                key_down: true,
                preserve_flags: false,
                extra_flags,
            },
            SyntheticKey {
                keycode: target_keycode,
                key_down: false,
                preserve_flags: false,
                extra_flags,
            },
        ])
    }

    /// Clean up mouse mode state and add held keys to suppressed_releases.
    fn exit_mouse_mode(&mut self) {
        self.mouse_mode = MouseModeState::Inactive;
        // If Tab is still physically held, suppress its eventual release.
        if self.tab_held_in_mouse_mode {
            self.suppressed_releases.insert(VirtualKeyCode::Tab as u16);
        }
        self.tab_held_in_mouse_mode = false;
        for kc in self.mouse_held_keys.drain() {
            self.suppressed_releases.insert(kc);
        }
    }

    fn process_mouse_mode_key(
        &mut self,
        keycode: u16,
        key_down: bool,
        is_autorepeat: bool,
        _flags: u64,
    ) -> MouseModeResult {
        let tab_keycode = VirtualKeyCode::Tab as u16;

        match self.mouse_mode {
            MouseModeState::Inactive => unreachable!("should not be called when inactive"),

            MouseModeState::Pending => {
                // Tab autorepeat or Tab key-up
                if keycode == tab_keycode {
                    if !key_down {
                        // Tab released without a mouse-layer key → emit Tab tap, exit
                        self.mouse_mode = MouseModeState::Inactive;
                        return MouseModeResult::Handled(EngineAction::Emit(vec![
                            SyntheticKey {
                                keycode: tab_keycode,
                                key_down: true,
                                preserve_flags: false,
                                extra_flags: 0,
                            },
                            SyntheticKey {
                                keycode: tab_keycode,
                                key_down: false,
                                preserve_flags: false,
                                extra_flags: 0,
                            },
                        ]));
                    }
                    // Tab autorepeat → suppress
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // Ignore key-ups in Pending (they belong to previously pressed keys)
                if !key_down {
                    return MouseModeResult::Handled(EngineAction::PassThrough);
                }

                // JKLI → confirm mouse mode, emit move (fast since Tab is held)
                if Self::is_mouse_move_key(keycode).is_some() {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    // Don't add Tab to suppressed_releases here — the Active handler
                    // manages Tab directly. Tab is added on exit_mouse_mode() if still held.
                    self.mouse_held_keys.insert(keycode);
                    return MouseModeResult::Handled(EngineAction::System(
                        self.current_mouse_move_action(),
                    ));
                }

                // R → enter mouse mode and trigger mouse back immediately.
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::R)) {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    self.mouse_held_keys.insert(keycode);
                    return MouseModeResult::Handled(EngineAction::System(
                        SystemAction::MouseClick {
                            button: MouseButton::Back,
                        },
                    ));
                }

                // N → enter mouse mode, click, then exit without leaking Tab/N key-up.
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::N)) {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    self.mouse_held_keys.insert(keycode);
                    self.exit_mouse_mode();
                    return MouseModeResult::Handled(EngineAction::System(
                        SystemAction::MouseClick {
                            button: MouseButton::Left,
                        },
                    ));
                }

                // U/O → enter mouse mode and scroll immediately.
                if let Some(direction) = Self::is_mouse_scroll_key(keycode) {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    self.mouse_held_keys.insert(keycode);
                    return MouseModeResult::Handled(EngineAction::System(
                        SystemAction::MouseScroll { direction },
                    ));
                }

                // C → enter mouse mode and trigger the platform sidebar shortcut.
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::C)) {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    self.mouse_held_keys.insert(keycode);
                    return MouseModeResult::Handled(Self::shortcut_action(
                        self.mouse_sidebar_shortcut_keycode,
                        self.mouse_sidebar_shortcut_flags,
                    ));
                }

                // M → enter mouse mode and right-click immediately.
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::M)) {
                    self.mouse_mode = MouseModeState::Active;
                    self.tab_held_in_mouse_mode = true;
                    self.mouse_held_keys.insert(keycode);
                    return MouseModeResult::Handled(EngineAction::System(
                        SystemAction::MouseClick {
                            button: MouseButton::Right,
                        },
                    ));
                }

                // Any other key → cancel, emit Tab tap + triggering key as synthetics.
                // Same approach as bracket mode fallback: emit both keys as
                // synthetic events and track them in replayed_keys for key-up.
                // This avoids the triggering key being silently dropped (modifier
                // case) or producing an orphan Tab↑ (non-modifier case).
                self.mouse_mode = MouseModeState::Inactive;
                self.replayed_keys.insert(
                    tab_keycode,
                    ReplayKeyInfo {
                        preserve_flags: false,
                        extra_flags: 0,
                    },
                );
                self.replayed_keys.insert(
                    keycode,
                    ReplayKeyInfo {
                        preserve_flags: true,
                        extra_flags: 0,
                    },
                );
                MouseModeResult::Handled(EngineAction::Emit(vec![
                    SyntheticKey {
                        keycode: tab_keycode,
                        key_down: true,
                        preserve_flags: false,
                        extra_flags: 0,
                    },
                    SyntheticKey {
                        keycode,
                        key_down: true,
                        preserve_flags: true,
                        extra_flags: 0,
                    },
                ]))
            }

            MouseModeState::Active => {
                // Tab key in Active → track held state for speed control
                if keycode == tab_keycode {
                    let speed_changed = self.tab_held_in_mouse_mode != key_down;
                    if key_down {
                        self.tab_held_in_mouse_mode = true;
                        // Don't add to suppressed_releases — we handle Tab directly here.
                    } else {
                        self.tab_held_in_mouse_mode = false;
                    }
                    if speed_changed && self.has_mouse_move_keys_held() {
                        return MouseModeResult::Handled(EngineAction::System(
                            self.current_mouse_move_action(),
                        ));
                    }
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // JKLI → update current mouse movement vector.
                if Self::is_mouse_move_key(keycode).is_some() {
                    if key_down {
                        if is_autorepeat || !self.mouse_held_keys.insert(keycode) {
                            return MouseModeResult::Handled(EngineAction::Suppress);
                        }
                        return MouseModeResult::Handled(EngineAction::System(
                            self.current_mouse_move_action(),
                        ));
                    } else {
                        if !self.mouse_held_keys.remove(&keycode) {
                            return MouseModeResult::Handled(EngineAction::Suppress);
                        }
                        return MouseModeResult::Handled(EngineAction::System(
                            self.current_mouse_move_action(),
                        ));
                    }
                }

                // U/O → mouse scroll
                if let Some(direction) = Self::is_mouse_scroll_key(keycode) {
                    if key_down {
                        self.mouse_held_keys.insert(keycode);
                        return MouseModeResult::Handled(EngineAction::System(
                            SystemAction::MouseScroll { direction },
                        ));
                    } else {
                        self.mouse_held_keys.remove(&keycode);
                        return MouseModeResult::Handled(EngineAction::Suppress);
                    }
                }

                // R → mouse back button (stays in mouse mode)
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::R)) {
                    if key_down {
                        if is_autorepeat || !self.mouse_held_keys.insert(keycode) {
                            return MouseModeResult::Handled(EngineAction::Suppress);
                        }
                        return MouseModeResult::Handled(EngineAction::System(
                            SystemAction::MouseClick {
                                button: MouseButton::Back,
                            },
                        ));
                    } else {
                        self.mouse_held_keys.remove(&keycode);
                    }
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // C → platform sidebar shortcut (stays in mouse mode)
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::C)) {
                    if key_down {
                        if is_autorepeat || !self.mouse_held_keys.insert(keycode) {
                            return MouseModeResult::Handled(EngineAction::Suppress);
                        }
                        return MouseModeResult::Handled(Self::shortcut_action(
                            self.mouse_sidebar_shortcut_keycode,
                            self.mouse_sidebar_shortcut_flags,
                        ));
                    } else {
                        self.mouse_held_keys.remove(&keycode);
                    }
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // M → right mouse click (stays in mouse mode)
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::M)) {
                    if key_down && !is_autorepeat {
                        self.mouse_held_keys.insert(keycode);
                        return MouseModeResult::Handled(EngineAction::System(
                            SystemAction::MouseClick {
                                button: MouseButton::Right,
                            },
                        ));
                    } else if !key_down {
                        self.mouse_held_keys.remove(&keycode);
                    }
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // N → left mouse click AND exit mouse mode
                if matches!(VirtualKeyCode::from_raw(keycode), Some(VirtualKeyCode::N)) {
                    if key_down && !is_autorepeat {
                        self.mouse_held_keys.insert(keycode);
                        self.exit_mouse_mode();
                        return MouseModeResult::Handled(EngineAction::System(
                            SystemAction::MouseClick {
                                button: MouseButton::Left,
                            },
                        ));
                    }
                    // N key_up or autorepeat after exit: suppress
                    return MouseModeResult::Handled(EngineAction::Suppress);
                }

                // Any other key → exit mouse mode, reprocess key
                self.exit_mouse_mode();
                MouseModeResult::ExitAndReprocess
            }
        }
    }

    fn process_bracket_mode_key(
        &mut self,
        keycode: u16,
        key_down: bool,
        is_autorepeat: bool,
    ) -> EngineAction {
        if is_autorepeat {
            return EngineAction::Suppress;
        }

        match self.bracket_mode {
            BracketModeState::Inactive => EngineAction::PassThrough,
            BracketModeState::WaitingForPrefix => {
                if !key_down {
                    return EngineAction::Suppress;
                }

                if let Some(pair) = Self::bracket_pair_for_prefix(keycode) {
                    self.bracket_mode = BracketModeState::WaitingForConfirm {
                        pending_pair: pair,
                        prefix_keycode: keycode,
                    };
                    EngineAction::System(Self::bracket_mode_action(true, Some(pair)))
                } else {
                    self.bracket_mode = BracketModeState::Inactive;
                    self.emit_replayed_key_down_and_system(
                        keycode,
                        Self::bracket_mode_action(false, None),
                    )
                }
            }
            BracketModeState::WaitingForConfirm {
                pending_pair,
                prefix_keycode,
            } => {
                if !key_down {
                    return EngineAction::Suppress;
                }

                if keycode == VirtualKeyCode::K as u16 {
                    self.bracket_mode = BracketModeState::Inactive;
                    self.suppressed_releases.insert(keycode);
                    return EngineAction::EmitAndSystem(
                        self.emit_bracket_pair(pending_pair),
                        Self::bracket_mode_action(false, None),
                    );
                }

                self.bracket_mode = BracketModeState::Inactive;
                self.emit_bracket_prefix_fallback(
                    prefix_keycode,
                    keycode,
                    Self::bracket_mode_action(false, None),
                )
            }
        }
    }

    /// Emit events for a mapped key-down. Handles both Single and Sequence mappings.
    fn emit_mapping_down(&mut self, modifier_keycode: u16, source_keycode: u16) -> EngineAction {
        let mapping = self
            .layer_mapping(modifier_keycode, source_keycode)
            .expect("mapping should exist for active hold layer")
            .clone();
        match mapping {
            KeyMapping::Single {
                target_keycode,
                extra_flags,
                preserve_flags,
            } => {
                self.held_keys.insert(
                    source_keycode,
                    HeldKeyInfo::Single {
                        target_keycode,
                        extra_flags,
                    },
                );
                EngineAction::Emit(vec![SyntheticKey {
                    keycode: target_keycode,
                    key_down: true,
                    preserve_flags,
                    extra_flags,
                }])
            }
            KeyMapping::Sequence(ref steps) => {
                self.held_keys.insert(source_keycode, HeldKeyInfo::Sequence);
                let mut events = Vec::with_capacity(steps.len() * 2);
                for step in steps {
                    events.push(SyntheticKey {
                        keycode: step.keycode,
                        key_down: true,
                        preserve_flags: false,
                        extra_flags: step.extra_flags,
                    });
                    events.push(SyntheticKey {
                        keycode: step.keycode,
                        key_down: false,
                        preserve_flags: false,
                        extra_flags: step.extra_flags,
                    });
                }
                EngineAction::Emit(events)
            }
        }
    }

    /// Process a key event and return the action to take.
    ///
    /// `keycode`: macOS virtual keycode
    /// `key_down`: true for key press, false for key release
    /// `is_autorepeat`: true if this is an OS key-repeat event
    /// `flags`: modifier flags from the original CGEvent
    pub fn process_key(
        &mut self,
        keycode: u16,
        key_down: bool,
        is_autorepeat: bool,
        flags: u64,
    ) -> EngineAction {
        self.event_count += 1;

        if !self.enabled {
            return EngineAction::PassThrough;
        }

        if !key_down {
            if self.suppressed_releases.remove(&keycode) {
                return EngineAction::Suppress;
            }
            if let Some(info) = self.replayed_keys.remove(&keycode) {
                return EngineAction::Emit(vec![SyntheticKey {
                    keycode,
                    key_down: false,
                    preserve_flags: info.preserve_flags,
                    extra_flags: info.extra_flags,
                }]);
            }
        }

        // Suppress autorepeat for keys held during mouse mode exit (prevent character leak).
        if key_down && is_autorepeat && self.suppressed_releases.contains(&keycode) {
            return EngineAction::Suppress;
        }

        // Pre-process: Control→Command modifier remapping for Windows keyboard compatibility.
        // Only remap when Control is pressed WITHOUT Command (avoid intercepting Ctrl+Cmd combos).
        let screenshot_shortcut_flags = keycode::FLAG_COMMAND | keycode::FLAG_SHIFT;
        if self.mac_compatibility_shortcuts_enabled
            && keycode == VirtualKeyCode::S as u16
            && flags & screenshot_shortcut_flags == screenshot_shortcut_flags
            && flags & (keycode::FLAG_CONTROL | keycode::FLAG_OPTION) == 0
        {
            if is_autorepeat {
                return EngineAction::Suppress;
            }

            if key_down {
                self.win_shift_screenshot_held.insert(keycode);
                if let HoldState::HoldPending(modifier_keycode) = self.hold_state {
                    self.hold_state = HoldState::AwaitModifierRelease(modifier_keycode);
                }
            } else {
                self.win_shift_screenshot_held.remove(&keycode);
            }

            let keys = vec![SyntheticKey {
                keycode: VirtualKeyCode::Key4 as u16,
                key_down,
                preserve_flags: false,
                extra_flags: screenshot_shortcut_flags,
            }];

            if self.bracket_mode != BracketModeState::Inactive {
                self.bracket_mode = BracketModeState::Inactive;
                return EngineAction::EmitAndSystem(keys, Self::bracket_mode_action(false, None));
            }

            return EngineAction::Emit(keys);
        }

        if !key_down && self.win_shift_screenshot_held.remove(&keycode) {
            return EngineAction::Emit(vec![SyntheticKey {
                keycode: VirtualKeyCode::Key4 as u16,
                key_down: false,
                preserve_flags: false,
                extra_flags: screenshot_shortcut_flags,
            }]);
        }

        if flags & keycode::FLAG_CONTROL != 0
            && flags & keycode::FLAG_COMMAND == 0
            && self.ctrl_to_cmd_keys.contains(&keycode)
        {
            // Suppress autorepeat for remapped shortcuts (prevent repeated paste/save)
            if is_autorepeat {
                return EngineAction::Suppress;
            }
            let new_flags = (flags & !keycode::FLAG_CONTROL) | keycode::FLAG_COMMAND;
            if key_down {
                self.ctrl_to_cmd_held.insert(keycode);
                // If a hold modifier is pending, the user clearly intended a Ctrl shortcut,
                // not a layer action. Transition to AwaitModifierRelease to suppress the
                // orphaned modifier release.
                if let HoldState::HoldPending(modifier_keycode) = self.hold_state {
                    self.hold_state = HoldState::AwaitModifierRelease(modifier_keycode);
                }
            } else {
                self.ctrl_to_cmd_held.remove(&keycode);
            }
            let keys = vec![SyntheticKey {
                keycode,
                key_down,
                preserve_flags: false,
                extra_flags: new_flags,
            }];

            if self.bracket_mode != BracketModeState::Inactive {
                self.bracket_mode = BracketModeState::Inactive;
                return EngineAction::EmitAndSystem(keys, Self::bracket_mode_action(false, None));
            }

            return EngineAction::Emit(keys);
        }

        // Handle key-up for Ctrl→Cmd remapped keys when Ctrl was released first.
        // The key-up arrives without FLAG_CONTROL, so the check above won't catch it.
        // Preserve any other modifiers (Shift, Option) that may still be held.
        if !key_down && self.ctrl_to_cmd_held.remove(&keycode) {
            let new_flags = (flags & !keycode::FLAG_CONTROL) | keycode::FLAG_COMMAND;
            return EngineAction::Emit(vec![SyntheticKey {
                keycode,
                key_down: false,
                preserve_flags: false,
                extra_flags: new_flags,
            }]);
        }

        if self.bracket_mode != BracketModeState::Inactive {
            return self.process_bracket_mode_key(keycode, key_down, is_autorepeat);
        }

        // ── Mouse mode processing ──
        if self.mouse_mode != MouseModeState::Inactive {
            match self.process_mouse_mode_key(keycode, key_down, is_autorepeat, flags) {
                MouseModeResult::Handled(action) => return action,
                MouseModeResult::ExitAndReprocess => {
                    // Fall through to normal processing below
                }
            }
        }

        let is_option_key = matches!(
            VirtualKeyCode::from_raw(keycode),
            Some(VirtualKeyCode::Option | VirtualKeyCode::RightOption)
        );

        if self.alt_tab_shortcut_enabled && self.alt_tab_active {
            if !key_down && is_option_key && flags & keycode::FLAG_OPTION == 0 {
                self.alt_tab_active = false;
                return EngineAction::System(SystemAction::AltTabCommit);
            }

            if key_down && keycode == VirtualKeyCode::Escape as u16 && !is_autorepeat {
                self.alt_tab_active = false;
                return EngineAction::System(SystemAction::AltTabCancel);
            }

            if keycode == VirtualKeyCode::Tab as u16 {
                if key_down && !is_autorepeat {
                    return EngineAction::System(SystemAction::AltTabCycle {
                        reverse: flags & keycode::FLAG_SHIFT != 0,
                    });
                }
                return EngineAction::Suppress;
            }
        }

        if self.alt_tab_shortcut_enabled
            && key_down
            && keycode == VirtualKeyCode::Tab as u16
            && flags & keycode::FLAG_OPTION != 0
        {
            if is_autorepeat {
                return EngineAction::Suppress;
            }
            self.alt_tab_active = true;
            return EngineAction::System(SystemAction::AltTabCycle {
                reverse: flags & keycode::FLAG_SHIFT != 0,
            });
        }

        // ── Mouse mode entry: plain Tab↓ with no modifiers, idle state ──
        if self.mouse_mode == MouseModeState::Inactive
            && key_down
            && !is_autorepeat
            && keycode == VirtualKeyCode::Tab as u16
            && flags
                & (keycode::FLAG_SHIFT
                    | keycode::FLAG_CONTROL
                    | keycode::FLAG_OPTION
                    | keycode::FLAG_COMMAND)
                == 0
            && self.hold_state == HoldState::Idle
            && !self.alt_tab_active
            && self.bracket_mode == BracketModeState::Inactive
        {
            self.mouse_mode = MouseModeState::Pending;
            return EngineAction::Suppress;
        }

        let active_modifier = self.current_modifier_keycode();
        let is_mapped = active_modifier
            .and_then(|modifier_keycode| self.layer_mapping(modifier_keycode, keycode))
            .is_some();

        match (self.hold_state, keycode, key_down) {
            // ── Modifier↓ in Idle → enter hold-pending ──
            (HoldState::Idle, kc, true) if self.hold_layers.contains_key(&kc) => {
                self.hold_state = HoldState::HoldPending(kc);
                EngineAction::Suppress
            }

            // ── Modifier↓ repeat in HoldPending/Holding → suppress (prevent leak) ──
            (
                HoldState::HoldPending(modifier_keycode) | HoldState::Holding(modifier_keycode),
                kc,
                true,
            ) if kc == modifier_keycode => EngineAction::Suppress,

            // ── Modifier↑ in HoldPending (unused) → emit tap ──
            (HoldState::HoldPending(modifier_keycode), kc, false) if kc == modifier_keycode => {
                self.hold_state = HoldState::Idle;
                if modifier_keycode == VirtualKeyCode::Semicolon as u16 {
                    self.bracket_mode = BracketModeState::WaitingForPrefix;
                    EngineAction::System(Self::bracket_mode_action(true, None))
                } else {
                    EngineAction::Emit(self.emit_modifier_tap(modifier_keycode))
                }
            }

            // ── Modifier↑ in Holding → release all held keys, return to idle ──
            (HoldState::Holding(modifier_keycode), kc, false) if kc == modifier_keycode => {
                let releases = self.drain_held_keys();
                self.hold_state = HoldState::Idle;
                if releases.is_empty() {
                    EngineAction::Suppress
                } else {
                    EngineAction::Emit(releases)
                }
            }

            // ── Modifier↑ in AwaitModifierRelease → suppress orphaned release ──
            (HoldState::AwaitModifierRelease(modifier_keycode), kc, false)
                if kc == modifier_keycode =>
            {
                self.hold_state = HoldState::Idle;
                EngineAction::Suppress
            }

            // ── MappedKey↓ in HoldPending → confirm hold, emit target↓ ──
            (HoldState::HoldPending(modifier_keycode), kc, true) if is_mapped => {
                self.hold_state = HoldState::Holding(modifier_keycode);
                self.emit_mapping_down(modifier_keycode, kc)
            }

            // ── MappedKey↓ in Holding → emit target↓ (suppress autorepeat for sequences) ──
            (HoldState::Holding(modifier_keycode), kc, true) if is_mapped => {
                if is_autorepeat
                    && matches!(
                        self.layer_mapping(modifier_keycode, kc),
                        Some(KeyMapping::Sequence(_))
                    )
                {
                    return EngineAction::Suppress;
                }
                self.emit_mapping_down(modifier_keycode, kc)
            }

            // ── MappedKey↑ in Holding (was remapped) → emit target↑ or suppress ──
            (HoldState::Holding(_), kc, false) if self.held_keys.contains_key(&kc) => {
                match self.held_keys.remove(&kc).unwrap() {
                    HeldKeyInfo::Single {
                        target_keycode,
                        extra_flags,
                    } => EngineAction::Emit(vec![SyntheticKey {
                        keycode: target_keycode,
                        key_down: false,
                        preserve_flags: false,
                        extra_flags,
                    }]),
                    HeldKeyInfo::Sequence => EngineAction::Suppress,
                }
            }

            // ── NonMappedKey↓ in HoldPending → fallback: emit modifier tap + key ──
            (HoldState::HoldPending(modifier_keycode), kc, true) if kc != modifier_keycode => {
                self.hold_state = HoldState::AwaitModifierRelease(modifier_keycode);
                self.emit_fallback_tap_and_key(modifier_keycode, kc)
            }

            // ── Everything else → pass through ──
            _ => EngineAction::PassThrough,
        }
    }
}

impl Default for MappingEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space() -> u16 {
        VirtualKeyCode::Space as u16
    }
    fn j() -> u16 {
        VirtualKeyCode::J as u16
    }
    fn k() -> u16 {
        VirtualKeyCode::K as u16
    }
    fn i_key() -> u16 {
        VirtualKeyCode::I as u16
    }
    fn l() -> u16 {
        VirtualKeyCode::L as u16
    }
    fn d() -> u16 {
        VirtualKeyCode::D as u16
    } // unmapped key for fallback tests
    fn left() -> u16 {
        VirtualKeyCode::LeftArrow as u16
    }
    fn down() -> u16 {
        VirtualKeyCode::DownArrow as u16
    }
    fn up() -> u16 {
        VirtualKeyCode::UpArrow as u16
    }
    fn right() -> u16 {
        VirtualKeyCode::RightArrow as u16
    }
    fn home() -> u16 {
        VirtualKeyCode::Home as u16
    }
    fn end() -> u16 {
        VirtualKeyCode::End as u16
    }
    fn tab() -> u16 {
        VirtualKeyCode::Tab as u16
    }
    fn option_key() -> u16 {
        VirtualKeyCode::Option as u16
    }
    fn delete() -> u16 {
        VirtualKeyCode::Delete as u16
    }
    fn r_key() -> u16 {
        VirtualKeyCode::R as u16
    }
    fn ret() -> u16 {
        VirtualKeyCode::Return as u16
    }
    fn esc() -> u16 {
        VirtualKeyCode::Escape as u16
    }
    fn three_key() -> u16 {
        VirtualKeyCode::Key3 as u16
    }
    fn n() -> u16 {
        VirtualKeyCode::N as u16
    }
    fn m() -> u16 {
        VirtualKeyCode::M as u16
    }
    fn u_key() -> u16 {
        VirtualKeyCode::U as u16
    }
    fn o_key() -> u16 {
        VirtualKeyCode::O as u16
    }
    fn comma() -> u16 {
        0x2B
    }
    fn semicolon_key() -> u16 {
        0x29
    }
    fn minus_key() -> u16 {
        0x1B
    }
    fn equal_key() -> u16 {
        0x18
    }
    fn backslash_key() -> u16 {
        0x2A
    }
    fn slash_key() -> u16 {
        0x2C
    }
    fn period_key() -> u16 {
        0x2F
    }
    fn grave_key() -> u16 {
        0x32
    }
    fn b() -> u16 {
        VirtualKeyCode::B as u16
    }
    fn h() -> u16 {
        VirtualKeyCode::H as u16
    }
    fn digit_0() -> u16 {
        VirtualKeyCode::Key0 as u16
    }
    fn digit_1() -> u16 {
        VirtualKeyCode::Key1 as u16
    }
    fn digit_2() -> u16 {
        VirtualKeyCode::Key2 as u16
    }
    fn digit_4() -> u16 {
        VirtualKeyCode::Key4 as u16
    }
    fn digit_5() -> u16 {
        VirtualKeyCode::Key5 as u16
    }
    fn digit_6() -> u16 {
        VirtualKeyCode::Key6 as u16
    }
    fn digit_7() -> u16 {
        VirtualKeyCode::Key7 as u16
    }
    fn digit_8() -> u16 {
        VirtualKeyCode::Key8 as u16
    }
    fn digit_9() -> u16 {
        VirtualKeyCode::Key9 as u16
    }
    fn left_bracket_key() -> u16 {
        VirtualKeyCode::LeftBracket as u16
    }
    fn right_bracket_key() -> u16 {
        VirtualKeyCode::RightBracket as u16
    }

    fn assert_emit(action: &EngineAction, expected: &[(u16, bool)]) {
        match action {
            EngineAction::Emit(keys) => {
                assert_eq!(
                    keys.len(),
                    expected.len(),
                    "expected {} events, got {}: {:?}",
                    expected.len(),
                    keys.len(),
                    keys
                );
                for (i, (exp_kc, exp_down)) in expected.iter().enumerate() {
                    assert_eq!(keys[i].keycode, *exp_kc, "event {i} keycode mismatch");
                    assert_eq!(keys[i].key_down, *exp_down, "event {i} key_down mismatch");
                }
            }
            other => panic!("expected Emit, got {:?}", other),
        }
    }

    fn assert_emit_with_flags(action: &EngineAction, expected: &[(u16, bool, u64)]) {
        match action {
            EngineAction::Emit(keys) => {
                assert_eq!(
                    keys.len(),
                    expected.len(),
                    "expected {} events, got {}: {:?}",
                    expected.len(),
                    keys.len(),
                    keys
                );
                for (i, (exp_kc, exp_down, exp_flags)) in expected.iter().enumerate() {
                    assert_eq!(keys[i].keycode, *exp_kc, "event {i} keycode mismatch");
                    assert_eq!(keys[i].key_down, *exp_down, "event {i} key_down mismatch");
                    assert_eq!(
                        keys[i].extra_flags, *exp_flags,
                        "event {i} extra_flags mismatch"
                    );
                }
            }
            other => panic!("expected Emit, got {:?}", other),
        }
    }

    fn assert_system(action: &EngineAction, expected: SystemAction) {
        match action {
            EngineAction::System(system) => {
                assert_eq!(*system, expected);
            }
            other => panic!("expected System, got {:?}", other),
        }
    }

    fn assert_emit_and_system_with_flags(
        action: &EngineAction,
        expected: &[(u16, bool, u64)],
        expected_system: SystemAction,
    ) {
        match action {
            EngineAction::EmitAndSystem(keys, system) => {
                assert_eq!(*system, expected_system);
                assert_eq!(
                    keys.len(),
                    expected.len(),
                    "expected {} events, got {}: {:?}",
                    expected.len(),
                    keys.len(),
                    keys
                );
                for (i, (exp_kc, exp_down, exp_flags)) in expected.iter().enumerate() {
                    assert_eq!(keys[i].keycode, *exp_kc, "event {i} keycode mismatch");
                    assert_eq!(keys[i].key_down, *exp_down, "event {i} key_down mismatch");
                    assert_eq!(
                        keys[i].extra_flags, *exp_flags,
                        "event {i} extra_flags mismatch"
                    );
                }
            }
            other => panic!("expected EmitAndSystem, got {:?}", other),
        }
    }

    #[test]
    fn test_space_tap() {
        let mut engine = MappingEngine::new();

        let action = engine.process_key(space(), true, false, 0);
        assert!(matches!(action, EngineAction::Suppress));

        let action = engine.process_key(space(), false, false, 0);
        assert_emit(&action, &[(space(), true), (space(), false)]);
    }

    #[test]
    fn test_space_hold_navigation() {
        let mut engine = MappingEngine::new();

        // Space↓ → suppress
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // J↓ → Left↓
        let action = engine.process_key(j(), true, false, 0);
        assert_emit(&action, &[(left(), true)]);

        // J↑ → Left↑
        let action = engine.process_key(j(), false, false, 0);
        assert_emit(&action, &[(left(), false)]);

        // Space↑ → suppress (was used as modifier, no keys left held)
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_space_hold_unmapped_key_then_release() {
        let mut engine = MappingEngine::new();

        engine.process_key(space(), true, false, 0);

        // D↓ (unmapped) → emit space tap + D (fallback)
        let action = engine.process_key(d(), true, false, 0);
        assert_emit(&action, &[(space(), true), (space(), false), (d(), true)]);

        // Now in AwaitSpaceRelease: Space↑ should be suppressed
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_disabled() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(false);

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::PassThrough
        ));
    }

    // ── Bug fix tests ──

    #[test]
    fn test_stuck_keys_space_released_before_nav() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);

        // Space↑ while J is still held → must emit Left↑
        let action = engine.process_key(space(), false, false, 0);
        assert_emit(&action, &[(left(), false)]);

        // J↑ in Idle → should pass through as plain J (it's no longer being remapped)
        assert!(matches!(
            engine.process_key(j(), false, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_multiple_nav_keys_released_on_space_up() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);
        assert_emit(&engine.process_key(k(), true, false, 0), &[(down(), true)]);

        // Space↑ → must emit Left↑ and Down↑ for both held keys
        let action = engine.process_key(space(), false, false, 0);
        match &action {
            EngineAction::Emit(keys) => {
                assert_eq!(keys.len(), 2);
                let keycodes: Vec<u16> = keys.iter().map(|k| k.keycode).collect();
                assert!(keycodes.contains(&left()), "should release Left arrow");
                assert!(keycodes.contains(&down()), "should release Down arrow");
                assert!(keys.iter().all(|k| !k.key_down), "all should be key-up");
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_nav_key_up_without_prior_down_passes_through() {
        let mut engine = MappingEngine::new();

        // J↓ in Idle → pass through
        assert!(matches!(
            engine.process_key(j(), true, false, 0),
            EngineAction::PassThrough
        ));

        // Space↓ → HoldPending
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // J↑ in HoldPending → J is mapped but key-up, not in held_keys → pass through
        assert!(matches!(
            engine.process_key(j(), false, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_set_enabled_false_releases_held_keys() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);
        assert_emit(&engine.process_key(k(), true, false, 0), &[(down(), true)]);

        // Disable while holding → should return releases for held arrows
        let releases = engine.set_enabled(false);
        assert_eq!(releases.len(), 2);
        let keycodes: Vec<u16> = releases.iter().map(|k| k.keycode).collect();
        assert!(keycodes.contains(&left()));
        assert!(keycodes.contains(&down()));
        assert!(releases.iter().all(|k| !k.key_down));
    }

    #[test]
    fn test_all_four_directions() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // J→Left, K→Down, I→Up, L→Right
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);
        assert_emit(
            &engine.process_key(j(), false, false, 0),
            &[(left(), false)],
        );
        assert_emit(&engine.process_key(k(), true, false, 0), &[(down(), true)]);
        assert_emit(
            &engine.process_key(k(), false, false, 0),
            &[(down(), false)],
        );
        assert_emit(
            &engine.process_key(i_key(), true, false, 0),
            &[(up(), true)],
        );
        assert_emit(
            &engine.process_key(i_key(), false, false, 0),
            &[(up(), false)],
        );
        assert_emit(&engine.process_key(l(), true, false, 0), &[(right(), true)]);
        assert_emit(
            &engine.process_key(l(), false, false, 0),
            &[(right(), false)],
        );

        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_rapid_space_tap_tap() {
        let mut engine = MappingEngine::new();

        engine.process_key(space(), true, false, 0);
        let action = engine.process_key(space(), false, false, 0);
        assert_emit(&action, &[(space(), true), (space(), false)]);

        // Second tap
        engine.process_key(space(), true, false, 0);
        let action = engine.process_key(space(), false, false, 0);
        assert_emit(&action, &[(space(), true), (space(), false)]);
    }

    #[test]
    fn test_await_space_release_other_keys_pass_through() {
        let mut engine = MappingEngine::new();

        engine.process_key(space(), true, false, 0);
        engine.process_key(d(), true, false, 0); // D is unmapped → AwaitSpaceRelease

        // D↑ should pass through
        assert!(matches!(
            engine.process_key(d(), false, false, 0),
            EngineAction::PassThrough
        ));

        // Space↑ should be suppressed (orphaned release)
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_nav_key_up_in_holding_not_tracked() {
        let mut engine = MappingEngine::new();

        // J↓ in Idle
        assert!(matches!(
            engine.process_key(j(), true, false, 0),
            EngineAction::PassThrough
        ));

        // Space↓ → HoldPending
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // K↓ → transitions to Holding, emits Down↓
        assert_emit(&engine.process_key(k(), true, false, 0), &[(down(), true)]);

        // J↑ → J was not remapped (pressed before Space), pass through
        assert!(matches!(
            engine.process_key(j(), false, false, 0),
            EngineAction::PassThrough
        ));

        // K↑ → K was remapped, emit Down↑
        assert_emit(
            &engine.process_key(k(), false, false, 0),
            &[(down(), false)],
        );
    }

    #[test]
    fn test_space_repeat_suppressed_in_hold_pending() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        let action = engine.process_key(space(), false, false, 0);
        assert_emit(&action, &[(space(), true), (space(), false)]);
    }

    #[test]
    fn test_space_repeat_suppressed_in_holding() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        assert_emit(
            &engine.process_key(j(), false, false, 0),
            &[(left(), false)],
        );
    }

    #[test]
    fn test_fallback_space_no_preserve_flags() {
        let mut engine = MappingEngine::new();

        engine.process_key(space(), true, false, 0);
        let action = engine.process_key(d(), true, false, 0); // D is unmapped

        match &action {
            EngineAction::Emit(keys) => {
                assert!(
                    !keys[0].preserve_flags,
                    "fallback space-down must not preserve flags"
                );
                assert!(
                    !keys[1].preserve_flags,
                    "fallback space-up must not preserve flags"
                );
                assert!(keys[2].preserve_flags, "fallback key should preserve flags");
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_arrow_keys_preserve_flags() {
        let mut engine = MappingEngine::new();

        engine.process_key(space(), true, false, 0);
        let action = engine.process_key(j(), true, false, 0);

        match &action {
            EngineAction::Emit(keys) => {
                assert!(keys[0].preserve_flags, "arrow key should preserve flags");
                assert_eq!(
                    keys[0].extra_flags, 0,
                    "plain arrow should have no extra flags"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    // ── New mapping tests ──

    #[test]
    fn test_space_a_emits_return() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(
            &engine.process_key(VirtualKeyCode::A as u16, true, false, 0),
            &[(ret(), true)],
        );
        assert_emit(
            &engine.process_key(VirtualKeyCode::A as u16, false, false, 0),
            &[(ret(), false)],
        );
    }

    #[test]
    fn test_space_s_emits_escape() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(
            &engine.process_key(VirtualKeyCode::S as u16, true, false, 0),
            &[(esc(), true)],
        );
    }

    #[test]
    fn test_space_e_emits_backspace() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(
            &engine.process_key(VirtualKeyCode::E as u16, true, false, 0),
            &[(delete(), true)],
        );
    }

    #[test]
    fn test_space_q_emits_four_spaces() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::Q as u16, true, false, 0);
        // 4 spaces: each as down+up = 8 events
        assert_emit(
            &action,
            &[
                (space(), true),
                (space(), false),
                (space(), true),
                (space(), false),
                (space(), true),
                (space(), false),
                (space(), true),
                (space(), false),
            ],
        );
        // Q↑ should be suppressed (sequence, no release needed)
        assert!(matches!(
            engine.process_key(VirtualKeyCode::Q as u16, false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_space_u_emits_option_left() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::U as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(left(), true, keycode::FLAG_OPTION)]);
    }

    #[test]
    fn test_space_o_emits_option_right() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::O as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(right(), true, keycode::FLAG_OPTION)]);
    }

    #[test]
    fn test_space_h_emits_cmd_left() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::H as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(left(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_space_n_emits_cmd_right() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::N as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(right(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_space_r_emits_option_delete() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::R as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(delete(), true, keycode::FLAG_OPTION)]);
    }

    #[test]
    fn test_space_w_emits_delete_line_sequence() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::W as u16, true, false, 0);
        // Delete line: Cmd+Left(down,up), Shift+Cmd+Right(down,up), Backspace(down,up) = 6 events
        assert_emit_with_flags(
            &action,
            &[
                (left(), true, keycode::FLAG_COMMAND),
                (left(), false, keycode::FLAG_COMMAND),
                (right(), true, keycode::FLAG_SHIFT | keycode::FLAG_COMMAND),
                (right(), false, keycode::FLAG_SHIFT | keycode::FLAG_COMMAND),
                (delete(), true, 0),
                (delete(), false, 0),
            ],
        );
        // W↑ should be suppressed
        assert!(matches!(
            engine.process_key(VirtualKeyCode::W as u16, false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_space_z_emits_ctrl_shift_tab() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::Z as u16, true, false, 0);
        assert_emit_with_flags(
            &action,
            &[(tab(), true, keycode::FLAG_CONTROL | keycode::FLAG_SHIFT)],
        );
    }

    #[test]
    fn test_space_v_emits_ctrl_tab() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        let action = engine.process_key(VirtualKeyCode::V as u16, true, false, 0);
        assert_emit_with_flags(&action, &[(tab(), true, keycode::FLAG_CONTROL)]);
    }

    #[test]
    fn windows_space_layer_uses_native_navigation_targets() {
        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::H as u16, true, false, 0),
            &[(home(), true, 0)],
        );
        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::H as u16, false, false, 0),
            &[(home(), false, 0)],
        );

        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::U as u16, true, false, 0),
            &[(left(), true, keycode::FLAG_CONTROL)],
        );
        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::U as u16, false, false, 0),
            &[(left(), false, keycode::FLAG_CONTROL)],
        );

        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::N as u16, true, false, 0),
            &[(end(), true, 0)],
        );
    }

    #[test]
    fn windows_delete_word_and_line_use_ctrl_and_home_end() {
        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
        engine.process_key(space(), true, false, 0);

        assert_emit_with_flags(
            &engine.process_key(VirtualKeyCode::R as u16, true, false, 0),
            &[(delete(), true, keycode::FLAG_CONTROL)],
        );
        assert!(matches!(
            engine.process_key(VirtualKeyCode::R as u16, false, false, 0),
            EngineAction::Emit(_)
        ));

        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
        engine.process_key(space(), true, false, 0);
        let action = engine.process_key(VirtualKeyCode::W as u16, true, false, 0);
        assert_emit_with_flags(
            &action,
            &[
                (home(), true, 0),
                (home(), false, 0),
                (end(), true, keycode::FLAG_SHIFT),
                (end(), false, keycode::FLAG_SHIFT),
                (delete(), true, 0),
                (delete(), false, 0),
            ],
        );
    }

    #[test]
    fn windows_platform_does_not_take_over_native_system_shortcuts() {
        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);

        assert!(matches!(
            engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL),
            EngineAction::PassThrough
        ));
        assert!(matches!(
            engine.process_key(
                s_key(),
                true,
                false,
                keycode::FLAG_COMMAND | keycode::FLAG_SHIFT
            ),
            EngineAction::PassThrough
        ));
        assert!(matches!(
            engine.process_key(tab(), true, false, keycode::FLAG_OPTION),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_option_tab_triggers_custom_window_switch_cycle() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);
        assert!(matches!(
            action,
            EngineAction::System(SystemAction::AltTabCycle { reverse: false })
        ));
    }

    #[test]
    fn test_shift_option_tab_cycles_backward() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(
            tab(),
            true,
            false,
            keycode::FLAG_OPTION | keycode::FLAG_SHIFT,
        );
        assert!(matches!(
            action,
            EngineAction::System(SystemAction::AltTabCycle { reverse: true })
        ));
    }

    #[test]
    fn test_option_tab_key_up_is_suppressed_while_switcher_is_active() {
        let mut engine = MappingEngine::new();
        let _ = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);
        assert!(matches!(
            engine.process_key(tab(), false, false, keycode::FLAG_OPTION),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_option_release_commits_active_window_switcher() {
        let mut engine = MappingEngine::new();
        let _ = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);
        let action = engine.process_key(option_key(), false, false, 0);
        assert!(matches!(
            action,
            EngineAction::System(SystemAction::AltTabCommit)
        ));
    }

    #[test]
    fn test_escape_cancels_active_window_switcher() {
        let mut engine = MappingEngine::new();
        let _ = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);
        let action = engine.process_key(esc(), true, false, 0);
        assert!(matches!(
            action,
            EngineAction::System(SystemAction::AltTabCancel)
        ));
        assert!(matches!(
            engine.process_key(option_key(), false, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_cancel_alt_tab_session_prevents_late_option_release_commit() {
        let mut engine = MappingEngine::new();
        let _ = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);

        engine.cancel_alt_tab_session();

        assert!(matches!(
            engine.process_key(option_key(), false, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_modifier_mapping_preserves_flags_for_nav() {
        // Navigation mappings (H, N, U, O) should preserve_flags for Shift+selection
        let mut engine = MappingEngine::new();
        engine.process_key(space(), true, false, 0);

        let action = engine.process_key(VirtualKeyCode::U as u16, true, false, 0);
        match &action {
            EngineAction::Emit(keys) => {
                assert!(
                    keys[0].preserve_flags,
                    "word-left should preserve flags for Shift+selection"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_action_mapping_does_not_preserve_flags() {
        // Action mappings (A, S, E, Q, W, R, Z, V) should NOT preserve_flags
        let mut engine = MappingEngine::new();
        engine.process_key(space(), true, false, 0);

        let action = engine.process_key(VirtualKeyCode::A as u16, true, false, 0);
        match &action {
            EngineAction::Emit(keys) => {
                assert!(
                    !keys[0].preserve_flags,
                    "action key should not preserve flags"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_three_tap_preserves_flags() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(three_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        let action = engine.process_key(three_key(), false, false, keycode::FLAG_SHIFT);
        assert_emit(&action, &[(three_key(), true), (three_key(), false)]);

        match &action {
            EngineAction::Emit(keys) => {
                assert!(
                    keys.iter().all(|key| key.preserve_flags),
                    "hold-3 tap should preserve modifiers such as Shift"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_three_hold_maps_requested_number_keys() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(three_key(), true, false, 0),
            EngineAction::Suppress
        ));

        let cases = [
            (n(), digit_1()),
            (m(), digit_2()),
            (comma(), three_key()),
            (j(), digit_4()),
            (k(), digit_5()),
            (l(), digit_6()),
            (VirtualKeyCode::U as u16, digit_7()),
            (i_key(), digit_8()),
            (VirtualKeyCode::O as u16, digit_9()),
            (b(), digit_0()),
            (h(), digit_0()),
        ];

        for (source_key, target_key) in cases {
            let action = engine.process_key(source_key, true, false, 0);
            assert_emit(&action, &[(target_key, true)]);

            let action = engine.process_key(source_key, false, false, 0);
            assert_emit(&action, &[(target_key, false)]);
        }

        assert!(matches!(
            engine.process_key(three_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_three_layer_digit_mapping_does_not_preserve_shift() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(three_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        let action = engine.process_key(n(), true, false, keycode::FLAG_SHIFT);
        match &action {
            EngineAction::Emit(keys) => {
                assert_eq!(keys[0].keycode, digit_1());
                assert!(
                    !keys[0].preserve_flags,
                    "digit layer should emit raw digits, not shifted punctuation"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_three_hold_unmapped_key_replays_shifted_three_tap() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(three_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        let action = engine.process_key(d(), true, false, keycode::FLAG_SHIFT);
        match &action {
            EngineAction::Emit(keys) => {
                assert_eq!(keys.len(), 3);
                assert_eq!(keys[0].keycode, three_key());
                assert!(keys[0].key_down);
                assert!(keys[0].preserve_flags);
                assert_eq!(keys[1].keycode, three_key());
                assert!(!keys[1].key_down);
                assert!(keys[1].preserve_flags);
                assert_eq!(keys[2].keycode, d());
                assert!(keys[2].key_down);
                assert!(keys[2].preserve_flags);
            }
            _ => panic!("expected Emit"),
        }

        assert!(matches!(
            engine.process_key(three_key(), false, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_three_released_before_comma_releases_synthetic_three() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(three_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(
            &engine.process_key(comma(), true, false, 0),
            &[(three_key(), true)],
        );

        let action = engine.process_key(three_key(), false, false, 0);
        assert_emit(&action, &[(three_key(), false)]);

        assert!(matches!(
            engine.process_key(comma(), false, false, 0),
            EngineAction::PassThrough
        ));
    }

    #[test]
    fn test_semicolon_tap_enters_bracket_mode() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));

        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );
    }

    #[test]
    fn test_semicolon_shift_tap_also_enters_bracket_mode() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        assert_system(
            &engine.process_key(semicolon_key(), false, false, keycode::FLAG_SHIFT),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );
    }

    #[test]
    fn test_semicolon_hold_maps_requested_symbol_keys() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));

        let cases = [
            (VirtualKeyCode::A as u16, minus_key(), 0),
            (b(), digit_5(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::C as u16, period_key(), 0),
            (d(), equal_key(), 0),
            (VirtualKeyCode::E as u16, digit_6(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::F as u16, period_key(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::G as u16, digit_1(), keycode::FLAG_SHIFT),
            (h(), equal_key(), keycode::FLAG_SHIFT),
            (i_key(), semicolon_key(), keycode::FLAG_SHIFT),
            (j(), semicolon_key(), 0),
            (k(), grave_key(), 0),
            (n(), slash_key(), 0),
            (VirtualKeyCode::Q as u16, minus_key(), 0),
            (VirtualKeyCode::R as u16, digit_7(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::S as u16, comma(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::T as u16, grave_key(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::U as u16, digit_4(), keycode::FLAG_SHIFT),
            (
                VirtualKeyCode::V as u16,
                backslash_key(),
                keycode::FLAG_SHIFT,
            ),
            (VirtualKeyCode::W as u16, three_key(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::X as u16, minus_key(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::Y as u16, digit_2(), keycode::FLAG_SHIFT),
            (VirtualKeyCode::Z as u16, backslash_key(), 0),
        ];

        for (source_key, target_key, extra_flags) in cases {
            let action = engine.process_key(source_key, true, false, 0);
            assert_emit_with_flags(&action, &[(target_key, true, extra_flags)]);

            let action = engine.process_key(source_key, false, false, 0);
            assert_emit_with_flags(&action, &[(target_key, false, extra_flags)]);
        }

        assert!(matches!(
            engine.process_key(semicolon_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_semicolon_layer_mapping_does_not_preserve_shift() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        let action = engine.process_key(VirtualKeyCode::A as u16, true, false, keycode::FLAG_SHIFT);
        match &action {
            EngineAction::Emit(keys) => {
                assert_eq!(keys[0].keycode, minus_key());
                assert_eq!(keys[0].extra_flags, 0);
                assert!(
                    !keys[0].preserve_flags,
                    "symbol layer should emit its requested symbol, not a shifted variant"
                );
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_semicolon_hold_unmapped_key_replays_shifted_semicolon_tap() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, keycode::FLAG_SHIFT),
            EngineAction::Suppress
        ));

        let action = engine.process_key(VirtualKeyCode::P as u16, true, false, keycode::FLAG_SHIFT);
        match &action {
            EngineAction::Emit(keys) => {
                assert_eq!(keys.len(), 3);
                assert_eq!(keys[0].keycode, semicolon_key());
                assert!(keys[0].key_down);
                assert!(keys[0].preserve_flags);
                assert_eq!(keys[1].keycode, semicolon_key());
                assert!(!keys[1].key_down);
                assert!(keys[1].preserve_flags);
                assert_eq!(keys[2].keycode, VirtualKeyCode::P as u16);
                assert!(keys[2].key_down);
                assert!(keys[2].preserve_flags);
            }
            _ => panic!("expected Emit"),
        }
    }

    #[test]
    fn test_bracket_mode_xk_emits_parentheses_and_hides_overlay() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );

        assert_system(
            &engine.process_key(VirtualKeyCode::X as u16, true, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: Some(BracketPair::Round),
            },
        );
        assert!(matches!(
            engine.process_key(VirtualKeyCode::X as u16, false, false, 0),
            EngineAction::Suppress
        ));

        assert_emit_and_system_with_flags(
            &engine.process_key(k(), true, false, 0),
            &[
                (digit_9(), true, keycode::FLAG_SHIFT),
                (digit_9(), false, keycode::FLAG_SHIFT),
                (digit_0(), true, keycode::FLAG_SHIFT),
                (digit_0(), false, keycode::FLAG_SHIFT),
            ],
            SystemAction::BracketModeChanged {
                visible: false,
                pending_pair: None,
            },
        );
        assert!(matches!(
            engine.process_key(k(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_bracket_mode_zk_emits_square_brackets_and_hides_overlay() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );

        assert_system(
            &engine.process_key(VirtualKeyCode::Z as u16, true, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: Some(BracketPair::Square),
            },
        );
        assert!(matches!(
            engine.process_key(VirtualKeyCode::Z as u16, false, false, 0),
            EngineAction::Suppress
        ));

        assert_emit_and_system_with_flags(
            &engine.process_key(k(), true, false, 0),
            &[
                (left_bracket_key(), true, 0),
                (left_bracket_key(), false, 0),
                (right_bracket_key(), true, 0),
                (right_bracket_key(), false, 0),
            ],
            SystemAction::BracketModeChanged {
                visible: false,
                pending_pair: None,
            },
        );
        assert!(matches!(
            engine.process_key(k(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_bracket_mode_dk_emits_curly_brackets_and_hides_overlay() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );

        assert_system(
            &engine.process_key(d(), true, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: Some(BracketPair::Curly),
            },
        );
        assert!(matches!(
            engine.process_key(d(), false, false, 0),
            EngineAction::Suppress
        ));

        assert_emit_and_system_with_flags(
            &engine.process_key(k(), true, false, 0),
            &[
                (left_bracket_key(), true, keycode::FLAG_SHIFT),
                (left_bracket_key(), false, keycode::FLAG_SHIFT),
                (right_bracket_key(), true, keycode::FLAG_SHIFT),
                (right_bracket_key(), false, keycode::FLAG_SHIFT),
            ],
            SystemAction::BracketModeChanged {
                visible: false,
                pending_pair: None,
            },
        );
        assert!(matches!(
            engine.process_key(k(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_bracket_mode_invalid_first_key_replays_input_and_hides_overlay() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );

        assert_emit_and_system_with_flags(
            &engine.process_key(VirtualKeyCode::A as u16, true, false, 0),
            &[(VirtualKeyCode::A as u16, true, 0)],
            SystemAction::BracketModeChanged {
                visible: false,
                pending_pair: None,
            },
        );
        assert_emit(
            &engine.process_key(VirtualKeyCode::A as u16, false, false, 0),
            &[(VirtualKeyCode::A as u16, false)],
        );
    }

    #[test]
    fn test_bracket_mode_invalid_second_key_replays_buffered_prefix_then_input() {
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(semicolon_key(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_system(
            &engine.process_key(semicolon_key(), false, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: None,
            },
        );

        assert_system(
            &engine.process_key(VirtualKeyCode::X as u16, true, false, 0),
            SystemAction::BracketModeChanged {
                visible: true,
                pending_pair: Some(BracketPair::Round),
            },
        );
        assert!(matches!(
            engine.process_key(VirtualKeyCode::X as u16, false, false, 0),
            EngineAction::Suppress
        ));

        assert_emit_and_system_with_flags(
            &engine.process_key(VirtualKeyCode::A as u16, true, false, 0),
            &[
                (VirtualKeyCode::X as u16, true, 0),
                (VirtualKeyCode::X as u16, false, 0),
                (VirtualKeyCode::A as u16, true, 0),
            ],
            SystemAction::BracketModeChanged {
                visible: false,
                pending_pair: None,
            },
        );
        assert_emit(
            &engine.process_key(VirtualKeyCode::A as u16, false, false, 0),
            &[(VirtualKeyCode::A as u16, false)],
        );
    }

    #[test]
    fn test_release_includes_extra_flags() {
        // Key-up for modifier mappings should include the same extra_flags
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // U↓ → Option+Left↓
        engine.process_key(VirtualKeyCode::U as u16, true, false, 0);

        // U↑ → Left↑ with Option flag
        let action = engine.process_key(VirtualKeyCode::U as u16, false, false, 0);
        assert_emit_with_flags(&action, &[(left(), false, keycode::FLAG_OPTION)]);
    }

    #[test]
    fn test_drain_held_keys_includes_extra_flags() {
        // When Space is released with modifier-mapped keys held, releases should include flags
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // Hold U (Option+Left)
        engine.process_key(VirtualKeyCode::U as u16, true, false, 0);

        // Space↑ → should drain with Option flag
        let action = engine.process_key(space(), false, false, 0);
        assert_emit_with_flags(&action, &[(left(), false, keycode::FLAG_OPTION)]);
    }

    #[test]
    fn test_sequence_autorepeat_suppressed() {
        let mut engine = MappingEngine::new();
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // First W↓ fires the delete-line sequence
        let action = engine.process_key(VirtualKeyCode::W as u16, true, false, 0);
        assert!(matches!(action, EngineAction::Emit(_)));

        // Autorepeat W↓ should be suppressed
        assert!(matches!(
            engine.process_key(VirtualKeyCode::W as u16, true, true, 0),
            EngineAction::Suppress
        ));

        // But autorepeat on Single mappings (e.g. J→Left) should still work
        let action = engine.process_key(VirtualKeyCode::J as u16, true, true, 0);
        assert_emit(&action, &[(left(), true)]);
    }

    // ── Ctrl→Cmd remapping tests ──

    fn c_key() -> u16 {
        VirtualKeyCode::C as u16
    }
    fn v_key() -> u16 {
        VirtualKeyCode::V as u16
    }
    fn x_key() -> u16 {
        VirtualKeyCode::X as u16
    }
    fn s_key() -> u16 {
        VirtualKeyCode::S as u16
    }
    fn z_key() -> u16 {
        VirtualKeyCode::Z as u16
    }
    fn a_key() -> u16 {
        VirtualKeyCode::A as u16
    }

    #[test]
    fn test_cmd_shift_s_remapped_to_cmd_shift_4() {
        let mut engine = MappingEngine::new();
        let flags = keycode::FLAG_COMMAND | keycode::FLAG_SHIFT;

        let action = engine.process_key(s_key(), true, false, flags);
        assert_emit_with_flags(&action, &[(digit_4(), true, flags)]);

        let action = engine.process_key(s_key(), false, false, flags);
        assert_emit_with_flags(&action, &[(digit_4(), false, flags)]);
    }

    #[test]
    fn test_ctrl_c_remapped_to_cmd_c() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(c_key(), true, keycode::FLAG_COMMAND)]);

        // Key-up also remapped
        let action = engine.process_key(c_key(), false, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(c_key(), false, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_v_remapped_to_cmd_v() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(v_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(v_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_x_remapped_to_cmd_x() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(x_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(x_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_s_remapped_to_cmd_s() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(s_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(s_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_z_remapped_to_cmd_z() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(z_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(z_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_a_remapped_to_cmd_a() {
        let mut engine = MappingEngine::new();
        let action = engine.process_key(a_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(a_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_shift_c_remapped_preserves_shift() {
        let mut engine = MappingEngine::new();
        let flags = keycode::FLAG_CONTROL | keycode::FLAG_SHIFT;
        let action = engine.process_key(c_key(), true, false, flags);
        let expected_flags = keycode::FLAG_COMMAND | keycode::FLAG_SHIFT;
        assert_emit_with_flags(&action, &[(c_key(), true, expected_flags)]);
    }

    #[test]
    fn test_ctrl_cmd_c_not_remapped() {
        // Ctrl+Cmd+C should NOT be remapped (preserve original Ctrl+Cmd combo)
        let mut engine = MappingEngine::new();
        let flags = keycode::FLAG_CONTROL | keycode::FLAG_COMMAND;
        let action = engine.process_key(c_key(), true, false, flags);
        assert!(matches!(action, EngineAction::PassThrough));
    }

    #[test]
    fn test_ctrl_remap_with_space_hold_pending() {
        // If Space is held (HoldPending) and Ctrl+C is pressed,
        // the Ctrl+C should be remapped and Space should NOT be emitted.
        let mut engine = MappingEngine::new();

        // Space↓ → HoldPending
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));

        // Ctrl+C → Cmd+C, transitions to AwaitSpaceRelease
        let action = engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(c_key(), true, keycode::FLAG_COMMAND)]);

        // Space↑ should be suppressed (orphaned release)
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_no_flags_c_passes_to_space_machine() {
        // C without Control flag in Idle → PassThrough (C is not a Space mapping)
        let mut engine = MappingEngine::new();
        let action = engine.process_key(c_key(), true, false, 0);
        assert!(matches!(action, EngineAction::PassThrough));
    }

    #[test]
    fn test_ctrl_d_not_remapped() {
        // D is not in ctrl_to_cmd_keys, so Ctrl+D should pass through
        let mut engine = MappingEngine::new();
        let action = engine.process_key(d(), true, false, keycode::FLAG_CONTROL);
        assert!(matches!(action, EngineAction::PassThrough));
    }

    #[test]
    fn test_ctrl_released_before_key_up() {
        // Ctrl is released before C key-up → key-up should still be remapped
        let mut engine = MappingEngine::new();

        // Ctrl+C↓ → Cmd+C↓
        let action = engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(c_key(), true, keycode::FLAG_COMMAND)]);

        // C↑ without Ctrl flag (Ctrl was released first) → still emit Cmd+C↑
        let action = engine.process_key(c_key(), false, false, 0);
        assert_emit_with_flags(&action, &[(c_key(), false, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_ctrl_remap_autorepeat_suppressed() {
        // Autorepeat for remapped shortcuts should be suppressed
        let mut engine = MappingEngine::new();

        // Ctrl+V↓ → Cmd+V↓
        let action = engine.process_key(v_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(v_key(), true, keycode::FLAG_COMMAND)]);

        // Ctrl+V autorepeat → suppressed
        assert!(matches!(
            engine.process_key(v_key(), true, true, keycode::FLAG_CONTROL),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_space_holding_then_ctrl_c() {
        // In Holding state (Space used as modifier), Ctrl+C should still remap
        let mut engine = MappingEngine::new();

        // Space↓ → HoldPending
        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        // J↓ → transitions to Holding
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);

        // Ctrl+C in Holding → Cmd+C (Ctrl remap wins)
        let action = engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(c_key(), true, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_space_holding_ctrl_released_before_key_up() {
        // In Holding state, Ctrl released before C↑ → must still emit Cmd+C↑
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);

        // Ctrl+C↓ → Cmd+C↓
        engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        // Ctrl released (FlagsChanged, ignored by interceptor), then C↑ with flags=0
        let action = engine.process_key(c_key(), false, false, 0);
        assert_emit_with_flags(&action, &[(c_key(), false, keycode::FLAG_COMMAND)]);
    }

    #[test]
    fn test_space_hold_pending_ctrl_released_before_key_up() {
        // Space HoldPending + Ctrl+C, Ctrl released before C↑ → no stray character
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        // Ctrl+C↓ → AwaitSpaceRelease + Cmd+C↓
        engine.process_key(c_key(), true, false, keycode::FLAG_CONTROL);
        // C↑ without Ctrl → must still remap, not pass through as stray 'c'
        let action = engine.process_key(c_key(), false, false, 0);
        assert_emit_with_flags(&action, &[(c_key(), false, keycode::FLAG_COMMAND)]);
        // Space↑ suppressed
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_ctrl_shift_released_before_key_up_preserves_shift() {
        // Ctrl+Shift+Z↓ → Cmd+Shift+Z↓; then Ctrl released, Z↑ with Shift only
        let mut engine = MappingEngine::new();
        let flags_down = keycode::FLAG_CONTROL | keycode::FLAG_SHIFT;
        let action = engine.process_key(z_key(), true, false, flags_down);
        assert_emit_with_flags(
            &action,
            &[(z_key(), true, keycode::FLAG_COMMAND | keycode::FLAG_SHIFT)],
        );

        // Ctrl released first, Z↑ arrives with only Shift flag
        let action = engine.process_key(z_key(), false, false, keycode::FLAG_SHIFT);
        assert_emit_with_flags(
            &action,
            &[(z_key(), false, keycode::FLAG_COMMAND | keycode::FLAG_SHIFT)],
        );
    }

    #[test]
    fn test_space_hold_pending_ctrl_v_no_stray_space() {
        // Space↓, Ctrl+V↓, Ctrl↑, V↑, Space↑ → no stray Space tap, no raw V↑
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        // Ctrl+V↓ → AwaitSpaceRelease + Cmd+V
        let action = engine.process_key(v_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(v_key(), true, keycode::FLAG_COMMAND)]);
        // Ctrl released, V↑ with no flags → still remapped
        let action = engine.process_key(v_key(), false, false, 0);
        assert_emit_with_flags(&action, &[(v_key(), false, keycode::FLAG_COMMAND)]);
        // Space↑ suppressed (orphaned)
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn test_space_holding_nav_then_ctrl_a() {
        // Space↓, J↓ (Holding), Ctrl+A↓/↑, J↑, Space↑ → nav keys still release properly
        let mut engine = MappingEngine::new();

        assert!(matches!(
            engine.process_key(space(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_emit(&engine.process_key(j(), true, false, 0), &[(left(), true)]);

        // Ctrl+A↓ in Holding → Cmd+A
        let action = engine.process_key(a_key(), true, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(a_key(), true, keycode::FLAG_COMMAND)]);
        // Ctrl+A↑
        let action = engine.process_key(a_key(), false, false, keycode::FLAG_CONTROL);
        assert_emit_with_flags(&action, &[(a_key(), false, keycode::FLAG_COMMAND)]);

        // J↑ → Left↑ (nav still tracked)
        assert_emit(
            &engine.process_key(j(), false, false, 0),
            &[(left(), false)],
        );
        // Space↑ → idle (no more held keys)
        assert!(matches!(
            engine.process_key(space(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    // ── Mouse mode tests ──

    #[test]
    fn mouse_mode_tab_then_j_enters_active() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Tab↓ → Pending (suppress)
        assert!(matches!(
            engine.process_key(tab(), true, false, 0),
            EngineAction::Suppress
        ));
        assert_eq!(engine.mouse_mode, MouseModeState::Pending);

        // J↓ → Active, emit MouseMove Left (fast because Tab is held)
        assert_system(
            &engine.process_key(j(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: true,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);
    }

    #[test]
    fn mouse_mode_tab_then_r_enters_active_and_triggers_mouse_back() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_system(
            &engine.process_key(r_key(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Back,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert!(matches!(
            engine.process_key(r_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_tab_then_n_clicks_left_without_leaking_tab() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_system(
            &engine.process_key(n(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Left,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        assert!(matches!(
            engine.process_key(n(), false, false, 0),
            EngineAction::Suppress
        ));
        assert!(matches!(
            engine.process_key(tab(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_tab_then_u_enters_active_and_scrolls() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_system(
            &engine.process_key(u_key(), true, false, 0),
            SystemAction::MouseScroll {
                direction: MouseScrollDirection::Up,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert_system(
            &engine.process_key(u_key(), true, true, 0),
            SystemAction::MouseScroll {
                direction: MouseScrollDirection::Up,
            },
        );
        assert!(matches!(
            engine.process_key(u_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_tab_then_m_enters_active_and_clicks_right() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_system(
            &engine.process_key(m(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Right,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert!(matches!(
            engine.process_key(m(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn windows_mouse_mode_tab_then_c_uses_ctrl_b_without_prior_move() {
        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_emit_with_flags(
            &engine.process_key(c_key(), true, false, 0),
            &[
                (b(), true, keycode::FLAG_CONTROL),
                (b(), false, keycode::FLAG_CONTROL),
            ],
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert!(matches!(
            engine.process_key(c_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_tab_release_in_pending_emits_tab_tap() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Tab↓ → Pending
        assert!(matches!(
            engine.process_key(tab(), true, false, 0),
            EngineAction::Suppress
        ));

        // Tab↑ → emit Tab tap (down+up), back to Inactive
        assert_emit(
            &engine.process_key(tab(), false, false, 0),
            &[(tab(), true), (tab(), false)],
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
    }

    #[test]
    fn mouse_mode_pending_other_key_cancels_and_emits_both() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Tab↓ → Pending
        engine.process_key(tab(), true, false, 0);

        // D↓ (unmapped key) → cancel, emit Tab↓ + D↓ as synthetics
        let action = engine.process_key(d(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        assert_emit(&action, &[(tab(), true), (d(), true)]);

        // Tab↑ and D↑ should be handled via replayed_keys
        assert_emit(
            &engine.process_key(tab(), false, false, 0),
            &[(tab(), false)],
        );
        assert_emit(&engine.process_key(d(), false, false, 0), &[(d(), false)]);
    }

    #[test]
    fn mouse_mode_active_jkli_movement() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active mouse mode (Tab held → fast)
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // K↓ → combine with held J for diagonal movement (fast, Tab still held)
        assert_system(
            &engine.process_key(k(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 1,
                fast: true,
            },
        );

        // Release Tab → same vector, slower speed
        assert_system(
            &engine.process_key(tab(), false, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 1,
                fast: false,
            },
        );

        // L↓ → horizontal inputs cancel, leaving only downward motion
        assert_system(
            &engine.process_key(l(), true, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: 1,
                fast: false,
            },
        );

        // I↓ → vertical inputs cancel too, so movement stops
        assert_system(
            &engine.process_key(i_key(), true, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: 0,
                fast: false,
            },
        );

        // Re-press Tab → same held vector state, now fast again
        assert_system(
            &engine.process_key(tab(), true, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: 0,
                fast: true,
            },
        );

        // J autorepeat → suppressed once the key is already held
        assert!(matches!(
            engine.process_key(j(), true, true, 0),
            EngineAction::Suppress
        ));

        // J↑ → update the combined vector from the remaining held keys
        assert_system(
            &engine.process_key(j(), false, false, 0),
            SystemAction::MouseMove {
                x: 1,
                y: 0,
                fast: true,
            },
        );
    }

    #[test]
    fn mouse_mode_combines_held_directions_into_one_vector() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);

        assert_system(
            &engine.process_key(j(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: true,
            },
        );

        assert_system(
            &engine.process_key(i_key(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: -1,
                fast: true,
            },
        );

        assert_system(
            &engine.process_key(j(), false, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: -1,
                fast: true,
            },
        );

        assert_system(
            &engine.process_key(i_key(), false, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: 0,
                fast: true,
            },
        );
    }

    #[test]
    fn mouse_mode_tab_speed_changes_update_current_vector() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        assert_system(
            &engine.process_key(tab(), false, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: false,
            },
        );

        assert_system(
            &engine.process_key(tab(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: true,
            },
        );
    }

    #[test]
    fn mouse_mode_direction_autorepeat_is_suppressed_once_held() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        assert!(matches!(
            engine.process_key(j(), true, true, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_active_n_clicks_left_and_exits() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // N↓ → MouseClick Left AND exit mouse mode
        assert_system(
            &engine.process_key(n(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Left,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // N↑ should be suppressed (was moved to suppressed_releases on exit)
        assert!(matches!(
            engine.process_key(n(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_active_m_clicks_right() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // M↓ → MouseClick Right
        assert_system(
            &engine.process_key(m(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Right,
            },
        );
    }

    #[test]
    fn mouse_mode_active_r_triggers_mouse_back_and_stays_active() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        assert_system(
            &engine.process_key(r_key(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Back,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert!(matches!(
            engine.process_key(r_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_active_c_toggles_sidebar_and_stays_active() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        assert_emit_with_flags(
            &engine.process_key(c_key(), true, false, 0),
            &[
                (left_bracket_key(), true, keycode::FLAG_COMMAND),
                (left_bracket_key(), false, keycode::FLAG_COMMAND),
            ],
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        assert!(matches!(
            engine.process_key(c_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn windows_mouse_mode_c_uses_ctrl_b_sidebar_shortcut() {
        let mut engine = MappingEngine::new_for_platform(MappingPlatform::Windows);
        engine.set_enabled(true);

        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        assert_emit_with_flags(
            &engine.process_key(c_key(), true, false, 0),
            &[
                (b(), true, keycode::FLAG_CONTROL),
                (b(), false, keycode::FLAG_CONTROL),
            ],
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);
    }

    #[test]
    fn mouse_mode_n_exits_and_suppresses_key_up() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // N↓ → click and exit
        engine.process_key(n(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // N↑ should be suppressed (was moved to suppressed_releases on exit)
        assert!(matches!(
            engine.process_key(n(), false, false, 0),
            EngineAction::Suppress
        ));

        // J↑ should also be suppressed (was in mouse_held_keys when exit happened)
        assert!(matches!(
            engine.process_key(j(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_active_tab_suppressed() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // Tab release → keep the same vector but switch to slow mode
        assert_system(
            &engine.process_key(tab(), false, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: false,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // Tab re-press → same vector, fast mode again
        assert_system(
            &engine.process_key(tab(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: true,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);
    }

    #[test]
    fn mouse_mode_active_other_key_exits() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // D↓ (unrelated key) → exit mouse mode, reprocess D
        let action = engine.process_key(d(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        // D reprocessed through normal pipeline → PassThrough
        assert!(matches!(action, EngineAction::PassThrough));
    }

    #[test]
    fn mouse_mode_cancel_from_external() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // External cancel (e.g. physical mouse click)
        let action = engine.cancel_mouse_mode();
        assert!(action.is_some());
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // Double cancel → None
        assert!(engine.cancel_mouse_mode().is_none());
    }

    #[test]
    fn mouse_mode_tab_with_modifier_does_not_enter() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Shift+Tab → PassThrough (not mouse mode)
        let action = engine.process_key(tab(), true, false, keycode::FLAG_SHIFT);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        assert!(matches!(action, EngineAction::PassThrough));

        // Cmd+Tab → PassThrough (not mouse mode)
        let action = engine.process_key(tab(), true, false, keycode::FLAG_COMMAND);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        assert!(matches!(action, EngineAction::PassThrough));
    }

    #[test]
    fn mouse_mode_option_tab_triggers_alt_tab_not_mouse() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Option+Tab → AltTabCycle, NOT mouse mode
        let action = engine.process_key(tab(), true, false, keycode::FLAG_OPTION);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        assert_system(&action, SystemAction::AltTabCycle { reverse: false });
    }

    #[test]
    fn mouse_mode_set_enabled_false_resets() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // Disable → should reset
        engine.set_enabled(false);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
    }

    #[test]
    fn mouse_mode_tab_release_after_active_suppressed() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Tab↓ → Pending
        engine.process_key(tab(), true, false, 0);
        // J↓ → Active (Tab added to suppressed_releases)
        engine.process_key(j(), true, false, 0);
        // Release J
        engine.process_key(j(), false, false, 0);

        // Now exit mouse mode by pressing D
        engine.process_key(d(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // Tab↑ should be suppressed (was added to suppressed_releases)
        assert!(matches!(
            engine.process_key(tab(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_scroll_u_up_o_down() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // U↓ → MouseScroll Up
        assert_system(
            &engine.process_key(u_key(), true, false, 0),
            SystemAction::MouseScroll {
                direction: MouseScrollDirection::Up,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // U autorepeat → MouseScroll Up (continuous scrolling)
        assert_system(
            &engine.process_key(u_key(), true, true, 0),
            SystemAction::MouseScroll {
                direction: MouseScrollDirection::Up,
            },
        );

        // U↑ → Suppress
        assert!(matches!(
            engine.process_key(u_key(), false, false, 0),
            EngineAction::Suppress
        ));

        // O↓ → MouseScroll Down
        assert_system(
            &engine.process_key(o_key(), true, false, 0),
            SystemAction::MouseScroll {
                direction: MouseScrollDirection::Down,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // O↑ → Suppress
        assert!(matches!(
            engine.process_key(o_key(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_tab_speed_toggle() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active with Tab held → fast
        engine.process_key(tab(), true, false, 0);
        assert_system(
            &engine.process_key(j(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: true,
            },
        );

        // Release Tab → keep same vector, slow down immediately
        assert_system(
            &engine.process_key(tab(), false, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 0,
                fast: false,
            },
        );
        assert_system(
            &engine.process_key(k(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 1,
                fast: false,
            },
        );

        // Re-press Tab → same combined vector, fast again
        assert_system(
            &engine.process_key(tab(), true, false, 0),
            SystemAction::MouseMove {
                x: -1,
                y: 1,
                fast: true,
            },
        );
        assert_system(
            &engine.process_key(l(), true, false, 0),
            SystemAction::MouseMove {
                x: 0,
                y: 1,
                fast: true,
            },
        );
    }

    #[test]
    fn mouse_mode_n_autorepeat_suppressed_after_exit() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // N↓ → click and exit
        engine.process_key(n(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // N autorepeat after exit → suppressed (don't type 'n')
        assert!(matches!(
            engine.process_key(n(), true, true, 0),
            EngineAction::Suppress
        ));

        // N↑ → suppressed
        assert!(matches!(
            engine.process_key(n(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_held_jkli_autorepeat_suppressed_after_exit() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active, hold J
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // Exit with D
        engine.process_key(d(), true, false, 0);
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);

        // J autorepeat after exit → suppressed
        assert!(matches!(
            engine.process_key(j(), true, true, 0),
            EngineAction::Suppress
        ));

        // J↑ → suppressed
        assert!(matches!(
            engine.process_key(j(), false, false, 0),
            EngineAction::Suppress
        ));
    }

    #[test]
    fn mouse_mode_m_stays_active() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);

        // M↓ → right click, stay in mode
        assert_system(
            &engine.process_key(m(), true, false, 0),
            SystemAction::MouseClick {
                button: MouseButton::Right,
            },
        );
        assert_eq!(engine.mouse_mode, MouseModeState::Active);

        // M autorepeat → suppress
        assert!(matches!(
            engine.process_key(m(), true, true, 0),
            EngineAction::Suppress
        ));

        // M↑ → suppress
        assert!(matches!(
            engine.process_key(m(), false, false, 0),
            EngineAction::Suppress
        ));

        // Still active
        assert_eq!(engine.mouse_mode, MouseModeState::Active);
    }

    #[test]
    fn mouse_mode_cancel_resets_tab_held() {
        let mut engine = MappingEngine::new();
        engine.set_enabled(true);

        // Enter active with Tab held
        engine.process_key(tab(), true, false, 0);
        engine.process_key(j(), true, false, 0);
        assert!(engine.tab_held_in_mouse_mode);

        // External cancel (physical mouse click)
        engine.cancel_mouse_mode();
        assert_eq!(engine.mouse_mode, MouseModeState::Inactive);
        assert!(!engine.tab_held_in_mouse_mode);
    }
}
