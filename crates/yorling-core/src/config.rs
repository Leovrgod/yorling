use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppConfig {
    pub version: u32,
    pub profiles: Vec<Profile>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Profile {
    pub name: String,
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub app_rules: Vec<AppRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Layer {
    pub name: String,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type")]
pub enum Rule {
    #[serde(rename = "simple")]
    Simple { from: KeyRef, to: KeyRef },
    #[serde(rename = "tap_hold")]
    TapHold {
        from: KeyRef,
        tap: KeyRef,
        hold: HoldAction,
        #[serde(default = "default_hold_timeout")]
        hold_timeout_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeyRef {
    pub key: String,
    #[serde(default)]
    pub modifiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HoldAction {
    pub key: String,
    #[serde(default)]
    pub layer: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AppRule {
    pub app_identifier: String,
    pub overrides: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    #[serde(default = "default_hold_timeout")]
    pub tap_hold_timeout_ms: u64,
    #[serde(default = "default_emergency_hotkey")]
    pub emergency_hotkey: String,
    /// When false (default), switching foreground app resets to Base Layer.
    /// When true, layer state persists across app switches.
    #[serde(default)]
    pub layer_sticky: bool,
}

fn default_hold_timeout() -> u64 {
    200
}

fn default_emergency_hotkey() -> String {
    "Cmd+Option+Shift+Delete".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: vec![Profile {
                name: "Default".to_string(),
                layers: vec![
                    Layer {
                        name: "Base".to_string(),
                        rules: vec![Rule::TapHold {
                            from: KeyRef {
                                key: "Space".to_string(),
                                modifiers: vec![],
                            },
                            tap: KeyRef {
                                key: "Space".to_string(),
                                modifiers: vec![],
                            },
                            hold: HoldAction {
                                key: "LayerToggle".to_string(),
                                layer: Some(1),
                            },
                            hold_timeout_ms: 200,
                        }],
                    },
                    Layer {
                        name: "Navigation".to_string(),
                        rules: vec![
                            Rule::Simple {
                                from: KeyRef {
                                    key: "J".to_string(),
                                    modifiers: vec![],
                                },
                                to: KeyRef {
                                    key: "LeftArrow".to_string(),
                                    modifiers: vec![],
                                },
                            },
                            Rule::Simple {
                                from: KeyRef {
                                    key: "K".to_string(),
                                    modifiers: vec![],
                                },
                                to: KeyRef {
                                    key: "DownArrow".to_string(),
                                    modifiers: vec![],
                                },
                            },
                            Rule::Simple {
                                from: KeyRef {
                                    key: "I".to_string(),
                                    modifiers: vec![],
                                },
                                to: KeyRef {
                                    key: "UpArrow".to_string(),
                                    modifiers: vec![],
                                },
                            },
                            Rule::Simple {
                                from: KeyRef {
                                    key: "L".to_string(),
                                    modifiers: vec![],
                                },
                                to: KeyRef {
                                    key: "RightArrow".to_string(),
                                    modifiers: vec![],
                                },
                            },
                        ],
                    },
                ],
                app_rules: vec![],
            }],
            settings: Settings {
                tap_hold_timeout_ms: 200,
                emergency_hotkey: default_emergency_hotkey(),
                layer_sticky: false,
            },
        }
    }
}
