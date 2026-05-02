use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

pub struct CursorProvider;

impl CursorProvider {
    pub fn new() -> Self {
        Self
    }

    fn hooks_json_path() -> anyhow::Result<std::path::PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".cursor").join("hooks.json"))
    }

    fn build_hook_command(bridge_path: &Path, transport_endpoint: &Path) -> String {
        format!(
            "{} --source cursor {} {}",
            super::shell_quote(bridge_path),
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
        )
    }
}

#[async_trait]
impl AgentProvider for CursorProvider {
    fn id(&self) -> &str {
        "cursor"
    }

    fn display_name(&self) -> &str {
        "Cursor"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let hooks_path = Self::hooks_json_path()?;
        let hook_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        let mut root: Value = if hooks_path.exists() {
            let content = tokio::fs::read_to_string(&hooks_path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let root_obj = root
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks.json is not an object"))?;

        // Ensure version field
        root_obj.entry("version").or_insert(serde_json::json!(1));

        let hooks_map = root_obj
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

        // Cursor "flat" format: { "eventName": [{ "command": "..." }] }
        // Note: camelCase event names
        let events = [
            "beforeSubmitPrompt",
            "beforeShellExecution",
            "afterShellExecution",
            "beforeReadFile",
            "afterFileEdit",
            "beforeMCPExecution",
            "afterMCPExecution",
            "afterAgentThought",
            "stop",
        ];

        for event_name in events {
            let full_cmd = format!("{hook_cmd} --event {event_name}");
            let entry = serde_json::json!({
                "command": full_cmd,
            });

            if let Some(existing) = hooks_map.get_mut(event_name) {
                if let Some(arr) = existing.as_array_mut() {
                    arr.retain(|e| !is_yorling_entry_flat(e));
                    arr.push(entry);
                }
            } else {
                hooks_map.insert(event_name.to_string(), Value::Array(vec![entry]));
            }
        }

        if let Some(parent) = hooks_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&root)?;
        tokio::fs::write(&hooks_path, json_str).await?;

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
                        arr.retain(|entry| !is_yorling_entry_flat(entry));
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

        let expected_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        let Some(hooks) = root.get("hooks").and_then(|h| h.as_object()) else {
            return HookStatus::NotInstalled;
        };

        // Cursor flat format: look for "command" directly on each entry
        let required = [
            "beforeSubmitPrompt",
            "beforeShellExecution",
            "afterFileEdit",
            "stop",
        ];
        let found = required
            .iter()
            .filter(|event| {
                hooks
                    .get(**event)
                    .and_then(|e| e.as_array())
                    .is_some_and(|arr| {
                        arr.iter().any(|entry| {
                            entry
                                .get("command")
                                .and_then(|c| c.as_str())
                                .is_some_and(|c| c.contains(&expected_cmd))
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
                    .get("conversation_id")
                    .or_else(|| raw.body.get("session_id"))
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
            "beforeSubmitPrompt" => {
                let text = raw
                    .body
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::UserPrompt { text }
            }
            "beforeShellExecution" => {
                let tool = "shell".to_string();
                let input = raw
                    .body
                    .get("command")
                    .map(|c| serde_json::json!({ "command": c }))
                    .unwrap_or(serde_json::json!({}));
                let request_id = raw
                    .body
                    .get("generation_id")
                    .and_then(|g| g.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{session_id}:{timestamp}"));
                IslandEventType::PermissionRequest {
                    tool,
                    input,
                    request_id,
                }
            }
            "afterShellExecution" => {
                let success = raw
                    .body
                    .get("status")
                    .and_then(|s| s.as_str())
                    .is_none_or(|s| s != "error");
                IslandEventType::ToolUseEnd {
                    tool: "shell".to_string(),
                    success,
                }
            }
            "beforeReadFile" => {
                let file_path = raw
                    .body
                    .get("file_path")
                    .and_then(|f| f.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                IslandEventType::ToolUseStart {
                    tool: "read_file".to_string(),
                    input: serde_json::json!({ "file_path": file_path }),
                }
            }
            "afterFileEdit" => IslandEventType::ToolUseEnd {
                tool: "file_edit".to_string(),
                success: true,
            },
            "beforeMCPExecution" => {
                let tool = raw
                    .body
                    .get("tool_name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("mcp_tool")
                    .to_string();
                let input = raw
                    .body
                    .get("tool_input")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));
                let request_id = raw
                    .body
                    .get("generation_id")
                    .and_then(|g| g.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{session_id}:{timestamp}"));
                IslandEventType::PermissionRequest {
                    tool,
                    input,
                    request_id,
                }
            }
            "afterMCPExecution" => {
                let tool = raw
                    .body
                    .get("tool_name")
                    .and_then(|t| t.as_str())
                    .unwrap_or("mcp_tool")
                    .to_string();
                let success = raw
                    .body
                    .get("status")
                    .and_then(|s| s.as_str())
                    .is_none_or(|s| s != "error");
                IslandEventType::ToolUseEnd { tool, success }
            }
            "afterAgentThought" => IslandEventType::AgentThinking,
            "stop" => IslandEventType::Stop,
            other => {
                anyhow::bail!("Unknown Cursor hook event: {}", other);
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
        let (cont, perm) = match decision {
            Decision::Allow | Decision::AllowAlways => (true, "allow"),
            Decision::Deny => (false, "deny"),
        };

        Ok(serde_json::to_vec(&serde_json::json!({
            "continue": cont,
            "permission": perm,
        }))?)
    }

    fn encode_question_response(
        &self,
        _answer: &str,
        _from_permission: bool,
        _answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        // Cursor does not support question responses through hooks
        Ok(Vec::new())
    }

    fn config_paths(&self) -> Vec<std::path::PathBuf> {
        Self::hooks_json_path().into_iter().collect()
    }

    fn detection_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        if let Some(home) = dirs::home_dir() {
            let root = home.join(".cursor");
            paths.push(root.clone());
            paths.push(root.join("hooks.json"));
        }
        paths
    }
}

/// Cursor flat format: entries have "command" directly (no nested "hooks" array)
fn is_yorling_entry_flat(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(|c| c.as_str())
        .is_some_and(|c| c.contains("yorling-bridge"))
}

fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
    let cwd = body
        .get("cwd")
        .or_else(|| {
            body.get("workspace_roots")
                .and_then(|w| w.as_array())
                .and_then(|arr| arr.first())
        })
        .and_then(|c| c.as_str())
        .map(String::from);

    if cwd.is_some() {
        Some(TerminalContext {
            pid: None,
            tty: None,
            cwd,
            terminal_app: Some("Cursor".to_string()),
            terminal_bundle_id: Some("com.todesktop.230313mzl4w4u92".to_string()),
            terminal_session_id: body
                .get("terminal_session_id")
                .or_else(|| body.get("terminalSessionId"))
                .and_then(|t| t.as_str())
                .map(String::from),
            pane_title: body
                .get("pane_title")
                .or_else(|| body.get("paneTitle"))
                .or_else(|| body.get("title"))
                .and_then(|t| t.as_str())
                .map(String::from),
            warp_pane_uuid: None,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn hook_command_uses_cursor_source() {
        let cmd = CursorProvider::build_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
        );
        assert!(cmd.contains("--source cursor"));
    }

    #[test]
    fn cursor_allow_encodes_continue_true() {
        let provider = CursorProvider::new();
        let payload = provider
            .encode_permission_response(Decision::Allow)
            .expect("allow payload");
        let json: Value = serde_json::from_slice(&payload).expect("valid json");
        assert_eq!(json.get("continue").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            json.get("permission").and_then(|v| v.as_str()),
            Some("allow")
        );
    }

    #[test]
    fn cursor_deny_encodes_continue_false() {
        let provider = CursorProvider::new();
        let payload = provider
            .encode_permission_response(Decision::Deny)
            .expect("deny payload");
        let json: Value = serde_json::from_slice(&payload).expect("valid json");
        assert_eq!(json.get("continue").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            json.get("permission").and_then(|v| v.as_str()),
            Some("deny")
        );
    }

    #[test]
    fn flat_entry_detection_works() {
        let entry = serde_json::json!({
            "command": "/path/to/yorling-bridge --source cursor"
        });
        assert!(is_yorling_entry_flat(&entry));

        let other = serde_json::json!({
            "command": "/path/to/other-tool"
        });
        assert!(!is_yorling_entry_flat(&other));
    }
}
