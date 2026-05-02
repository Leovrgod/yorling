use serde::{Deserialize, Serialize};

use crate::transcript::TranscriptPreview;

/// Raw payload received from a hook bridge
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawHookPayload {
    /// Which hook event fired (e.g. "PreToolUse", "Notification")
    pub hook_event: String,
    /// The JSON body from the agent hook
    pub body: serde_json::Value,
    /// Source agent identifier (e.g. "claude", "codex")
    pub source: String,
    /// Session identifier from the agent (if available)
    pub session_id: Option<String>,
}

/// Terminal context attached to events for terminal-jump feature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalContext {
    pub pid: Option<u32>,
    pub tty: Option<String>,
    pub cwd: Option<String>,
    pub terminal_app: Option<String>,
    #[serde(default)]
    pub terminal_bundle_id: Option<String>,
    #[serde(default)]
    pub terminal_session_id: Option<String>,
    #[serde(default)]
    pub pane_title: Option<String>,
    #[serde(default)]
    pub warp_pane_uuid: Option<String>,
}

/// Normalized event flowing through the island system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IslandEvent {
    pub session_id: String,
    pub provider_id: String,
    pub timestamp: i64,
    pub event_type: IslandEventType,
    pub terminal_context: Option<TerminalContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum IslandEventType {
    SessionStart,
    SessionEnd,
    SessionDiscard,
    UserPrompt {
        text: String,
    },
    ToolUseStart {
        tool: String,
        input: serde_json::Value,
    },
    ToolUseEnd {
        tool: String,
        success: bool,
    },
    PermissionRequest {
        tool: String,
        input: serde_json::Value,
        request_id: String,
    },
    PermissionDecision {
        request_id: String,
        decision: Decision,
    },
    AskQuestion {
        question: String,
        options: Vec<String>,
        request_id: String,
        from_permission: bool,
        answer_key: Option<String>,
    },
    QuestionAnswer {
        request_id: String,
        answer: String,
    },
    AgentThinking,
    AgentResponse {
        text: String,
    },
    SubagentStart {
        subagent_id: String,
    },
    SubagentStop {
        subagent_id: String,
    },
    ContextCompaction,
    Stop,
    Notification {
        title: String,
        body: String,
    },
    TranscriptSync {
        preview: TranscriptPreview,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    AllowAlways,
}

/// Response sent back to the bridge for blocking requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub request_id: String,
    #[serde(flatten)]
    pub payload: BridgeResponsePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum BridgeResponsePayload {
    PermissionDecision { decision: Decision },
    QuestionAnswer { answer: String },
}
