use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyAction {
    Press,
    Release,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEvent {
    pub keycode: u16,
    pub action: KeyAction,
    pub flags: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineEvent {
    pub original: KeyEvent,
    pub result: EventResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventResult {
    PassThrough,
    Suppress,
    Remap { keycode: u16, flags: u64 },
}
