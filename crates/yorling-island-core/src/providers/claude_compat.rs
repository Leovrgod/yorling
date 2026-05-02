use serde_json::Value;

use crate::event::{IslandEvent, IslandEventType, RawHookPayload, TerminalContext};

pub(crate) fn normalize_claude_compatible_event(
    provider_id: &str,
    display_name: &str,
    raw: &RawHookPayload,
    assistant_notification_types: &[&str],
    allow_pre_tool_questions: bool,
) -> anyhow::Result<IslandEvent> {
    let session_id = raw
        .session_id
        .clone()
        .or_else(|| {
            first_string(
                &raw.body,
                &["session_id", "provider_session_id", "sessionId"],
            )
        })
        .unwrap_or_else(|| "unknown".to_string());

    let timestamp = current_timestamp();
    let terminal_context = extract_terminal_context(&raw.body);

    let event_type = match raw.hook_event.as_str() {
        "PreToolUse" => {
            let tool = first_string(&raw.body, &["tool_name", "tool", "name"])
                .unwrap_or_else(|| "unknown".to_string());
            let input = raw
                .body
                .get("tool_input")
                .cloned()
                .or_else(|| raw.body.get("input").cloned())
                .unwrap_or_else(|| serde_json::json!({}));

            if allow_pre_tool_questions && is_question_tool(&tool) {
                let (question, options, answer_key) = extract_question_from_input(&input);
                IslandEventType::AskQuestion {
                    question,
                    options,
                    request_id: request_id_from_body(&raw.body, &session_id, timestamp),
                    from_permission: true,
                    answer_key: Some(answer_key),
                }
            } else {
                IslandEventType::ToolUseStart { tool, input }
            }
        }
        "PostToolUse" => {
            let tool = first_string(&raw.body, &["tool_name", "tool", "name"])
                .unwrap_or_else(|| "unknown".to_string());
            let success = !raw.body.get("error").is_some_and(|error| !error.is_null());
            IslandEventType::ToolUseEnd { tool, success }
        }
        "PostToolUseFailure" => {
            let tool = first_string(&raw.body, &["tool_name", "tool", "name"])
                .unwrap_or_else(|| "unknown".to_string());
            IslandEventType::ToolUseEnd {
                tool,
                success: false,
            }
        }
        "Notification" => {
            if let Some(question) = raw.body.get("question").and_then(|value| value.as_str()) {
                let options = extract_option_labels(raw.body.get("options"));
                IslandEventType::AskQuestion {
                    question: question.to_string(),
                    options,
                    request_id: request_id_from_body(&raw.body, &session_id, timestamp),
                    from_permission: false,
                    answer_key: None,
                }
            } else if is_assistant_notification(&raw.body, assistant_notification_types) {
                let text =
                    first_string(&raw.body, &["message", "response", "text"]).unwrap_or_default();
                IslandEventType::AgentResponse { text }
            } else {
                let title = raw
                    .body
                    .get("title")
                    .or_else(|| raw.body.get("notification_type"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let body = raw
                    .body
                    .get("message")
                    .or_else(|| raw.body.get("details"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::Notification { title, body }
            }
        }
        "AgentResponse" => IslandEventType::AgentResponse {
            text: first_string(&raw.body, &["message", "response", "text"]).unwrap_or_default(),
        },
        "Stop" => IslandEventType::Stop,
        "SubagentStart" => {
            let subagent_id = raw
                .body
                .get("subagent_id")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            IslandEventType::SubagentStart { subagent_id }
        }
        "SubagentStop" => {
            let subagent_id = raw
                .body
                .get("subagent_id")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string();
            IslandEventType::SubagentStop { subagent_id }
        }
        "UserPromptSubmit" => IslandEventType::UserPrompt {
            text: raw
                .body
                .get("prompt")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "SessionStart" => IslandEventType::SessionStart,
        "SessionEnd" => IslandEventType::SessionEnd,
        "PreCompact" => IslandEventType::ContextCompaction,
        "PermissionRequest" => normalize_permission_request(&raw.body, &session_id, timestamp),
        other => anyhow::bail!("Unknown {display_name} hook event: {other}"),
    };

    Ok(IslandEvent {
        session_id,
        provider_id: provider_id.to_string(),
        timestamp,
        event_type,
        terminal_context,
    })
}

pub(crate) fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
    let cwd = body
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(String::from);
    if cwd.is_none() {
        return None;
    }

    Some(TerminalContext {
        pid: body
            .get("pid")
            .and_then(|value| value.as_u64())
            .map(|pid| pid as u32),
        tty: body
            .get("tty")
            .and_then(|value| value.as_str())
            .map(String::from),
        cwd,
        terminal_app: body
            .get("terminal_app")
            .or_else(|| body.get("terminalApp"))
            .and_then(|value| value.as_str())
            .map(String::from),
        terminal_bundle_id: body
            .get("terminal_bundle_id")
            .or_else(|| body.get("terminalBundleId"))
            .and_then(|value| value.as_str())
            .map(String::from),
        terminal_session_id: body
            .get("terminal_session_id")
            .or_else(|| body.get("terminalSessionId"))
            .or_else(|| body.get("session_uuid"))
            .and_then(|value| value.as_str())
            .map(String::from),
        pane_title: body
            .get("pane_title")
            .or_else(|| body.get("paneTitle"))
            .or_else(|| body.get("title"))
            .and_then(|value| value.as_str())
            .map(String::from),
        warp_pane_uuid: body
            .get("warp_pane_uuid")
            .or_else(|| body.get("warpPaneUuid"))
            .and_then(|value| value.as_str())
            .map(String::from),
    })
}

pub(crate) fn first_string(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(|value| value.as_str()))
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn extract_option_labels(options: Option<&Value>) -> Vec<String> {
    match options {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(value) => Some(value.to_string()),
                Value::Object(_) => item
                    .get("label")
                    .or_else(|| item.get("title"))
                    .and_then(|value| value.as_str())
                    .map(str::to_string),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(crate) fn request_id_from_body(body: &Value, session_id: &str, timestamp: i64) -> String {
    first_string(
        body,
        &["request_id", "requestId", "tool_use_id", "toolUseId", "id"],
    )
    .unwrap_or_else(|| format!("{session_id}:{timestamp}"))
}

pub(crate) fn is_question_tool(tool: &str) -> bool {
    matches!(
        tool,
        "AskUserQuestion" | "ask_user_question" | "ask_followup_question"
    )
}

pub(crate) fn normalize_permission_request(
    body: &Value,
    session_id: &str,
    timestamp: i64,
) -> IslandEventType {
    let tool = first_string(body, &["tool_name", "tool", "message"])
        .unwrap_or_else(|| "unknown".to_string());
    let input = body
        .get("tool_input")
        .cloned()
        .or_else(|| body.get("input").cloned())
        .unwrap_or_else(|| serde_json::json!({}));
    let request_id = request_id_from_body(body, session_id, timestamp);

    if is_question_tool(&tool) {
        let (question, options, answer_key) = extract_question_from_input(&input);
        return IslandEventType::AskQuestion {
            question,
            options,
            request_id,
            from_permission: true,
            answer_key: Some(answer_key),
        };
    }

    IslandEventType::PermissionRequest {
        tool,
        input,
        request_id,
    }
}

fn extract_question_from_input(input: &Value) -> (String, Vec<String>, String) {
    if let Some(first_question) = input
        .get("questions")
        .and_then(|value| value.as_array())
        .and_then(|questions| questions.first())
    {
        let question = first_question
            .get("question")
            .and_then(|value| value.as_str())
            .unwrap_or("Question")
            .to_string();
        let options = extract_option_labels(first_question.get("options"));
        let answer_key = first_string(first_question, &["header", "id"])
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "answer".to_string());
        return (question, options, answer_key);
    }

    let question = input
        .get("question")
        .and_then(|value| value.as_str())
        .unwrap_or("Question")
        .to_string();
    let options = extract_option_labels(input.get("options"));
    (question, options, "answer".to_string())
}

fn is_assistant_notification(body: &Value, assistant_notification_types: &[&str]) -> bool {
    let notification_type = body
        .get("notification_type")
        .or_else(|| body.get("title"))
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    assistant_notification_types
        .iter()
        .any(|expected| notification_type == *expected)
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
