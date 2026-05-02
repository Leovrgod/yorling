use std::{collections::HashSet, path::Path, sync::Mutex};

use async_trait::async_trait;
use serde_json::Value;

use crate::provider::{AgentProvider, HookStatus, SkipHookEvent};
use crate::{
    event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext},
    transcript::is_codex_title_generation_prompt,
};

pub struct CodexProvider {
    ignored_auxiliary_sessions: Mutex<HashSet<String>>,
}

impl CodexProvider {
    pub fn new() -> Self {
        Self {
            ignored_auxiliary_sessions: Mutex::new(HashSet::new()),
        }
    }

    fn hooks_json_path() -> anyhow::Result<std::path::PathBuf> {
        let home = std::env::var("CODEX_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))
            .ok_or_else(|| anyhow::anyhow!("Cannot determine Codex home directory"))?;
        Ok(home.join("hooks.json"))
    }

    fn config_toml_path() -> anyhow::Result<std::path::PathBuf> {
        let home = std::env::var("CODEX_HOME")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))
            .ok_or_else(|| anyhow::anyhow!("Cannot determine Codex home directory"))?;
        Ok(home.join("config.toml"))
    }

    fn build_event_hook_command(
        bridge_path: &Path,
        transport_endpoint: &Path,
        event_name: &str,
    ) -> String {
        let command = format!(
            "{} --source codex {} {} --event {}",
            super::shell_quote(bridge_path),
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
            event_name,
        );

        #[cfg(windows)]
        {
            command
        }

        #[cfg(not(windows))]
        {
            format!(
                "sh -c {}",
                shell_quote_command(&format!("{command} 2>/dev/null"))
            )
        }
    }

    fn should_ignore_cached_auxiliary_session(&self, session_id: &str, hook_event: &str) -> bool {
        if session_id == "unknown" {
            return false;
        }

        let mut ignored_sessions = self
            .ignored_auxiliary_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let should_ignore = ignored_sessions.contains(session_id);
        if should_ignore && matches!(hook_event, "Stop" | "SessionEnd") {
            ignored_sessions.remove(session_id);
        }

        should_ignore
    }

    fn remember_auxiliary_session(&self, session_id: &str) {
        if session_id == "unknown" {
            return;
        }

        self.ignored_auxiliary_sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(session_id.to_string());
    }
}

#[async_trait]
impl AgentProvider for CodexProvider {
    fn id(&self) -> &str {
        "codex"
    }

    fn display_name(&self) -> &str {
        "Codex CLI"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let hooks_path = Self::hooks_json_path()?;

        // Read or create existing hooks.json
        let mut hooks_root: Value = if hooks_path.exists() {
            let content = tokio::fs::read_to_string(&hooks_path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let hooks_map = hooks_root
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks.json is not an object"))?
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

        // Codex "nested" format: { "EventName": [{ "matcher": "...", "hooks": [{ "type": "command", "command": "..." }] }] }
        let events = [
            ("SessionStart", Some("*"), 5),
            ("UserPromptSubmit", None, 5),
            ("PreToolUse", Some("*"), 5),
            ("PostToolUse", Some("*"), 5),
            ("PermissionRequest", Some("*"), 300),
            ("Stop", None, 5),
        ];

        for (event_name, matcher, timeout) in events {
            let command_hook = serde_json::json!({
                "type": "command",
                "command": Self::build_event_hook_command(bridge_path, transport_endpoint, event_name),
                "timeout": timeout,
            });

            let entry = if let Some(m) = matcher {
                serde_json::json!({
                    "matcher": m,
                    "hooks": [command_hook],
                })
            } else {
                serde_json::json!({
                    "hooks": [command_hook],
                })
            };

            if let Some(existing) = hooks_map.get_mut(event_name) {
                if let Some(arr) = existing.as_array_mut() {
                    arr.retain(|e| !is_yorling_entry(e));
                    arr.push(entry);
                }
            } else {
                hooks_map.insert(event_name.to_string(), Value::Array(vec![entry]));
            }
        }

        // Ensure parent directory exists
        if let Some(parent) = hooks_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&hooks_root)?;
        tokio::fs::write(&hooks_path, json_str).await?;

        // Also ensure hooks are enabled in config.toml
        Self::ensure_hooks_enabled().await?;

        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let hooks_path = Self::hooks_json_path()?;
        if !hooks_path.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&hooks_path).await?;
        let mut root: Value = serde_json::from_str(&content)?;

        let mut should_remove_hooks = false;
        if let Some(hooks) = root.pointer_mut("/hooks") {
            if let Some(hooks_map) = hooks.as_object_mut() {
                hooks_map.retain(|_, entries| {
                    if let Some(arr) = entries.as_array_mut() {
                        arr.retain(|entry| !is_yorling_entry(entry));
                        return !arr.is_empty();
                    }
                    true
                });
                should_remove_hooks = hooks_map.is_empty();
            }
        }

        if should_remove_hooks {
            if let Some(obj) = root.as_object_mut() {
                obj.remove("hooks");
            }
        }

        let json_str = serde_json::to_string_pretty(&root)?;
        tokio::fs::write(&hooks_path, json_str).await?;

        Ok(())
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let hooks_path = match Self::hooks_json_path() {
            Ok(p) => p,
            Err(_) => return HookStatus::NotInstalled,
        };

        if !hooks_path.exists() {
            return HookStatus::NotInstalled;
        }

        let content = match tokio::fs::read_to_string(&hooks_path).await {
            Ok(c) => c,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Cannot read hooks.json".into(),
                };
            }
        };

        let root: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Invalid JSON in hooks.json".into(),
                };
            }
        };

        let Some(hooks) = root.get("hooks").and_then(|h| h.as_object()) else {
            return HookStatus::NotInstalled;
        };

        let required = [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
        ];
        let found = required
            .iter()
            .filter(|event| {
                hooks
                    .get(**event)
                    .and_then(|e| e.as_array())
                    .is_some_and(|arr| {
                        arr.iter().any(|entry| {
                            codex_hook_entry_matches(entry, bridge_path, transport_endpoint, event)
                        })
                    })
            })
            .count();

        if found == 0 {
            HookStatus::NotInstalled
        } else if found < required.len() {
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

        if self.should_ignore_cached_auxiliary_session(&session_id, &raw.hook_event) {
            return Err(SkipHookEvent::new(format!(
                "ignored auxiliary Codex session {session_id}"
            ))
            .into());
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let terminal_context = extract_terminal_context(&raw.body);
        if is_codex_internal_memory_task(&raw.body) {
            if session_id == "unknown" {
                return Err(SkipHookEvent::new("ignored Codex internal memory task").into());
            }
            self.remember_auxiliary_session(&session_id);
            return Ok(IslandEvent {
                session_id,
                provider_id: self.id().to_string(),
                timestamp,
                event_type: IslandEventType::SessionDiscard,
                terminal_context,
            });
        }

        let event_type = match raw.hook_event.as_str() {
            "SessionStart" => IslandEventType::SessionStart,
            "UserPromptSubmit" => {
                let text = extract_prompt(&raw.body).unwrap_or("").to_string();
                if is_codex_title_generation_prompt(&text) {
                    if session_id == "unknown" {
                        return Err(SkipHookEvent::new(
                            "ignored Codex title-generation helper session",
                        )
                        .into());
                    }
                    self.remember_auxiliary_session(&session_id);
                    return Ok(IslandEvent {
                        session_id,
                        provider_id: self.id().to_string(),
                        timestamp,
                        event_type: IslandEventType::SessionDiscard,
                        terminal_context,
                    });
                }
                IslandEventType::UserPrompt { text }
            }
            "PreToolUse" => {
                let tool = raw
                    .body
                    .get("tool_name")
                    .or_else(|| raw.body.get("tool"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let input = raw
                    .body
                    .get("tool_input")
                    .or_else(|| raw.body.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                IslandEventType::ToolUseStart { tool, input }
            }
            "PostToolUse" => {
                let tool = raw
                    .body
                    .get("tool_name")
                    .or_else(|| raw.body.get("tool"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let success = !raw.body.get("error").is_some_and(|e| !e.is_null());
                IslandEventType::ToolUseEnd { tool, success }
            }
            "PermissionRequest" => {
                let tool = raw
                    .body
                    .get("tool_name")
                    .or_else(|| raw.body.get("tool"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let input = raw
                    .body
                    .get("tool_input")
                    .or_else(|| raw.body.get("input"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let request_id =
                    codex_permission_request_id(&raw.body, &session_id, &tool, timestamp);
                IslandEventType::PermissionRequest {
                    tool,
                    input,
                    request_id,
                }
            }
            "Stop" => IslandEventType::Stop,
            other => {
                anyhow::bail!("Unknown Codex hook event: {}", other);
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
        // Codex PermissionRequest keeps the hook process open until Yorling replies.
        true
    }

    fn encode_permission_response(&self, decision: Decision) -> anyhow::Result<Vec<u8>> {
        let denied = matches!(decision, Decision::Deny);
        let mut decision_payload = serde_json::json!({
            "behavior": if denied { "deny" } else { "allow" },
        });

        if denied && let Some(obj) = decision_payload.as_object_mut() {
            obj.insert(
                "message".to_string(),
                serde_json::Value::String("Denied by Yorling".into()),
            );
        }

        Ok(serde_json::to_vec(&serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": "PermissionRequest",
                "decision": decision_payload,
            }
        }))?)
    }

    fn encode_question_response(
        &self,
        _answer: &str,
        _from_permission: bool,
        _answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        // Codex does not support question responses through hooks
        Ok(Vec::new())
    }

    fn config_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        if let Ok(path) = Self::hooks_json_path() {
            paths.push(path);
        }
        if let Ok(path) = Self::config_toml_path() {
            paths.push(path);
        }
        paths
    }

    fn detection_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        if let Some(path) = std::env::var_os("CODEX_HOME").map(std::path::PathBuf::from) {
            paths.push(path.clone());
            paths.push(path.join("hooks.json"));
            paths.push(path.join("config.toml"));
            return paths;
        }

        if let Some(home) = dirs::home_dir() {
            let root = home.join(".codex");
            paths.push(root.clone());
            paths.push(root.join("hooks.json"));
            paths.push(root.join("config.toml"));
        }

        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["codex"]
    }
}

impl CodexProvider {
    async fn ensure_hooks_enabled() -> anyhow::Result<()> {
        let config_path = Self::config_toml_path()?;
        let content = if config_path.exists() {
            tokio::fs::read_to_string(&config_path).await?
        } else {
            String::new()
        };

        // Check if hooks feature flag is already set
        if content.contains("codex_hooks") && content.contains("true") {
            return Ok(());
        }

        // Append or update the features section
        let updated = if content.contains("[features]") {
            if content.contains("codex_hooks") {
                content.replace("codex_hooks = false", "codex_hooks = true")
            } else {
                content.replace("[features]", "[features]\ncodex_hooks = true")
            }
        } else {
            format!("{content}\n[features]\ncodex_hooks = true\n")
        };

        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&config_path, updated).await?;
        Ok(())
    }
}

fn is_yorling_entry(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(|hooks| hooks.as_array())
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(|c| c.as_str())
                    .is_some_and(|c| c.contains("yorling-bridge"))
            })
        })
}

fn codex_hook_entry_matches(
    entry: &Value,
    bridge_path: &Path,
    transport_endpoint: &Path,
    event_name: &str,
) -> bool {
    entry
        .get("hooks")
        .and_then(|hooks| hooks.as_array())
        .is_some_and(|hooks| {
            hooks.iter().any(|hook| {
                hook.get("command")
                    .and_then(|command| command.as_str())
                    .is_some_and(|command| {
                        codex_hook_command_matches(
                            command,
                            bridge_path,
                            transport_endpoint,
                            event_name,
                        )
                    })
            })
        })
}

fn codex_hook_command_matches(
    command: &str,
    bridge_path: &Path,
    transport_endpoint: &Path,
    event_name: &str,
) -> bool {
    let bridge = bridge_path.to_string_lossy();
    let endpoint = transport_endpoint.to_string_lossy();
    command.contains("yorling-bridge")
        && command.contains(bridge.as_ref())
        && command.contains("--source codex")
        && command.contains(super::transport_flag())
        && command.contains(endpoint.as_ref())
        && command.contains(&format!("--event {event_name}"))
}

fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
    let cwd = body.get("cwd").and_then(|c| c.as_str()).map(String::from);
    if cwd.is_some() {
        Some(TerminalContext {
            pid: body.get("pid").and_then(|p| p.as_u64()).map(|p| p as u32),
            tty: body
                .get("terminal_tty")
                .or_else(|| body.get("terminalTty"))
                .or_else(|| body.get("tty"))
                .and_then(|t| t.as_str())
                .map(String::from),
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
                .or_else(|| body.get("pane_uuid"))
                .or_else(|| body.get("paneUuid"))
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
                .or_else(|| body.get("pane_uuid"))
                .or_else(|| body.get("paneUuid"))
                .and_then(|t| t.as_str())
                .map(String::from),
        })
    } else {
        None
    }
}

fn extract_prompt(body: &Value) -> Option<&str> {
    body.get("prompt")
        .or_else(|| body.get("message"))
        .and_then(|value| value.as_str())
}

fn is_codex_internal_memory_task(body: &Value) -> bool {
    let cwd = body
        .get("cwd")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let prompt = extract_prompt(body).unwrap_or_default();

    is_codex_internal_memory_cwd(cwd)
        || (prompt.contains("Memory Writing Agent")
            && (prompt.contains("consolidate raw memories") || prompt.contains("rollout")))
}

fn is_codex_internal_memory_cwd(cwd: &str) -> bool {
    let normalized = cwd.replace('\\', "/");
    normalized.ends_with("/.codex/memories") || normalized.contains("/.codex/memories/")
}

fn codex_permission_request_id(
    body: &Value,
    session_id: &str,
    tool: &str,
    timestamp: i64,
) -> String {
    for key in [
        "request_id",
        "requestId",
        "tool_use_id",
        "toolUseId",
        "call_id",
        "callId",
        "id",
    ] {
        if let Some(value) = body.get(key).and_then(|value| value.as_str())
            && !value.trim().is_empty()
        {
            return value.to_string();
        }
    }

    if let Some(turn_id) = body
        .get("turn_id")
        .or_else(|| body.get("turnId"))
        .and_then(|value| value.as_str())
        && !turn_id.trim().is_empty()
    {
        return format!("codex-permission:{session_id}:{turn_id}:{tool}");
    }

    format!("codex-permission:{session_id}:{tool}:{timestamp}")
}

#[cfg(not(windows))]
fn shell_quote_command(command: &str) -> String {
    format!("'{}'", command.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn hook_command_uses_codex_source() {
        let cmd = CodexProvider::build_event_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
            "SessionStart",
        );
        assert!(cmd.contains("--source codex"));
    }

    #[cfg(not(windows))]
    #[test]
    fn event_hook_command_suppresses_stderr() {
        let cmd = CodexProvider::build_event_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
            "PostToolUse",
        );
        assert!(cmd.contains("2>/dev/null"));
        assert!(cmd.contains("--event PostToolUse"));
    }

    #[test]
    fn codex_hook_verify_matches_shell_wrapped_event_commands() {
        let bridge_path = Path::new("/tmp/Yorling.app/Contents/MacOS/yorling-bridge");
        let endpoint = Path::new("/tmp/Application Support/yorling/island.sock");
        let entry = serde_json::json!({
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": CodexProvider::build_event_hook_command(
                    bridge_path,
                    endpoint,
                    "PermissionRequest",
                ),
            }],
        });

        assert!(codex_hook_entry_matches(
            &entry,
            bridge_path,
            endpoint,
            "PermissionRequest",
        ));
        assert!(!codex_hook_entry_matches(
            &entry,
            bridge_path,
            endpoint,
            "PostToolUse",
        ));
    }

    #[test]
    fn codex_deny_produces_permission_request_response() {
        let provider = CodexProvider::new();
        let payload = provider
            .encode_permission_response(Decision::Deny)
            .expect("deny payload");
        let json: Value = serde_json::from_slice(&payload).expect("valid json");
        assert_eq!(
            json.pointer("/hookSpecificOutput/hookEventName")
                .and_then(|v| v.as_str()),
            Some("PermissionRequest")
        );
        assert_eq!(
            json.pointer("/hookSpecificOutput/decision/behavior")
                .and_then(|v| v.as_str()),
            Some("deny")
        );
    }

    #[test]
    fn codex_allow_produces_permission_request_response() {
        let provider = CodexProvider::new();
        let payload = provider
            .encode_permission_response(Decision::Allow)
            .expect("allow payload");
        let json: Value = serde_json::from_slice(&payload).expect("valid json");
        assert_eq!(
            json.pointer("/hookSpecificOutput/decision/behavior")
                .and_then(|v| v.as_str()),
            Some("allow")
        );
    }

    #[test]
    fn codex_permission_request_becomes_pending_permission() {
        let provider = CodexProvider::new();
        let raw = RawHookPayload {
            hook_event: "PermissionRequest".into(),
            body: serde_json::json!({
                "session_id": "codex-session",
                "turn_id": "turn-1",
                "tool_name": "exec_command",
                "tool_input": { "command": "pnpm build" },
            }),
            source: "codex".into(),
            session_id: Some("codex-session".into()),
        };

        let event = provider.normalize_event(&raw).expect("permission request");
        assert!(matches!(
            event.event_type,
            IslandEventType::PermissionRequest { ref tool, ref input, ref request_id }
                if tool == "exec_command"
                    && input.get("command").and_then(|v| v.as_str()) == Some("pnpm build")
                    && request_id.contains("turn-1")
        ));
    }

    #[test]
    fn codex_title_generation_prompt_discards_auxiliary_session() {
        let provider = CodexProvider::new();
        let raw = RawHookPayload {
            hook_event: "UserPromptSubmit".into(),
            body: serde_json::json!({
                "prompt": "You are a helpful assistant. You will be presented with a user prompt, and your job is to provide a short title for a task that will be created from that prompt.\nGenerate a concise UI title (18-36 characters) for this task.\nReturn only the title. No quotes or trailing punctuation.",
            }),
            source: "codex".into(),
            session_id: Some("title-helper".into()),
        };

        let event = provider
            .normalize_event(&raw)
            .expect("auxiliary session discard");
        assert_eq!(event.session_id, "title-helper");
        assert!(matches!(event.event_type, IslandEventType::SessionDiscard));
    }

    #[test]
    fn codex_skips_followup_events_for_ignored_auxiliary_session() {
        let provider = CodexProvider::new();
        let prompt = RawHookPayload {
            hook_event: "UserPromptSubmit".into(),
            body: serde_json::json!({
                "prompt": "You are a helpful assistant. You will be presented with a user prompt, and your job is to provide a short title for a task that will be created from that prompt.\nGenerate a concise UI title (18-36 characters) for this task.\nReturn only the title. No quotes or trailing punctuation.",
            }),
            source: "codex".into(),
            session_id: Some("title-helper".into()),
        };
        let stop = RawHookPayload {
            hook_event: "Stop".into(),
            body: serde_json::json!({}),
            source: "codex".into(),
            session_id: Some("title-helper".into()),
        };

        let prompt_event = provider
            .normalize_event(&prompt)
            .expect("prompt should discard auxiliary session");
        assert!(matches!(
            prompt_event.event_type,
            IslandEventType::SessionDiscard
        ));
        let error = provider
            .normalize_event(&stop)
            .expect_err("stop should skip");
        assert!(error.downcast_ref::<SkipHookEvent>().is_some());
    }

    #[test]
    fn codex_internal_memory_task_discards_auxiliary_session() {
        let provider = CodexProvider::new();
        let raw = RawHookPayload {
            hook_event: "UserPromptSubmit".into(),
            body: serde_json::json!({
                "cwd": "/Users/example/.codex/memories",
                "prompt": "## Memory Writing Agent: Phase 2 (Consolidation)\nYour job: consolidate raw memories and rollout summaries.",
            }),
            source: "codex".into(),
            session_id: Some("memory-writer".into()),
        };

        let event = provider
            .normalize_event(&raw)
            .expect("memory helper discard");
        assert_eq!(event.session_id, "memory-writer");
        assert!(matches!(event.event_type, IslandEventType::SessionDiscard));
    }

    #[test]
    fn codex_normal_user_prompt_is_not_skipped() {
        let provider = CodexProvider::new();
        let raw = RawHookPayload {
            hook_event: "UserPromptSubmit".into(),
            body: serde_json::json!({
                "prompt": "帮我分析一下灵动岛 SessionCard 的路径展示逻辑",
            }),
            source: "codex".into(),
            session_id: Some("real-task".into()),
        };

        let event = provider.normalize_event(&raw).expect("should normalize");
        assert!(matches!(
            event.event_type,
            IslandEventType::UserPrompt { ref text } if text.contains("SessionCard")
        ));
    }
}
