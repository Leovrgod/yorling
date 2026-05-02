use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};

use crate::session::{ChatMessage, ChatRole};

const CHAT_PREVIEW_LIMIT: usize = 100;
const TOOL_HISTORY_LIMIT: usize = 12;
const SNAPSHOT_TAIL_WINDOW_BYTES: u64 = 262_144;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptSyncStatus {
    #[default]
    Unavailable,
    Synced,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolHistoryState {
    #[default]
    Started,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ToolHistoryItem {
    pub id: String,
    pub tool: String,
    pub state: ToolHistoryState,
    pub input_preview: Option<String>,
    pub output_preview: Option<String>,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TranscriptPreview {
    pub transcript_path: String,
    pub provider_session_id: Option<String>,
    pub status: TranscriptSyncStatus,
    pub synced_at: Option<i64>,
    pub turn_count: usize,
    pub latest_task_started_at: Option<i64>,
    pub latest_task_finished_at: Option<i64>,
    pub latest_pending_question: Option<TranscriptPendingQuestion>,
    pub last_error: Option<String>,
    pub chat_preview: Vec<ChatMessage>,
    pub tool_history: Vec<ToolHistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TranscriptPendingQuestion {
    pub request_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub received_at: i64,
}

#[derive(Default)]
pub(crate) struct CachedTranscriptState {
    transcript_path: String,
    offset: u64,
    provider_session_id: Option<String>,
    turn_count: usize,
    latest_task_started_at: Option<i64>,
    latest_task_finished_at: Option<i64>,
    latest_pending_question: Option<TranscriptPendingQuestion>,
    synced_at: Option<i64>,
    last_error: Option<String>,
    chat_preview: Vec<ChatMessage>,
    tool_history: Vec<ToolHistoryItem>,
}

impl CachedTranscriptState {
    fn reset(&mut self, transcript_path: &str) {
        self.transcript_path = transcript_path.to_string();
        self.offset = 0;
        self.provider_session_id = None;
        self.turn_count = 0;
        self.latest_task_started_at = None;
        self.latest_task_finished_at = None;
        self.latest_pending_question = None;
        self.synced_at = None;
        self.last_error = None;
        self.chat_preview.clear();
        self.tool_history.clear();
    }

    fn snapshot(&self, status: TranscriptSyncStatus) -> TranscriptPreview {
        TranscriptPreview {
            transcript_path: self.transcript_path.clone(),
            provider_session_id: self.provider_session_id.clone(),
            status,
            synced_at: self.synced_at,
            turn_count: self.turn_count,
            latest_task_started_at: self.latest_task_started_at,
            latest_task_finished_at: self.latest_task_finished_at,
            latest_pending_question: self.latest_pending_question.clone(),
            last_error: self.last_error.clone(),
            chat_preview: self.chat_preview.clone(),
            tool_history: self.tool_history.clone(),
        }
    }
}

pub struct TranscriptManager {
    states: DashMap<String, CachedTranscriptState>,
}

impl TranscriptManager {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }

    pub async fn sync(
        &self,
        session_id: &str,
        transcript_path: &str,
        provider_session_id_hint: Option<&str>,
    ) -> TranscriptPreview {
        self.sync_with_provider(session_id, transcript_path, provider_session_id_hint, "")
            .await
    }

    pub async fn sync_with_provider(
        &self,
        session_id: &str,
        transcript_path: &str,
        provider_session_id_hint: Option<&str>,
        provider_id: &str,
    ) -> TranscriptPreview {
        let mut state = self
            .states
            .remove(session_id)
            .map(|(_, state)| state)
            .unwrap_or_default();

        if state.transcript_path != transcript_path {
            state.reset(transcript_path);
        }

        if let Some(provider_session_id_hint) = provider_session_id_hint {
            if !provider_session_id_hint.trim().is_empty() {
                state.provider_session_id = Some(provider_session_id_hint.to_string());
            }
        }

        let preview = match read_transcript_delta(&mut state, provider_id).await {
            Ok(status) => state.snapshot(status),
            Err(error) => {
                state.last_error = Some(error.to_string());
                state.synced_at = Some(current_timestamp());
                state.snapshot(TranscriptSyncStatus::Degraded)
            }
        };

        self.states.insert(session_id.to_string(), state);
        preview
    }
}

impl Default for TranscriptManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn latest_preview_activity_timestamp(preview: &TranscriptPreview) -> Option<i64> {
    preview
        .chat_preview
        .iter()
        .map(|message| message.timestamp)
        .chain(
            preview
                .tool_history
                .iter()
                .flat_map(|item| [item.started_at, item.finished_at].into_iter().flatten()),
        )
        .chain(
            [
                preview.latest_task_started_at,
                preview.latest_task_finished_at,
            ]
            .into_iter()
            .flatten(),
        )
        .chain(
            preview
                .latest_pending_question
                .as_ref()
                .map(|question| question.received_at),
        )
        .max()
}

async fn read_transcript_delta(
    state: &mut CachedTranscriptState,
    provider_id: &str,
) -> anyhow::Result<TranscriptSyncStatus> {
    let mut file = tokio::fs::File::open(&state.transcript_path).await?;
    let metadata = file.metadata().await?;
    let len = metadata.len();

    let initial_sync = state.offset == 0;
    if state.offset > len {
        state.reset(&state.transcript_path.clone());
    }

    let mut buffer = String::new();
    if initial_sync && len > SNAPSHOT_TAIL_WINDOW_BYTES {
        let start = len - SNAPSHOT_TAIL_WINDOW_BYTES;
        file.seek(SeekFrom::Start(start)).await?;
        file.read_to_string(&mut buffer).await?;

        if start > 0 {
            if let Some(index) = buffer.find('\n') {
                buffer = buffer[index + 1..].to_string();
            }
        }
    } else if state.offset < len {
        file.seek(SeekFrom::Start(state.offset)).await?;
        file.read_to_string(&mut buffer).await?;
    }

    if !buffer.is_empty() {
        for line in buffer.lines().filter(|line| !line.trim().is_empty()) {
            parse_transcript_line_for_provider(provider_id, line, state);
        }
    }

    state.offset = len;
    state.synced_at = Some(current_timestamp());
    state.last_error = None;

    Ok(TranscriptSyncStatus::Synced)
}

/// Provider-specific transcript parsing. Dispatches to specialized parsers
/// for known providers, with a generic fallback for unknown ones.
pub(crate) fn parse_transcript_line_for_provider(
    provider_id: &str,
    line: &str,
    state: &mut CachedTranscriptState,
) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };

    if state.provider_session_id.is_none() {
        state.provider_session_id = get_first_string(
            &value,
            &[
                "provider_session_id",
                "providerSessionId",
                "conversation_id",
                "conversationId",
                "session_id",
            ],
        );
    }

    match provider_id {
        "claude-code" => parse_claude_line(&value, state),
        "copilot" => parse_copilot_line(&value, state),
        "codex" => parse_codex_line(&value, state),
        "gemini" => parse_gemini_line(&value, state),
        _ => parse_generic_line(&value, state),
    }
}

/// Claude Code JSONL: lines are `{type: "assistant", message: {content: [{type: "text", ...}, {type: "tool_use", ...}]}}`
/// and `{type: "user", message: {content: [{type: "tool_result", tool_use_id, content, is_error}]}}`.
fn parse_claude_line(value: &Value, state: &mut CachedTranscriptState) {
    let line_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let content_arr = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());

    if let Some(items) = content_arr {
        for item in items {
            let item_type = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            match item_type {
                "text" => {
                    if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                        let text = normalize_preview_text(text);
                        if !text.is_empty() {
                            let role = match line_type {
                                "user" | "human" => ChatRole::User,
                                _ => ChatRole::Assistant,
                            };
                            push_chat_preview(
                                &mut state.chat_preview,
                                ChatMessage {
                                    role,
                                    text,
                                    timestamp: extract_timestamp(value)
                                        .unwrap_or_else(current_timestamp),
                                },
                            );
                            state.turn_count += 1;
                        }
                    }
                }
                "tool_use" => {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let tool = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let input_preview = item.get("input").map(preview_value);
                    let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool,
                            state: ToolHistoryState::Started,
                            input_preview,
                            output_preview: None,
                            started_at: Some(timestamp),
                            finished_at: None,
                        },
                    );
                }
                "tool_result" => {
                    let id = item
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    let is_error = item
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let output_preview = item.get("content").map(preview_value);
                    let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool: String::new(), // will be merged with existing
                            state: if is_error {
                                ToolHistoryState::Failed
                            } else {
                                ToolHistoryState::Succeeded
                            },
                            input_preview: None,
                            output_preview,
                            started_at: None,
                            finished_at: Some(timestamp),
                        },
                    );
                }
                _ => {}
            }
        }
    } else {
        // Fallback for simple message lines (e.g. {role: "user", content: "..."})
        parse_generic_line(value, state);
    }
}

/// Codex JSONL: lines are either `{type: "event_msg", msg: {role, content, ...}}`
/// or `{type: "response_item", item: {type: "function_call" | "message", ...}}`.
fn parse_codex_line(value: &Value, state: &mut CachedTranscriptState) {
    let payload = value
        .get("payload")
        .or_else(|| value.get("item"))
        .or_else(|| value.get("msg"));
    let line_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    match line_type {
        "session_meta" => {
            if let Some(payload) = payload {
                if state.provider_session_id.is_none() {
                    state.provider_session_id = get_first_string(payload, &["id"]);
                }
            }
        }
        "event_msg" => {
            if let Some(message_payload) = payload {
                let event_kind = message_payload
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                match event_kind {
                    "task_started" => {
                        let timestamp = extract_timestamp(message_payload)
                            .or_else(|| extract_timestamp(value))
                            .unwrap_or_else(current_timestamp);
                        state.latest_task_started_at = Some(timestamp);
                        state.latest_pending_question = None;
                    }
                    "task_complete" => {
                        let timestamp = extract_timestamp(message_payload)
                            .or_else(|| extract_timestamp(value))
                            .unwrap_or_else(current_timestamp);
                        state.latest_task_finished_at = Some(timestamp);
                        state.latest_pending_question = None;
                        if let Some(text) = message_payload
                            .get("last_agent_message")
                            .and_then(extract_text)
                        {
                            let text = normalize_preview_text(&text);
                            if !text.is_empty() {
                                push_chat_preview(
                                    &mut state.chat_preview,
                                    ChatMessage {
                                        role: ChatRole::Assistant,
                                        text,
                                        timestamp,
                                    },
                                );
                                state.turn_count += 1;
                            }
                        }
                    }
                    "agent_message" => {
                        if let Some(text) = message_payload.get("message").and_then(extract_text) {
                            let text = normalize_preview_text(&text);
                            if !text.is_empty() {
                                push_chat_preview(
                                    &mut state.chat_preview,
                                    ChatMessage {
                                        role: ChatRole::Assistant,
                                        text,
                                        timestamp: extract_timestamp(message_payload)
                                            .or_else(|| extract_timestamp(value))
                                            .unwrap_or_else(current_timestamp),
                                    },
                                );
                                state.turn_count += 1;
                            }
                        }
                    }
                    "exec_command_end" => {
                        let id = get_first_string(message_payload, &["call_id", "callId", "id"])
                            .unwrap_or_else(|| {
                                format!(
                                    "exec:{}",
                                    extract_timestamp(message_payload)
                                        .or_else(|| extract_timestamp(value))
                                        .unwrap_or_else(current_timestamp)
                                )
                            });
                        let output_preview = message_payload
                            .get("aggregated_output")
                            .or_else(|| message_payload.get("stdout"))
                            .or_else(|| message_payload.get("stderr"))
                            .map(preview_text_or_value);
                        let timestamp = extract_timestamp(message_payload)
                            .or_else(|| extract_timestamp(value))
                            .unwrap_or_else(current_timestamp);

                        merge_tool_update(
                            &mut state.tool_history,
                            ToolHistoryItem {
                                id,
                                tool: "Bash".into(),
                                state: if message_payload
                                    .get("exit_code")
                                    .and_then(|v| v.as_i64())
                                    .is_some_and(|code| code != 0)
                                {
                                    ToolHistoryState::Failed
                                } else {
                                    ToolHistoryState::Succeeded
                                },
                                input_preview: None,
                                output_preview,
                                started_at: None,
                                finished_at: Some(timestamp),
                            },
                        );
                    }
                    _ => {
                        if let Some(message) = extract_message(message_payload) {
                            push_chat_preview(&mut state.chat_preview, message);
                            state.turn_count += 1;
                        }
                    }
                }
            }
        }
        "response_item" => {
            if let Some(item) = payload {
                let item_type = item
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                match item_type {
                    "function_call" => {
                        let id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        // Codex uses "exec_command" for shell, normalize to "Bash"
                        let raw_name = item
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);
                        if raw_name == "request_user_input" {
                            state.latest_pending_question =
                                parse_codex_request_user_input_question(item, &id, timestamp);
                        }
                        let tool = if raw_name == "exec_command" || raw_name == "shell" {
                            "Bash".to_string()
                        } else {
                            raw_name.to_string()
                        };
                        let input_preview = item.get("arguments").map(preview_value);

                        merge_tool_update(
                            &mut state.tool_history,
                            ToolHistoryItem {
                                id,
                                tool,
                                state: ToolHistoryState::Started,
                                input_preview,
                                output_preview: None,
                                started_at: Some(timestamp),
                                finished_at: None,
                            },
                        );
                    }
                    "function_call_output" => {
                        let id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or_default()
                            .to_string();
                        if state
                            .latest_pending_question
                            .as_ref()
                            .is_some_and(|question| question.request_id.ends_with(&id))
                        {
                            state.latest_pending_question = None;
                        }
                        let output_preview = item.get("output").map(preview_value);
                        let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);

                        merge_tool_update(
                            &mut state.tool_history,
                            ToolHistoryItem {
                                id,
                                tool: String::new(),
                                state: ToolHistoryState::Succeeded,
                                input_preview: None,
                                output_preview,
                                started_at: None,
                                finished_at: Some(timestamp),
                            },
                        );
                    }
                    "message" => {
                        if let Some(message) = extract_message(item) {
                            push_chat_preview(&mut state.chat_preview, message);
                            state.turn_count += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {
            if matches!(line_type, "turn_aborted" | "error") {
                let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);
                state.latest_task_finished_at = Some(timestamp);
                state.latest_pending_question = None;
            } else {
                // Unknown Codex line type, try generic extraction
                parse_generic_line(value, state);
            }
        }
    }
}

fn parse_codex_request_user_input_question(
    item: &Value,
    call_id: &str,
    timestamp: i64,
) -> Option<TranscriptPendingQuestion> {
    let arguments = item.get("arguments")?;
    let arguments = match arguments {
        Value::String(raw) => serde_json::from_str::<Value>(raw).ok()?,
        Value::Object(_) => arguments.clone(),
        _ => return None,
    };

    let question_items = arguments
        .get("questions")
        .and_then(|value| value.as_array());
    let first_question = question_items.and_then(|items| items.first());
    let question = first_question
        .and_then(|value| value.get("question"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            first_question
                .and_then(|value| value.get("header"))
                .and_then(|value| value.as_str())
        })
        .unwrap_or("Codex is waiting for input.")
        .trim()
        .to_string();

    let options = first_question
        .and_then(|value| value.get("options"))
        .and_then(|value| value.as_array())
        .map(|options| {
            options
                .iter()
                .filter_map(|option| {
                    option
                        .get("label")
                        .and_then(|value| value.as_str())
                        .or_else(|| option.as_str())
                        .map(str::trim)
                        .filter(|label| !label.is_empty())
                        .map(str::to_string)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let request_id = if call_id.trim().is_empty() {
        format!("codex-input:{timestamp}")
    } else {
        format!("codex-input:{call_id}")
    };

    Some(TranscriptPendingQuestion {
        request_id,
        question,
        options,
        received_at: timestamp,
    })
}

pub fn is_codex_title_generation_prompt(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    let title_intent = normalized.contains("generate a concise ui title")
        || normalized.contains("return only the title")
        || normalized.contains("provide a short title for a task")
        || normalized.contains("short title for a task")
        || normalized.contains("concise title");
    let prompt_context =
        normalized.contains("user prompt") || normalized.contains("presented with a prompt");
    let assistant_wrapper = normalized.contains("you are a helpful assistant")
        || normalized.contains("you will be presented with");

    title_intent && prompt_context && assistant_wrapper
}

/// GitHub Copilot session-state JSONL.
fn parse_copilot_line(value: &Value, state: &mut CachedTranscriptState) {
    let line_type = value
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let data = value.get("data").unwrap_or(value);

    match line_type {
        "session.start" => {
            if state.provider_session_id.is_none() {
                state.provider_session_id = get_first_string(data, &["sessionId", "session_id"]);
            }
        }
        "user.message" | "assistant.message" | "system.message" => {
            let role = match line_type {
                "user.message" => Some(ChatRole::User),
                "assistant.message" => Some(ChatRole::Assistant),
                "system.message" => Some(ChatRole::System),
                _ => None,
            };
            let timestamp = extract_timestamp(value)
                .or_else(|| extract_timestamp(data))
                .unwrap_or_else(current_timestamp);

            if let (Some(role), Some(text)) = (role, data.get("content").and_then(extract_text)) {
                let text = normalize_preview_text(&text);
                if !text.is_empty() {
                    push_chat_preview(
                        &mut state.chat_preview,
                        ChatMessage {
                            role,
                            text,
                            timestamp,
                        },
                    );
                    state.turn_count += 1;
                }
            }

            if let Some(tool_requests) = data.get("toolRequests").and_then(|v| v.as_array()) {
                for request in tool_requests {
                    let id =
                        get_first_string(request, &["toolCallId", "id"]).unwrap_or_else(|| {
                            format!(
                                "{}:{}",
                                get_first_string(request, &["name"])
                                    .unwrap_or_else(|| "tool".into()),
                                timestamp
                            )
                        });
                    let tool = get_first_string(request, &["name", "toolName", "tool"])
                        .unwrap_or_else(|| "unknown".into());
                    let input_preview = request.get("arguments").map(preview_text_or_value);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool,
                            state: ToolHistoryState::Started,
                            input_preview,
                            output_preview: None,
                            started_at: Some(timestamp),
                            finished_at: None,
                        },
                    );
                }
            }
        }
        "hook.start" => {
            if data
                .get("hookType")
                .and_then(|v| v.as_str())
                .is_some_and(|hook| hook == "preToolUse")
            {
                let timestamp = extract_timestamp(value)
                    .or_else(|| extract_timestamp(data))
                    .unwrap_or_else(current_timestamp);
                let tool_calls = data
                    .get("input")
                    .and_then(|input| input.get("toolCalls"))
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                for tool_call in tool_calls {
                    let id =
                        get_first_string(&tool_call, &["id", "toolCallId"]).unwrap_or_else(|| {
                            format!(
                                "{}:{}",
                                get_first_string(&tool_call, &["name"])
                                    .unwrap_or_else(|| "tool".into()),
                                timestamp
                            )
                        });
                    let tool = get_first_string(&tool_call, &["name", "toolName", "tool"])
                        .unwrap_or_else(|| "unknown".into());
                    let input_preview = tool_call.get("args").map(preview_text_or_value);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool,
                            state: ToolHistoryState::Started,
                            input_preview,
                            output_preview: None,
                            started_at: Some(timestamp),
                            finished_at: None,
                        },
                    );
                }
            }
        }
        "tool.execution_complete" => {
            let timestamp = extract_timestamp(value)
                .or_else(|| extract_timestamp(data))
                .unwrap_or_else(current_timestamp);
            let output_preview = data
                .get("result")
                .map(preview_text_or_value)
                .or_else(|| data.get("toolResult").map(preview_text_or_value));

            merge_tool_update(
                &mut state.tool_history,
                ToolHistoryItem {
                    id: get_first_string(data, &["toolCallId", "id"])
                        .unwrap_or_else(|| format!("tool:{}", timestamp)),
                    tool: get_first_string(data, &["toolName", "name", "tool"]).unwrap_or_default(),
                    state: if data
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(true)
                    {
                        ToolHistoryState::Succeeded
                    } else {
                        ToolHistoryState::Failed
                    },
                    input_preview: None,
                    output_preview,
                    started_at: None,
                    finished_at: Some(timestamp),
                },
            );
        }
        _ => parse_generic_line(data, state),
    }
}

/// Gemini CLI JSONL: uses `{role, parts: [{text: "..."}]}` format.
fn parse_gemini_line(value: &Value, state: &mut CachedTranscriptState) {
    // Gemini uses "parts" array with "text" fields
    if let Some(parts) = value.get("parts").and_then(|v| v.as_array()) {
        let role = extract_role(value);
        let text_parts: Vec<String> = parts
            .iter()
            .filter_map(|part| {
                // Function call part
                if let Some(fc) = part.get("functionCall") {
                    let name = fc
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let id = fc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            let ts = extract_timestamp(value).unwrap_or_else(current_timestamp);
                            format!("{name}:{ts}")
                        });
                    let input_preview = fc.get("args").map(preview_value);
                    let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool: name,
                            state: ToolHistoryState::Started,
                            input_preview,
                            output_preview: None,
                            started_at: Some(timestamp),
                            finished_at: None,
                        },
                    );
                    return None;
                }

                // Function response part
                if let Some(fr) = part.get("functionResponse") {
                    let name = fr
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let id = fr
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            let ts = extract_timestamp(value).unwrap_or_else(current_timestamp);
                            format!("{name}:{ts}")
                        });
                    let output_preview = fr.get("response").map(preview_value);
                    let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);

                    merge_tool_update(
                        &mut state.tool_history,
                        ToolHistoryItem {
                            id,
                            tool: name,
                            state: ToolHistoryState::Succeeded,
                            input_preview: None,
                            output_preview,
                            started_at: None,
                            finished_at: Some(timestamp),
                        },
                    );
                    return None;
                }

                // Text part
                part.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        if !text_parts.is_empty() {
            let text = normalize_preview_text(&text_parts.join("\n"));
            if !text.is_empty() {
                if let Some(role) = role {
                    push_chat_preview(
                        &mut state.chat_preview,
                        ChatMessage {
                            role,
                            text,
                            timestamp: extract_timestamp(value).unwrap_or_else(current_timestamp),
                        },
                    );
                    state.turn_count += 1;
                }
            }
        }
    } else {
        // Fallback for Gemini lines without "parts"
        parse_generic_line(value, state);
    }
}

/// Generic fallback parser for unknown providers.
fn parse_generic_line(value: &Value, state: &mut CachedTranscriptState) {
    if let Some(message) = extract_message(value) {
        push_chat_preview(&mut state.chat_preview, message);
        state.turn_count += 1;
    }

    if let Some(tool_update) = extract_tool_update(value) {
        merge_tool_update(&mut state.tool_history, tool_update);
    }
}

fn push_chat_preview(chat_preview: &mut Vec<ChatMessage>, message: ChatMessage) {
    let is_duplicate = chat_preview
        .last()
        .is_some_and(|last| last.role == message.role && last.text == message.text);
    if is_duplicate {
        return;
    }

    chat_preview.push(message);
    if chat_preview.len() > CHAT_PREVIEW_LIMIT {
        let drain = chat_preview.len() - CHAT_PREVIEW_LIMIT;
        chat_preview.drain(0..drain);
    }
}

fn merge_tool_update(tool_history: &mut Vec<ToolHistoryItem>, update: ToolHistoryItem) {
    if let Some(existing) = tool_history.iter_mut().find(|item| item.id == update.id) {
        if !update.tool.is_empty() {
            existing.tool = update.tool;
        }
        if update.input_preview.is_some() {
            existing.input_preview = update.input_preview;
        }
        if update.output_preview.is_some() {
            existing.output_preview = update.output_preview;
        }
        if update.started_at.is_some() {
            existing.started_at = update.started_at;
        }
        if update.finished_at.is_some() {
            existing.finished_at = update.finished_at;
        }
        existing.state = update.state;
        return;
    }

    tool_history.push(update);
    if tool_history.len() > TOOL_HISTORY_LIMIT {
        let drain = tool_history.len() - TOOL_HISTORY_LIMIT;
        tool_history.drain(0..drain);
    }
}

fn extract_message(value: &Value) -> Option<ChatMessage> {
    let message = value.get("message").unwrap_or(value);
    let role = extract_role(message)?;
    let text = extract_text(message).or_else(|| {
        message
            .get("content")
            .and_then(extract_text)
            .or_else(|| value.get("content").and_then(extract_text))
    })?;
    let text = normalize_preview_text(&text);
    if text.is_empty() {
        return None;
    }

    Some(ChatMessage {
        role,
        text,
        timestamp: extract_timestamp(message)
            .or_else(|| extract_timestamp(value))
            .unwrap_or_else(current_timestamp),
    })
}

fn extract_tool_update(value: &Value) -> Option<ToolHistoryItem> {
    let ty = value
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let tool = get_first_string(value, &["tool_name", "toolName", "tool", "name"])?;

    let looks_toolish = ty.contains("tool")
        || value.get("tool_name").is_some()
        || value.get("toolName").is_some()
        || value.get("tool_input").is_some()
        || value.get("toolInput").is_some();
    if !looks_toolish {
        return None;
    }

    let timestamp = extract_timestamp(value).unwrap_or_else(current_timestamp);
    let id = get_first_string(
        value,
        &[
            "tool_use_id",
            "toolUseId",
            "tool_call_id",
            "toolCallId",
            "call_id",
            "callId",
            "id",
        ],
    )
    .unwrap_or_else(|| format!("{tool}:{timestamp}"));

    let input_preview = value
        .get("tool_input")
        .or_else(|| value.get("toolInput"))
        .or_else(|| value.get("input"))
        .or_else(|| value.get("arguments"))
        .map(preview_value);
    let output_preview = value
        .get("tool_output")
        .or_else(|| value.get("toolOutput"))
        .or_else(|| value.get("output"))
        .or_else(|| value.get("result"))
        .or_else(|| value.get("error"))
        .map(preview_value);

    let state = if value.get("error").is_some() {
        ToolHistoryState::Failed
    } else if output_preview.is_some()
        || value
            .get("success")
            .and_then(|value| value.as_bool())
            .is_some()
        || ty.contains("result")
        || ty.contains("complete")
    {
        if value.get("success").and_then(|value| value.as_bool()) == Some(false) {
            ToolHistoryState::Failed
        } else {
            ToolHistoryState::Succeeded
        }
    } else {
        ToolHistoryState::Started
    };

    let mut item = ToolHistoryItem {
        id,
        tool,
        state,
        input_preview,
        output_preview,
        started_at: Some(timestamp),
        finished_at: None,
    };
    if matches!(
        state,
        ToolHistoryState::Succeeded | ToolHistoryState::Failed
    ) {
        item.finished_at = Some(timestamp);
    }

    Some(item)
}

fn extract_role(value: &Value) -> Option<ChatRole> {
    let raw = value
        .get("role")
        .or_else(|| value.get("sender"))
        .or_else(|| value.get("author"))
        .or_else(|| value.get("kind"))
        .and_then(|value| value.as_str())?
        .to_ascii_lowercase();

    match raw.as_str() {
        "user" | "human" => Some(ChatRole::User),
        "assistant" | "ai" | "model" => Some(ChatRole::Assistant),
        "system" => Some(ChatRole::System),
        _ => None,
    }
}

fn extract_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.to_string()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(extract_text)
                .map(|text| normalize_preview_text(&text))
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        Value::Object(map) => {
            for key in [
                "text",
                "content",
                "message",
                "body",
                "output",
                "result",
                "detailedContent",
                "aggregated_output",
                "textResultForLlm",
                "stdout",
                "stderr",
            ] {
                if let Some(text) = map.get(key).and_then(extract_text) {
                    if !text.trim().is_empty() {
                        return Some(text);
                    }
                }
            }

            if map.get("type").and_then(|value| value.as_str()) == Some("text") {
                return map.get("text").and_then(extract_text);
            }

            None
        }
        _ => None,
    }
}

fn extract_timestamp(value: &Value) -> Option<i64> {
    for key in ["timestamp", "created_at", "createdAt", "ts"] {
        if let Some(number) = value.get(key).and_then(|value| value.as_i64()) {
            return Some(if number > 1_000_000_000_000 {
                number
            } else {
                number * 1000
            });
        }
        if let Some(text) = value.get(key).and_then(|value| value.as_str())
            && let Ok(timestamp) = OffsetDateTime::parse(text, &Rfc3339)
        {
            return Some((timestamp.unix_timestamp_nanos() / 1_000_000) as i64);
        }
    }
    None
}

fn preview_value(value: &Value) -> String {
    let raw = match value {
        Value::String(text) => text.to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    };
    normalize_preview_text(&raw)
}

fn preview_text_or_value(value: &Value) -> String {
    extract_text(value)
        .map(|text| normalize_preview_text(&text))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| preview_value(value))
}

fn normalize_preview_text(text: &str) -> String {
    let squashed = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if squashed.chars().count() <= 140 {
        squashed
    } else {
        let truncated = squashed.chars().take(137).collect::<String>();
        format!("{truncated}...")
    }
}

fn get_first_string(value: &Value, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(text) = value.get(key).and_then(|value| value.as_str()) {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
    }

    None
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_tool_blocks_with_provider_specific_logic() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/session.jsonl");

        parse_transcript_line_for_provider(
            "claude-code",
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Running targeted read."},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"path":"src-tauri/src/island.rs"}}]}}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "claude-code",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Opened island.rs","is_error":false}]}}"#,
            &mut state,
        );

        assert_eq!(state.chat_preview.len(), 1);
        assert_eq!(state.chat_preview[0].role, ChatRole::Assistant);
        assert!(state.chat_preview[0].text.contains("Running targeted read"));
        assert_eq!(state.tool_history.len(), 1);
        assert_eq!(state.tool_history[0].id, "toolu_1");
        assert_eq!(state.tool_history[0].tool, "Read");
        assert_eq!(state.tool_history[0].state, ToolHistoryState::Succeeded);
        assert_eq!(
            state.tool_history[0].input_preview.as_deref(),
            Some("{\"path\":\"src-tauri/src/island.rs\"}")
        );
        assert_eq!(
            state.tool_history[0].output_preview.as_deref(),
            Some("Opened island.rs")
        );
    }

    #[test]
    fn extracts_chat_preview_from_jsonl_lines() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/session.jsonl");

        parse_transcript_line_for_provider(
            "",
            r#"{"role":"user","content":"Fix the failing jump test"}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "",
            r#"{"role":"assistant","content":[{"type":"text","text":"I am checking the transcript parser now."}]}"#,
            &mut state,
        );

        assert_eq!(state.turn_count, 2);
        assert_eq!(state.chat_preview.len(), 2);
        assert_eq!(state.chat_preview[0].role, ChatRole::User);
        assert!(state.chat_preview[1].text.contains("transcript parser"));
    }

    #[test]
    fn parses_codex_payload_wrapped_entries() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/codex.jsonl");

        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:31.952Z","type":"session_meta","payload":{"id":"019daf2d-2542-7810-869e-8d584b4433b6","cwd":"/Users/example/yorling/yorling"}}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:43.311Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"我会先把灵动岛摸清楚。"}]}}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:43.311Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"pwd\"}","call_id":"call_1"}}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:43.385Z","type":"event_msg","payload":{"type":"exec_command_end","call_id":"call_1","aggregated_output":"/Users/example/yorling/yorling\n","exit_code":0}}"#,
            &mut state,
        );

        assert_eq!(
            state.provider_session_id.as_deref(),
            Some("019daf2d-2542-7810-869e-8d584b4433b6")
        );
        assert_eq!(state.chat_preview.len(), 1);
        assert_eq!(state.chat_preview[0].role, ChatRole::Assistant);
        assert!(state.chat_preview[0].text.contains("灵动岛"));
        assert_eq!(state.tool_history.len(), 1);
        assert_eq!(state.tool_history[0].tool, "Bash");
        assert_eq!(state.tool_history[0].state, ToolHistoryState::Succeeded);
    }

    #[test]
    fn parses_codex_request_user_input_as_pending_question() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/codex.jsonl");

        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:43.311Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","arguments":"{\"questions\":[{\"id\":\"choice\",\"header\":\"Mode\",\"question\":\"Choose a command\",\"options\":[{\"label\":\"Run tests\",\"description\":\"Execute checks\"},{\"label\":\"Skip\",\"description\":\"Continue without tests\"}]}]}","call_id":"call_question"}}"#,
            &mut state,
        );

        let question = state
            .latest_pending_question
            .as_ref()
            .expect("pending question");
        assert_eq!(question.request_id, "codex-input:call_question");
        assert_eq!(question.question, "Choose a command");
        assert_eq!(question.options, vec!["Run tests", "Skip"]);
    }

    #[test]
    fn parses_codex_task_lifecycle_markers() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/codex.jsonl");

        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:40.000Z","type":"event_msg","payload":{"type":"task_started"}}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "codex",
            r#"{"timestamp":"2026-04-21T08:34:45.000Z","type":"event_msg","payload":{"type":"task_complete","last_agent_message":"Done."}}"#,
            &mut state,
        );

        assert_eq!(state.latest_task_started_at, Some(1_776_760_480_000));
        assert_eq!(state.latest_task_finished_at, Some(1_776_760_485_000));
        assert_eq!(
            state
                .chat_preview
                .last()
                .map(|message| message.text.as_str()),
            Some("Done.")
        );
    }

    #[test]
    fn parses_copilot_transcript_messages_and_tools() {
        let mut state = CachedTranscriptState::default();
        state.reset("/tmp/copilot-events.jsonl");

        parse_transcript_line_for_provider(
            "copilot",
            r#"{"type":"session.start","data":{"sessionId":"0000075e-e04f-448f-b8bc-e2c7062511d0","context":{"cwd":"/Users/example/yorling/yorling"}},"timestamp":"2026-04-21T07:08:33.876Z"}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "copilot",
            r#"{"type":"user.message","data":{"content":"修复灵动岛问题"},"timestamp":"2026-04-21T07:13:57.762Z"}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "copilot",
            r#"{"type":"assistant.message","data":{"content":"我先定位相关模块。","toolRequests":[{"toolCallId":"call_abc","name":"view","arguments":{"path":"src/island/components/CollapsedBar.tsx"}}]},"timestamp":"2026-04-21T07:13:57.791Z"}"#,
            &mut state,
        );
        parse_transcript_line_for_provider(
            "copilot",
            r#"{"type":"tool.execution_complete","data":{"toolCallId":"call_abc","toolName":"view","success":true,"result":{"content":"Opened CollapsedBar.tsx"}},"timestamp":"2026-04-21T07:14:39.110Z"}"#,
            &mut state,
        );

        assert_eq!(
            state.provider_session_id.as_deref(),
            Some("0000075e-e04f-448f-b8bc-e2c7062511d0")
        );
        assert_eq!(state.chat_preview.len(), 2);
        assert_eq!(state.chat_preview[0].role, ChatRole::User);
        assert_eq!(state.chat_preview[1].role, ChatRole::Assistant);
        assert_eq!(state.tool_history.len(), 1);
        assert_eq!(state.tool_history[0].tool, "view");
        assert_eq!(state.tool_history[0].state, ToolHistoryState::Succeeded);
        assert_eq!(
            state.tool_history[0].output_preview.as_deref(),
            Some("Opened CollapsedBar.tsx")
        );
    }

    #[test]
    fn merges_tool_history_by_id() {
        let mut history = Vec::new();

        merge_tool_update(
            &mut history,
            ToolHistoryItem {
                id: "tool-1".into(),
                tool: "Read".into(),
                state: ToolHistoryState::Started,
                input_preview: Some("{\"path\":\"README.md\"}".into()),
                output_preview: None,
                started_at: Some(1000),
                finished_at: None,
            },
        );
        merge_tool_update(
            &mut history,
            ToolHistoryItem {
                id: "tool-1".into(),
                tool: "Read".into(),
                state: ToolHistoryState::Succeeded,
                input_preview: None,
                output_preview: Some("Opened README.md".into()),
                started_at: None,
                finished_at: Some(1100),
            },
        );

        assert_eq!(history.len(), 1);
        assert_eq!(history[0].state, ToolHistoryState::Succeeded);
        assert_eq!(history[0].finished_at, Some(1100));
        assert_eq!(
            history[0].output_preview.as_deref(),
            Some("Opened README.md")
        );
    }
}
