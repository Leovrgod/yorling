use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

pub struct GeminiProvider;

impl GeminiProvider {
    pub fn new() -> Self {
        Self
    }

    fn settings_path() -> anyhow::Result<std::path::PathBuf> {
        let settings_override = std::env::var("GEMINI_CLI_SYSTEM_SETTINGS_PATH")
            .ok()
            .map(std::path::PathBuf::from);
        if let Some(p) = settings_override {
            return Ok(p);
        }
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".gemini").join("settings.json"))
    }

    fn build_hook_command(bridge_path: &Path, transport_endpoint: &Path) -> String {
        format!(
            "{} --source gemini {} {}",
            super::shell_quote(bridge_path),
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
        )
    }
}

#[async_trait]
impl AgentProvider for GeminiProvider {
    fn id(&self) -> &str {
        "gemini"
    }

    fn display_name(&self) -> &str {
        "Gemini CLI"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let settings_path = Self::settings_path()?;
        let hook_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        let mut settings: Value = if settings_path.exists() {
            let content = tokio::fs::read_to_string(&settings_path).await?;
            serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        let hooks_map = settings
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Settings is not an object"))?
            .entry("hooks")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("hooks is not an object"))?;

        // Gemini "nested" format with millisecond timeouts
        let events = [
            ("SessionStart", None, 5000),
            ("SessionEnd", None, 5000),
            ("BeforeTool", Some(""), 5000),
            ("AfterTool", Some(""), 5000),
            ("BeforeAgent", None, 5000),
            ("AfterAgent", None, 5000),
            ("Notification", None, 5000),
        ];

        for (event_name, matcher, timeout) in events {
            let full_cmd = format!("{hook_cmd} --event {event_name}");
            let command_hook = serde_json::json!({
                "type": "command",
                "command": full_cmd,
                "name": "Yorling Island",
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
        if let Some(parent) = settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&settings_path, json_str).await?;

        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let settings_path = Self::settings_path()?;
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
                        arr.retain(|entry| !is_yorling_entry(entry));
                        return !arr.is_empty();
                    }
                    true
                });
                should_remove_hooks = hooks_map.is_empty();
            }
        }

        if should_remove_hooks {
            if let Some(obj) = settings.as_object_mut() {
                obj.remove("hooks");
            }
        }

        let json_str = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&settings_path, json_str).await?;

        Ok(())
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let settings_path = match Self::settings_path() {
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
                    reason: "Cannot read settings.json".into(),
                };
            }
        };

        let settings: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => {
                return HookStatus::Broken {
                    reason: "Invalid JSON in settings.json".into(),
                };
            }
        };

        let expected_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        let Some(hooks) = settings.get("hooks").and_then(|h| h.as_object()) else {
            return HookStatus::NotInstalled;
        };

        let required = ["SessionStart", "BeforeTool", "AfterTool", "Notification"];
        let found = required
            .iter()
            .filter(|event| {
                hooks
                    .get(**event)
                    .and_then(|e| e.as_array())
                    .is_some_and(|arr| {
                        arr.iter().any(|entry| {
                            entry
                                .pointer("/hooks/0/command")
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
            "SessionStart" => IslandEventType::SessionStart,
            "SessionEnd" => IslandEventType::SessionEnd,
            "BeforeTool" => {
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
            "AfterTool" => {
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
            "BeforeAgent" => {
                let text = raw
                    .body
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::UserPrompt { text }
            }
            "AfterAgent" => {
                let text = raw
                    .body
                    .get("prompt_response")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::AgentResponse { text }
            }
            "Notification" => {
                let title = raw
                    .body
                    .get("notification_type")
                    .or_else(|| raw.body.get("title"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string();
                let body = raw
                    .body
                    .get("message")
                    .or_else(|| raw.body.get("details"))
                    .and_then(|b| b.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::Notification { title, body }
            }
            other => {
                anyhow::bail!("Unknown Gemini hook event: {}", other);
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
        // Gemini hooks are mostly fire-and-forget
        false
    }

    fn encode_permission_response(&self, decision: Decision) -> anyhow::Result<Vec<u8>> {
        match decision {
            Decision::Allow | Decision::AllowAlways => Ok(Vec::new()),
            Decision::Deny => Ok(serde_json::to_vec(&serde_json::json!({
                "decision": "deny",
                "reason": "Denied by Yorling"
            }))?),
        }
    }

    fn encode_question_response(
        &self,
        _answer: &str,
        _from_permission: bool,
        _answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    fn config_paths(&self) -> Vec<std::path::PathBuf> {
        Self::settings_path().into_iter().collect()
    }

    fn detection_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        if let Ok(settings_override) = std::env::var("GEMINI_CLI_SYSTEM_SETTINGS_PATH") {
            let settings_path = std::path::PathBuf::from(settings_override);
            paths.push(settings_path.clone());
            if let Some(parent) = settings_path.parent() {
                paths.push(parent.to_path_buf());
            }
            return paths;
        }

        if let Some(home) = dirs::home_dir() {
            let root = home.join(".gemini");
            paths.push(root.clone());
            paths.push(root.join("settings.json"));
        }

        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["gemini"]
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

fn extract_terminal_context(body: &Value) -> Option<TerminalContext> {
    let cwd = body.get("cwd").and_then(|c| c.as_str()).map(String::from);
    if cwd.is_some() {
        Some(TerminalContext {
            pid: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn hook_command_uses_gemini_source() {
        let cmd = GeminiProvider::build_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
        );
        assert!(cmd.contains("--source gemini"));
    }

    #[test]
    fn gemini_does_not_support_blocking_permissions() {
        let provider = GeminiProvider::new();
        assert!(!provider.supports_blocking_permission());
    }
}
