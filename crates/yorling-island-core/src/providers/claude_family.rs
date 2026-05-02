use std::borrow::Cow;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::Value;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

const MATCH_ANY: &[&str] = &["*"];
const MATCH_COMPACTION: &[&str] = &["auto", "manual"];

#[derive(Clone, Copy)]
pub enum HookEntryStyle {
    Plain,
    Matchers(&'static [&'static str]),
    MatchersWithTimeout(&'static [&'static str], u64),
}

#[derive(Clone, Copy)]
pub struct ClaudeFamilyEventSpec {
    pub name: &'static str,
    pub style: HookEntryStyle,
}

pub struct ClaudeFamilyProviderSpec {
    pub id: &'static str,
    pub display_name: &'static str,
    pub settings_relative_path: &'static str,
    pub detection_paths: &'static [&'static str],
    pub detection_commands: &'static [&'static str],
    pub supports_blocking_permission: bool,
    pub events: &'static [ClaudeFamilyEventSpec],
    pub required_events: &'static [&'static str],
}

pub struct ClaudeFamilyProvider {
    spec: &'static ClaudeFamilyProviderSpec,
}

impl ClaudeFamilyProvider {
    pub fn from_spec(spec: &'static ClaudeFamilyProviderSpec) -> Self {
        Self { spec }
    }

    fn settings_path(&self) -> anyhow::Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(self.spec.settings_relative_path))
    }

    fn build_hook_command(&self, bridge_path: &Path, transport_endpoint: &Path) -> String {
        format!(
            "{} --source {} {} {}",
            super::shell_quote(bridge_path),
            self.spec.id,
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
        )
    }

    fn detection_path_bufs(&self) -> Vec<PathBuf> {
        let Some(home) = dirs::home_dir() else {
            return Vec::new();
        };

        self.spec
            .detection_paths
            .iter()
            .map(|relative| home.join(relative))
            .collect()
    }
}

#[async_trait]
impl AgentProvider for ClaudeFamilyProvider {
    fn id(&self) -> &str {
        self.spec.id
    }

    fn display_name(&self) -> &str {
        self.spec.display_name
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let settings_path = self.settings_path()?;
        let hook_cmd = self.build_hook_command(bridge_path, transport_endpoint);

        let mut settings: Value = if settings_path.exists() {
            let content = tokio::fs::read_to_string(&settings_path).await?;
            serde_json::from_str(&content).map_err(|error| {
                anyhow::anyhow!("Invalid JSON in {}: {}", settings_path.display(), error)
            })?
        } else {
            serde_json::json!({})
        };

        let hooks_obj = settings
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Settings is not an object"))?;

        let hooks = hooks_obj
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}));

        let hooks_map = hooks
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

        for event in self.spec.events {
            merge_hook_entries(
                hooks_map,
                event.name,
                build_hook_entries(&hook_cmd, event.name, event.style),
            )?;
        }

        if let Some(parent) = settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&settings_path, json_str).await?;

        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let settings_path = self.settings_path()?;
        if !settings_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&settings_path).await?;
        let mut settings: Value = serde_json::from_str(&content).map_err(|error| {
            anyhow::anyhow!("Invalid JSON in {}: {}", settings_path.display(), error)
        })?;

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
        let settings_path = match self.settings_path() {
            Ok(path) => path,
            Err(_) => return HookStatus::NotInstalled,
        };

        if !settings_path.exists() {
            return HookStatus::NotInstalled;
        }

        let content = match tokio::fs::read_to_string(&settings_path).await {
            Ok(content) => content,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Cannot read settings file".into(),
                };
            }
        };

        let settings: Value = match serde_json::from_str(&content) {
            Ok(value) => value,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Invalid JSON in settings".into(),
                };
            }
        };

        let expected_cmd_fragment = self.build_hook_command(bridge_path, transport_endpoint);

        let Some(hooks) = settings.get("hooks").and_then(|value| value.as_object()) else {
            return HookStatus::NotInstalled;
        };
        let has_stale_legacy_hook = hooks.values().any(|entries| {
            entries
                .as_array()
                .is_some_and(|entries| entries.iter().any(is_stale_legacy_external_hook_entry))
        });

        let found_count = self
            .spec
            .required_events
            .iter()
            .filter(|event| {
                hooks
                    .get(**event)
                    .and_then(|entries| entries.as_array())
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            entry
                                .get("hooks")
                                .and_then(|nested| nested.as_array())
                                .is_some_and(|nested| {
                                    nested.iter().any(|hook| {
                                        hook.get("command")
                                            .and_then(|command| command.as_str())
                                            .is_some_and(|command| {
                                                command.contains(&expected_cmd_fragment)
                                            })
                                    })
                                })
                        })
                    })
            })
            .count();

        if found_count == 0 {
            if has_stale_legacy_hook {
                HookStatus::Outdated
            } else {
                HookStatus::NotInstalled
            }
        } else if found_count < self.spec.required_events.len() {
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
                    .and_then(|value| value.as_str())
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
                    .get("tool_input")
                    .cloned()
                    .or_else(|| raw.body.get("input").cloned())
                    .unwrap_or_else(|| serde_json::json!({}));
                IslandEventType::ToolUseStart { tool, input }
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
            "UserPromptSubmit" => {
                let text = raw
                    .body
                    .get("prompt")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::UserPrompt { text }
            }
            "SessionStart" => IslandEventType::SessionStart,
            "SessionEnd" => IslandEventType::SessionEnd,
            "PreCompact" => IslandEventType::ContextCompaction,
            "PermissionRequest" => normalize_permission_request(&raw.body, &session_id, timestamp),
            other => anyhow::bail!("Unknown {} hook event: {}", self.spec.display_name, other),
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
        self.spec.supports_blocking_permission
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

    fn config_paths(&self) -> Vec<PathBuf> {
        self.settings_path().into_iter().collect()
    }

    fn detection_paths(&self) -> Vec<PathBuf> {
        self.detection_path_bufs()
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        self.spec.detection_commands
    }
}

fn build_hook_entries(hook_cmd: &str, event_name: &str, style: HookEntryStyle) -> Vec<Value> {
    let full_cmd = format!("{hook_cmd} --event {event_name}");
    let (matchers, timeout) = match style {
        HookEntryStyle::Plain => {
            return vec![serde_json::json!({
                "hooks": [{
                    "type": "command",
                    "command": full_cmd,
                }]
            })];
        }
        HookEntryStyle::Matchers(matchers) => (matchers, None),
        HookEntryStyle::MatchersWithTimeout(matchers, timeout) => (matchers, Some(timeout)),
    };

    matchers
        .iter()
        .map(|matcher| {
            let mut command_hook = serde_json::json!({
                "type": "command",
                "command": full_cmd.clone(),
            });
            if let Some(timeout) = timeout {
                command_hook["timeout"] = serde_json::json!(timeout);
            }

            serde_json::json!({
                "matcher": matcher,
                "hooks": [command_hook],
            })
        })
        .collect()
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

fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
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

const QWEN_CODE_EVENTS: &[ClaudeFamilyEventSpec] = &[
    ClaudeFamilyEventSpec {
        name: "UserPromptSubmit",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "PreToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PostToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PostToolUseFailure",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "Notification",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "SessionStart",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "SessionEnd",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "Stop",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "SubagentStart",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "SubagentStop",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PreCompact",
        style: HookEntryStyle::Matchers(MATCH_COMPACTION),
    },
    ClaudeFamilyEventSpec {
        name: "PermissionRequest",
        style: HookEntryStyle::MatchersWithTimeout(MATCH_ANY, 86_400),
    },
];

const QODER_EVENTS: &[ClaudeFamilyEventSpec] = &[
    ClaudeFamilyEventSpec {
        name: "UserPromptSubmit",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "PreToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PostToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PostToolUseFailure",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PermissionRequest",
        style: HookEntryStyle::MatchersWithTimeout(MATCH_ANY, 86_400),
    },
    ClaudeFamilyEventSpec {
        name: "Notification",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "Stop",
        style: HookEntryStyle::Plain,
    },
];

const CODEBUDDY_EVENTS: &[ClaudeFamilyEventSpec] = &[
    ClaudeFamilyEventSpec {
        name: "UserPromptSubmit",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "PreToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "PostToolUse",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "Notification",
        style: HookEntryStyle::Matchers(MATCH_ANY),
    },
    ClaudeFamilyEventSpec {
        name: "Stop",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "SubagentStop",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "SessionStart",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "SessionEnd",
        style: HookEntryStyle::Plain,
    },
    ClaudeFamilyEventSpec {
        name: "PreCompact",
        style: HookEntryStyle::Matchers(MATCH_COMPACTION),
    },
];

pub const QWEN_CODE_SPEC: ClaudeFamilyProviderSpec = ClaudeFamilyProviderSpec {
    id: "qwen-code",
    display_name: "Qwen Code",
    settings_relative_path: ".qwen/settings.json",
    detection_paths: &[".qwen", ".qwen/settings.json"],
    detection_commands: &["qwen"],
    supports_blocking_permission: true,
    events: QWEN_CODE_EVENTS,
    required_events: &[
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
    ],
};

pub const QODER_SPEC: ClaudeFamilyProviderSpec = ClaudeFamilyProviderSpec {
    id: "qoder",
    display_name: "Qoder",
    settings_relative_path: ".qoder/settings.json",
    detection_paths: &[".qoder", ".qoder/settings.json"],
    detection_commands: &["qoder"],
    supports_blocking_permission: true,
    events: QODER_EVENTS,
    required_events: &[
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
    ],
};

pub const QODERWORK_SPEC: ClaudeFamilyProviderSpec = ClaudeFamilyProviderSpec {
    id: "qoderwork",
    display_name: "QoderWork",
    settings_relative_path: ".qoderwork/settings.json",
    detection_paths: &[".qoderwork", ".qoderwork/settings.json"],
    detection_commands: &["qoderwork"],
    supports_blocking_permission: true,
    events: QODER_EVENTS,
    required_events: &[
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PermissionRequest",
    ],
};

pub const CODEBUDDY_SPEC: ClaudeFamilyProviderSpec = ClaudeFamilyProviderSpec {
    id: "codebuddy",
    display_name: "CodeBuddy",
    settings_relative_path: ".codebuddy/settings.json",
    detection_paths: &[".codebuddy", ".codebuddy/settings.json"],
    detection_commands: &[],
    supports_blocking_permission: false,
    events: CODEBUDDY_EVENTS,
    required_events: &["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"],
};

pub const WORKBUDDY_SPEC: ClaudeFamilyProviderSpec = ClaudeFamilyProviderSpec {
    id: "workbuddy",
    display_name: "WorkBuddy",
    settings_relative_path: ".workbuddy/settings.json",
    detection_paths: &[".workbuddy", ".workbuddy/settings.json"],
    detection_commands: &[],
    supports_blocking_permission: false,
    events: CODEBUDDY_EVENTS,
    required_events: &["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"],
};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ClaudeFamilyProvider, QODER_SPEC, QWEN_CODE_SPEC};
    use crate::event::{Decision, IslandEventType, RawHookPayload};
    use crate::provider::AgentProvider;
    use serde_json::Value;

    #[test]
    fn hook_command_uses_provider_identifier() {
        let provider = ClaudeFamilyProvider::from_spec(&QWEN_CODE_SPEC);
        let command = provider.build_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
        );

        assert!(command.contains("--source qwen-code"));
    }

    #[test]
    fn post_tool_use_failure_is_normalized_as_failed_tool_end() {
        let provider = ClaudeFamilyProvider::from_spec(&QODER_SPEC);
        let raw = RawHookPayload {
            hook_event: "PostToolUseFailure".into(),
            body: serde_json::json!({
                "session_id": "session-1",
                "tool_name": "Bash",
            }),
            source: "qoder".into(),
            session_id: Some("session-1".into()),
        };

        let event = provider.normalize_event(&raw).expect("should normalize");

        match event.event_type {
            IslandEventType::ToolUseEnd { tool, success } => {
                assert_eq!(tool, "Bash");
                assert!(!success);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn permission_responses_follow_claude_hook_shape() {
        let provider = ClaudeFamilyProvider::from_spec(&QWEN_CODE_SPEC);
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
