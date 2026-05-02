use std::borrow::Cow;
use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

pub struct ClaudeCodeProvider;

impl ClaudeCodeProvider {
    pub fn new() -> Self {
        Self
    }

    fn claude_settings_path() -> anyhow::Result<std::path::PathBuf> {
        let config_dir = std::env::var("CLAUDE_CONFIG_DIR")
            .ok()
            .map(std::path::PathBuf::from);
        let path = config_dir.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".claude")
        });
        Ok(path.join("settings.json"))
    }

    fn build_hook_command(bridge_path: &Path, transport_endpoint: &Path) -> String {
        format!(
            "{} --source claude-code {} {}",
            super::shell_quote(bridge_path),
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
        )
    }
}

#[async_trait]
impl AgentProvider for ClaudeCodeProvider {
    fn id(&self) -> &str {
        "claude-code"
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let settings_path = Self::claude_settings_path()?;
        let hook_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        // Read or create existing settings
        let mut settings: Value = if settings_path.exists() {
            let content = tokio::fs::read_to_string(&settings_path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        // Build hooks configuration
        // Claude Code hooks format:
        // "hooks": {
        //   "PreToolUse": [{ "matcher": "", "hooks": [{ "type": "command", "command": "..." }] }],
        //   ...
        // }
        let hooks_obj = settings
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Settings is not an object"))?;

        let hooks = hooks_obj
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));

        let hooks_map = hooks
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

        for event_name in [
            "UserPromptSubmit",
            "Stop",
            "SubagentStop",
            "SessionStart",
            "SessionEnd",
        ] {
            merge_hook_entries(
                hooks_map,
                event_name,
                build_claude_entries(&hook_cmd, event_name, HookEntryStyle::Plain)?,
            )?;
        }

        for event_name in [
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Notification",
        ] {
            let style = if event_name == "PermissionRequest" {
                HookEntryStyle::WildcardWithTimeout(86_400)
            } else {
                HookEntryStyle::Wildcard
            };
            merge_hook_entries(
                hooks_map,
                event_name,
                build_claude_entries(&hook_cmd, event_name, style)?,
            )?;
        }

        merge_hook_entries(
            hooks_map,
            "PreCompact",
            build_claude_entries(&hook_cmd, "PreCompact", HookEntryStyle::PreCompact)?,
        )?;

        // Ensure parent directory exists
        if let Some(parent) = settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&settings_path, json_str).await?;

        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let settings_path = Self::claude_settings_path()?;
        if !settings_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&settings_path).await?;
        let mut settings: Value = serde_json::from_str(&content)?;

        let mut should_remove_hooks = false;
        if let Some(hooks) = settings.pointer_mut("/hooks") {
            if let Some(hooks_map) = hooks.as_object_mut() {
                hooks_map.retain(|_, entries| {
                    if let Some(arr) = entries.as_array_mut() {
                        arr.retain(|entry| !is_yorling_or_legacy_hook_entry(entry));
                        return !arr.is_empty();
                    }
                    true
                });
                should_remove_hooks = hooks_map.is_empty();
            }
        }

        if should_remove_hooks {
            if let Some(root) = settings.as_object_mut() {
                root.remove("hooks");
            }
        }

        let json_str = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&settings_path, json_str).await?;

        Ok(())
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let settings_path = match Self::claude_settings_path() {
            Ok(p) => p,
            Err(_) => return HookStatus::NotInstalled,
        };

        if !settings_path.exists() {
            return HookStatus::NotInstalled;
        }

        let content = match tokio::fs::read_to_string(&settings_path).await {
            Ok(c) => c,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Cannot read settings file".into(),
                };
            }
        };

        let settings: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Invalid JSON in settings".into(),
                };
            }
        };

        let expected_cmd_fragment = Self::build_hook_command(bridge_path, transport_endpoint);

        let Some(hooks) = settings.get("hooks").and_then(|h| h.as_object()) else {
            return HookStatus::NotInstalled;
        };
        let has_stale_legacy_hook = hooks.values().any(|entries| {
            entries
                .as_array()
                .is_some_and(|entries| entries.iter().any(is_stale_legacy_external_hook_entry))
        });

        let required_events = [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Notification",
        ];
        let mut found_count = 0;

        for event in &required_events {
            if let Some(entries) = hooks.get(*event).and_then(|e| e.as_array()) {
                let has_yorling = entries.iter().any(|entry| {
                    entry
                        .pointer("/hooks/0/command")
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c.contains(&expected_cmd_fragment))
                });
                if has_yorling {
                    found_count += 1;
                }
            }
        }

        if found_count == 0 {
            if has_stale_legacy_hook {
                HookStatus::Outdated
            } else {
                HookStatus::NotInstalled
            }
        } else if found_count < required_events.len() {
            HookStatus::Outdated
        } else if has_stale_legacy_hook {
            HookStatus::Outdated
        } else {
            HookStatus::Installed
        }
    }

    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
        let session_id = raw
            .session_id
            .clone()
            .or_else(|| {
                raw.body
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "unknown".to_string());

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let terminal_context = extract_terminal_context(&raw.body);

        let event_type = match raw.hook_event.as_str() {
            "PreToolUse" => {
                let tool = first_string(&raw.body, &["tool_name", "tool", "name"])
                    .unwrap_or_else(|| "unknown".to_string());
                let input = raw
                    .body
                    .get("input")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                IslandEventType::ToolUseStart { tool, input }
            }
            "PostToolUse" => {
                let tool = first_string(&raw.body, &["tool_name", "tool", "name"])
                    .unwrap_or_else(|| "unknown".to_string());
                let success = !raw.body.get("error").is_some_and(|e| !e.is_null());
                IslandEventType::ToolUseEnd { tool, success }
            }
            "Notification" => {
                if let Some(question) = raw.body.get("question").and_then(|q| q.as_str()) {
                    let options = extract_option_labels(raw.body.get("options"));
                    IslandEventType::AskQuestion {
                        question: question.to_string(),
                        options,
                        request_id: request_id_from_body(&raw.body, &session_id, timestamp),
                        from_permission: false,
                        answer_key: None,
                    }
                } else {
                    let title = raw
                        .body
                        .get("title")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    let body = raw
                        .body
                        .get("message")
                        .and_then(|b| b.as_str())
                        .unwrap_or("")
                        .to_string();
                    IslandEventType::Notification { title, body }
                }
            }
            "Stop" => IslandEventType::Stop,
            "SubagentStop" => {
                let subagent_id = raw
                    .body
                    .get("subagent_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                IslandEventType::SubagentStop { subagent_id }
            }
            "UserPromptSubmit" => {
                let text = raw
                    .body
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::UserPrompt { text }
            }
            "SessionStart" => IslandEventType::SessionStart,
            "SessionEnd" => IslandEventType::SessionEnd,
            "PreCompact" => IslandEventType::ContextCompaction,
            "PermissionRequest" => normalize_permission_request(&raw.body, &session_id, timestamp),
            other => {
                anyhow::bail!("Unknown Claude Code hook event: {}", other);
            }
        };

        Ok(IslandEvent {
            session_id,
            provider_id: self.id().to_string(),
            timestamp,
            event_type,
            terminal_context,
        })
    }

    fn supports_blocking_permission(&self) -> bool {
        true
    }

    fn encode_permission_response(&self, decision: Decision) -> anyhow::Result<Vec<u8>> {
        let behavior = match decision {
            Decision::Allow | Decision::AllowAlways => "allow",
            Decision::Deny => "deny",
        };

        Ok(serde_json::to_vec(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": {
                    "behavior": behavior,
                }
            }
        }))?)
    }

    fn encode_question_response(
        &self,
        answer: &str,
        from_permission: bool,
        answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        if from_permission {
            Ok(serde_json::to_vec(&serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": {
                        "behavior": "allow",
                        "updatedInput": {
                            "answers": {
                                answer_key.unwrap_or("answer"): answer,
                            }
                        }
                    }
                }
            }))?)
        } else {
            Ok(serde_json::to_vec(&serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "Notification",
                    "answer": answer,
                }
            }))?)
        }
    }

    fn config_paths(&self) -> Vec<std::path::PathBuf> {
        Self::claude_settings_path().into_iter().collect()
    }

    fn detection_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
            let config_dir = std::path::PathBuf::from(config_dir);
            paths.push(config_dir.clone());
            paths.push(config_dir.join("settings.json"));
            return paths;
        }

        if let Some(home) = dirs::home_dir() {
            let root = home.join(".claude");
            paths.push(root.clone());
            paths.push(root.join("settings.json"));
        }

        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["claude"]
    }
}

fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
    let cwd = body.get("cwd").and_then(|c| c.as_str()).map(String::from);
    if cwd.is_some() {
        Some(TerminalContext {
            pid: body.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32),
            tty: body.get("tty").and_then(|t| t.as_str()).map(String::from),
            cwd,
            terminal_app: body
                .get("terminal_app")
                .or_else(|| body.get("terminalApp"))
                .and_then(|t| t.as_str())
                .map(String::from),
            terminal_bundle_id: body
                .get("terminal_bundle_id")
                .or_else(|| body.get("terminalBundleId"))
                .and_then(|t| t.as_str())
                .map(String::from),
            terminal_session_id: body
                .get("terminal_session_id")
                .or_else(|| body.get("terminalSessionId"))
                .or_else(|| body.get("session_uuid"))
                .and_then(|t| t.as_str())
                .map(String::from),
            pane_title: body
                .get("pane_title")
                .or_else(|| body.get("paneTitle"))
                .or_else(|| body.get("title"))
                .and_then(|t| t.as_str())
                .map(String::from),
            warp_pane_uuid: body
                .get("warp_pane_uuid")
                .or_else(|| body.get("warpPaneUuid"))
                .and_then(|t| t.as_str())
                .map(String::from),
        })
    } else {
        None
    }
}

#[derive(Clone, Copy)]
enum HookEntryStyle {
    Plain,
    Wildcard,
    WildcardWithTimeout(u64),
    PreCompact,
}

fn build_claude_entries(
    hook_cmd: &str,
    event_name: &str,
    style: HookEntryStyle,
) -> anyhow::Result<Vec<Value>> {
    let full_cmd = format!("{hook_cmd} --event {event_name}");
    let command_hook = match style {
        HookEntryStyle::WildcardWithTimeout(timeout) => serde_json::json!({
            "type": "command",
            "command": full_cmd,
            "timeout": timeout,
        }),
        _ => serde_json::json!({
            "type": "command",
            "command": full_cmd,
        }),
    };

    let entries = match style {
        HookEntryStyle::Plain => vec![serde_json::json!({
            "hooks": [command_hook]
        })],
        HookEntryStyle::Wildcard | HookEntryStyle::WildcardWithTimeout(_) => {
            vec![serde_json::json!({
                "matcher": "*",
                "hooks": [command_hook]
            })]
        }
        HookEntryStyle::PreCompact => vec![
            serde_json::json!({
                "matcher": "auto",
                "hooks": [command_hook.clone()],
            }),
            serde_json::json!({
                "matcher": "manual",
                "hooks": [command_hook],
            }),
        ],
    };

    Ok(entries)
}

fn merge_hook_entries(
    hooks_map: &mut serde_json::Map<String, Value>,
    event_name: &str,
    new_entries: Vec<Value>,
) -> anyhow::Result<()> {
    if let Some(existing) = hooks_map.get_mut(event_name) {
        let arr = existing
            .as_array_mut()
            .ok_or_else(|| anyhow::anyhow!("Hook entry for {} is not an array", event_name))?;
        arr.retain(|entry| !is_yorling_or_legacy_hook_entry(entry));
        arr.extend(new_entries);
    } else {
        hooks_map.insert(event_name.to_string(), Value::Array(new_entries));
    }

    Ok(())
}

fn is_yorling_hook_entry(entry: &Value) -> bool {
    hook_entry_contains_command(entry, |command| command.contains("yorling-bridge"))
}

fn is_stale_legacy_external_hook_entry(entry: &Value) -> bool {
    hook_entry_contains_command(entry, is_stale_legacy_external_command)
}

fn is_stale_legacy_external_command(command: &str) -> bool {
    if !(command.contains(concat!("ping", "-island-bridge"))
        || command.contains(concat!("/.", "ping", "-island/")))
    {
        return false;
    }

    let Some(program) = leading_command_program(command) else {
        return false;
    };

    let program_path = Path::new(&program);
    program_path.is_absolute()
        && program.contains(concat!("/.", "ping", "-island/"))
        && !program_path.exists()
}

fn leading_command_program(command: &str) -> Option<String> {
    let trimmed = command.trim_start();
    let mut chars = trimmed.chars();
    let first = chars.next()?;

    if first == '\'' || first == '"' {
        let quote = first;
        let mut token = String::new();
        let mut escaped = false;
        for ch in chars {
            if escaped {
                token.push(ch);
                escaped = false;
            } else if quote == '"' && ch == '\\' {
                escaped = true;
            } else if ch == quote {
                return Some(token);
            } else {
                token.push(ch);
            }
        }
        return None;
    }

    Some(trimmed.split_whitespace().next()?.to_string())
}

fn is_yorling_or_legacy_hook_entry(entry: &Value) -> bool {
    is_yorling_hook_entry(entry) || is_stale_legacy_external_hook_entry(entry)
}

fn hook_entry_contains_command(entry: &Value, predicate: impl Fn(&str) -> bool) -> bool {
    entry
        .get("hooks")
        .and_then(|hooks| hooks.as_array())
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(|command| command.as_str())
                    .is_some_and(|command| predicate(command))
            })
        })
}

fn normalize_permission_request(body: &Value, session_id: &str, timestamp: i64) -> IslandEventType {
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

fn first_string(body: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(|value| value.as_str()))
        .map(ToOwned::to_owned)
}

fn request_id_from_body(body: &Value, session_id: &str, timestamp: i64) -> String {
    first_string(body, &["request_id", "tool_use_id", "id"])
        .unwrap_or_else(|| format!("{session_id}:{timestamp}"))
}

fn is_question_tool(tool: &str) -> bool {
    matches!(
        tool,
        "AskUserQuestion" | "ask_user_question" | "ask_followup_question"
    )
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

fn extract_option_labels(options: Option<&Value>) -> Vec<String> {
    match options {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::String(value) => Some(Cow::Borrowed(value.as_str())),
                Value::Object(_) => item
                    .get("label")
                    .and_then(|value| value.as_str())
                    .map(Cow::Borrowed),
                _ => None,
            })
            .map(|value| value.into_owned())
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClaudeCodeProvider, extract_question_from_input, is_stale_legacy_external_hook_entry,
        normalize_permission_request,
    };
    use crate::event::{Decision, IslandEventType, RawHookPayload};
    use crate::provider::AgentProvider;
    use serde_json::Value;
    use std::path::{Path, PathBuf};

    #[test]
    fn hook_command_uses_registered_provider_identifier() {
        let command = ClaudeCodeProvider::build_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
        );

        assert!(command.contains("--source claude-code"));
    }

    #[test]
    fn stale_legacy_external_hook_requires_missing_old_executable() {
        let (_root, missing_bridge) = temp_legacy_bridge_path("missing");
        let stale_entry =
            legacy_hook_entry(&format!("{} --source claude", missing_bridge.display()));

        assert!(is_stale_legacy_external_hook_entry(&stale_entry));

        let (root, live_bridge) = temp_legacy_bridge_path("live");
        std::fs::create_dir_all(live_bridge.parent().expect("bridge parent"))
            .expect("create bridge dir");
        std::fs::write(&live_bridge, "#!/bin/sh\n").expect("write bridge");
        let live_entry = legacy_hook_entry(&format!("'{}' --source claude", live_bridge.display()));

        assert!(!is_stale_legacy_external_hook_entry(&live_entry));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn permission_request_for_question_tool_is_normalized_as_question() {
        let event = normalize_permission_request(
            &serde_json::json!({
                "tool_name": "AskUserQuestion",
                "tool_input": {
                    "questions": [
                        {
                            "header": "workflow",
                            "question": "How should I proceed?",
                            "options": [
                                { "label": "Directly" },
                                { "label": "Plan first" }
                            ]
                        }
                    ]
                },
                "request_id": "req-1"
            }),
            "session-1",
            42,
        );

        match event {
            IslandEventType::AskQuestion {
                question,
                options,
                request_id,
                from_permission,
                answer_key,
            } => {
                assert_eq!(question, "How should I proceed?");
                assert_eq!(options, vec!["Directly", "Plan first"]);
                assert_eq!(request_id, "req-1");
                assert!(from_permission);
                assert_eq!(answer_key.as_deref(), Some("workflow"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn extract_question_from_simple_input_uses_default_answer_key() {
        let (question, options, answer_key) = extract_question_from_input(&serde_json::json!({
            "question": "Pick one",
            "options": ["A", "B"]
        }));

        assert_eq!(question, "Pick one");
        assert_eq!(options, vec!["A", "B"]);
        assert_eq!(answer_key, "answer");
    }

    #[test]
    fn pre_tool_use_reads_tool_name_when_tool_field_is_missing() {
        let provider = ClaudeCodeProvider::new();
        let raw = RawHookPayload {
            hook_event: "PreToolUse".into(),
            body: serde_json::json!({
                "session_id": "session-1",
                "tool_name": "Bash",
                "input": { "command": "echo hi" }
            }),
            source: "claude-code".into(),
            session_id: Some("session-1".into()),
        };

        let event = provider.normalize_event(&raw).expect("should normalize");

        match event.event_type {
            IslandEventType::ToolUseStart { tool, .. } => assert_eq!(tool, "Bash"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn post_tool_use_reads_tool_name_when_tool_field_is_missing() {
        let provider = ClaudeCodeProvider::new();
        let raw = RawHookPayload {
            hook_event: "PostToolUse".into(),
            body: serde_json::json!({
                "session_id": "session-1",
                "tool_name": "Bash"
            }),
            source: "claude-code".into(),
            session_id: Some("session-1".into()),
        };

        let event = provider.normalize_event(&raw).expect("should normalize");

        match event.event_type {
            IslandEventType::ToolUseEnd { tool, .. } => assert_eq!(tool, "Bash"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    fn legacy_hook_entry(command: &str) -> Value {
        serde_json::json!({
            "hooks": [
                {
                    "type": "command",
                    "command": command
                }
            ]
        })
    }

    fn temp_legacy_bridge_path(label: &str) -> (PathBuf, PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "yorling-hook-test-{}-{}-{label}",
            std::process::id(),
            nanos
        ));
        let bridge = root
            .join(concat!(".", "ping", "-island"))
            .join("bin")
            .join(concat!("ping", "-island-bridge"));
        (root, bridge)
    }

    #[test]
    fn permission_responses_follow_claude_hook_shape() {
        let provider = ClaudeCodeProvider::new();
        let payload = provider
            .encode_permission_response(Decision::AllowAlways)
            .expect("permission payload");
        let json: Value = serde_json::from_slice(&payload).expect("valid json");

        assert_eq!(
            json.pointer("/hookSpecificOutput/hookEventName")
                .and_then(|value| value.as_str()),
            Some("PermissionRequest")
        );
        assert_eq!(
            json.pointer("/hookSpecificOutput/decision/behavior")
                .and_then(|value| value.as_str()),
            Some("allow")
        );
    }
}
