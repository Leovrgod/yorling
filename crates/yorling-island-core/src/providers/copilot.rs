use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

pub struct CopilotProvider;

impl CopilotProvider {
    pub fn new() -> Self {
        Self
    }

    fn copilot_home() -> anyhow::Result<std::path::PathBuf> {
        if let Some(dir) = std::env::var_os("COPILOT_HOME") {
            return Ok(std::path::PathBuf::from(dir));
        }

        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".copilot"))
    }

    fn config_json_path() -> anyhow::Result<std::path::PathBuf> {
        Ok(Self::copilot_home()?.join("config.json"))
    }

    fn hooks_dir_path() -> anyhow::Result<std::path::PathBuf> {
        Ok(Self::copilot_home()?.join("hooks"))
    }

    fn managed_hooks_path() -> anyhow::Result<std::path::PathBuf> {
        Ok(Self::hooks_dir_path()?.join("yorling-island.json"))
    }

    fn legacy_hooks_json_path() -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|home| home.join(".github").join("hooks").join("island.json"))
    }

    fn build_hook_command(bridge_path: &Path, transport_endpoint: &Path) -> String {
        format!(
            "{} --source copilot {} {}",
            super::shell_quote(bridge_path),
            super::transport_flag(),
            super::shell_quote(transport_endpoint),
        )
    }

    fn hook_command_field() -> &'static str {
        #[cfg(windows)]
        {
            "windows"
        }

        #[cfg(not(windows))]
        {
            "bash"
        }
    }

    fn managed_hooks_json(bridge_path: &Path, transport_endpoint: &Path) -> Value {
        let hook_cmd = Self::build_hook_command(bridge_path, transport_endpoint);
        let hooks = HOOK_EVENTS
            .iter()
            .map(|event_name| {
                let mut command_hook = serde_json::Map::new();
                command_hook.insert("type".into(), Value::String("command".into()));
                command_hook.insert(
                    Self::hook_command_field().into(),
                    Value::String(format!("{hook_cmd} --event {event_name}")),
                );

                (
                    (*event_name).to_string(),
                    Value::Array(vec![Value::Object(command_hook)]),
                )
            })
            .collect::<serde_json::Map<String, Value>>();

        serde_json::json!({
            "version": 1,
            "hooks": hooks,
        })
    }
}

const HOOK_EVENTS: &[&str] = &[
    "sessionStart",
    "sessionEnd",
    "userPromptSubmitted",
    "preToolUse",
    "postToolUse",
    "agentStop",
    "subagentStop",
    "errorOccurred",
];

#[async_trait]
impl AgentProvider for CopilotProvider {
    fn id(&self) -> &str {
        "copilot"
    }

    fn display_name(&self) -> &str {
        "GitHub Copilot"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let hooks_path = Self::managed_hooks_path()?;
        if let Some(parent) = hooks_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let json_str = serde_json::to_string_pretty(&Self::managed_hooks_json(
            bridge_path,
            transport_endpoint,
        ))?;
        tokio::fs::write(&hooks_path, json_str).await?;

        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let hooks_path = Self::managed_hooks_path()?;
        if !hooks_path.exists() {
            return Ok(());
        }

        tokio::fs::remove_file(&hooks_path).await?;
        Ok(())
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let hooks_path = match Self::managed_hooks_path() {
            Ok(p) => p,
            Err(_) => return HookStatus::NotInstalled,
        };
        let expected_cmd = Self::build_hook_command(bridge_path, transport_endpoint);

        if hooks_path.exists() {
            let content = match tokio::fs::read_to_string(&hooks_path).await {
                Ok(c) => c,
                Err(_) => {
                    return HookStatus::Broken {
                        reason: "Cannot read Copilot hooks file".into(),
                    };
                }
            };

            let root: Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => {
                    return HookStatus::Broken {
                        reason: "Invalid JSON in Copilot hooks file".into(),
                    };
                }
            };

            let Some(hooks) = root.get("hooks").and_then(|h| h.as_object()) else {
                return HookStatus::Broken {
                    reason: "Copilot hooks file is missing a hooks object".into(),
                };
            };

            let found = count_expected_hooks(hooks, &expected_cmd);
            if found == 0 {
                return HookStatus::Broken {
                    reason: format!(
                        "Managed Copilot hooks file {} is not owned by Yorling",
                        hooks_path.display()
                    ),
                };
            }
            if found < REQUIRED_EVENTS.len() {
                return HookStatus::Outdated;
            }
            return HookStatus::Installed;
        }

        if legacy_install_detected(&expected_cmd).await {
            HookStatus::Outdated
        } else {
            HookStatus::NotInstalled
        }
    }

    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
        let session_id = extract_session_id(raw);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let terminal_context = extract_terminal_context(&raw.body);

        let event_type = match raw.hook_event.as_str() {
            "sessionStart" => IslandEventType::SessionStart,
            "sessionEnd" => IslandEventType::SessionEnd,
            "userPromptSubmitted" => {
                let text = raw
                    .body
                    .get("prompt")
                    .or_else(|| raw.body.get("message"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::UserPrompt { text }
            }
            "preToolUse" => {
                let tool = extract_tool_name(&raw.body);
                let input = extract_tool_input(&raw.body);
                IslandEventType::ToolUseStart { tool, input }
            }
            "postToolUse" => {
                let tool = extract_tool_name(&raw.body);
                let success = extract_tool_success(&raw.body);
                IslandEventType::ToolUseEnd { tool, success }
            }
            "agentStop" => IslandEventType::Stop,
            "subagentStop" => {
                let subagent_id = raw
                    .body
                    .get("subagent_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                IslandEventType::SubagentStop { subagent_id }
            }
            "errorOccurred" => {
                let title = raw
                    .body
                    .get("error_type")
                    .or_else(|| raw.body.get("title"))
                    .and_then(|t| t.as_str())
                    .unwrap_or("Error")
                    .to_string();
                let body = raw
                    .body
                    .get("error_message")
                    .or_else(|| raw.body.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                IslandEventType::Notification { title, body }
            }
            other => {
                anyhow::bail!("Unknown Copilot hook event: {}", other);
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
        false
    }

    fn encode_permission_response(&self, _decision: Decision) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
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
        let mut paths = Self::managed_hooks_path().into_iter().collect::<Vec<_>>();
        if let Some(legacy_path) = Self::legacy_hooks_json_path() {
            if paths.iter().all(|path| path != &legacy_path) {
                paths.push(legacy_path);
            }
        }
        paths
    }

    fn detection_paths(&self) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();

        if let Ok(root) = Self::copilot_home() {
            paths.push(root.clone());
            paths.push(root.join("hooks"));
            paths.push(root.join("hooks").join("yorling-island.json"));
            paths.push(root.join("config.json"));
        }

        if let Some(legacy_path) = Self::legacy_hooks_json_path() {
            paths.push(legacy_path);
        }

        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["copilot"]
    }
}

fn is_yorling_entry(entry: &Value) -> bool {
    extract_hook_command(entry).is_some_and(|command| command.contains("yorling-bridge"))
}

const REQUIRED_EVENTS: &[&str] = &[
    "sessionStart",
    "userPromptSubmitted",
    "preToolUse",
    "agentStop",
];

fn extract_hook_command(entry: &Value) -> Option<&str> {
    entry
        .get("bash")
        .or_else(|| entry.get("command"))
        .or_else(|| entry.get("osx"))
        .or_else(|| entry.get("linux"))
        .or_else(|| entry.get("windows"))
        .and_then(|value| value.as_str())
}

fn count_expected_hooks(hooks: &serde_json::Map<String, Value>, expected_cmd: &str) -> usize {
    REQUIRED_EVENTS
        .iter()
        .filter(|event| {
            hooks
                .get(**event)
                .and_then(|value| value.as_array())
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        extract_hook_command(entry)
                            .is_some_and(|command| command.contains(expected_cmd))
                    })
                })
        })
        .count()
}

async fn legacy_install_detected(expected_cmd: &str) -> bool {
    if let Ok(config_path) = CopilotProvider::config_json_path()
        && config_path.exists()
        && let Ok(content) = tokio::fs::read_to_string(&config_path).await
        && let Ok(root) = serde_json::from_str::<Value>(&content)
        && let Some(hooks) = root.get("hooks").and_then(|value| value.as_object())
        && (count_expected_hooks(hooks, expected_cmd) > 0
            || hooks.values().any(|value| {
                value
                    .as_array()
                    .is_some_and(|entries| entries.iter().any(is_yorling_entry))
            }))
    {
        return true;
    }

    if let Some(legacy_path) = CopilotProvider::legacy_hooks_json_path()
        && legacy_path.exists()
        && let Ok(content) = tokio::fs::read_to_string(&legacy_path).await
    {
        return content.contains("yorling-bridge");
    }

    false
}

fn extract_session_id(raw: &RawHookPayload) -> String {
    raw.session_id
        .clone()
        .or_else(|| {
            get_first_string(
                &raw.body,
                &[
                    "session_id",
                    "sessionId",
                    "conversation_id",
                    "conversationId",
                ],
            )
            .map(str::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

fn extract_tool_name(body: &Value) -> String {
    get_first_string(body, &["tool_name", "toolName", "tool", "name"])
        .unwrap_or("unknown")
        .to_string()
}

fn extract_tool_input(body: &Value) -> Value {
    if let Some(input) = body
        .get("tool_input")
        .or_else(|| body.get("input"))
        .cloned()
    {
        return input;
    }

    match body.get("toolArgs") {
        Some(Value::String(raw_args)) => {
            serde_json::from_str(raw_args).unwrap_or_else(|_| Value::String(raw_args.clone()))
        }
        Some(value) => value.clone(),
        None => serde_json::json!({}),
    }
}

fn extract_tool_success(body: &Value) -> bool {
    if let Some(result_type) = body
        .get("toolResult")
        .and_then(|result| result.get("resultType"))
        .and_then(|value| value.as_str())
    {
        return matches!(result_type, "success");
    }

    body.get("status")
        .and_then(|s| s.as_str())
        .is_none_or(|status| status != "error")
}

fn get_first_string<'a>(body: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| body.get(*key).and_then(|value| value.as_str()))
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
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_copilot_home(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "yorling-copilot-provider-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn hook_command_uses_copilot_source() {
        let cmd = CopilotProvider::build_hook_command(
            Path::new("/tmp/yorling-bridge"),
            Path::new("/tmp/island.sock"),
        );
        assert!(cmd.contains("--source copilot"));
    }

    #[test]
    fn normalize_session_start() {
        let provider = CopilotProvider::new();
        let raw = RawHookPayload {
            hook_event: "sessionStart".to_string(),
            body: serde_json::json!({}),
            source: "copilot".to_string(),
            session_id: Some("test-session".to_string()),
        };
        let event = provider.normalize_event(&raw).expect("should normalize");
        assert!(matches!(event.event_type, IslandEventType::SessionStart));
        assert_eq!(event.session_id, "test-session");
    }

    #[test]
    fn normalize_error_to_notification() {
        let provider = CopilotProvider::new();
        let raw = RawHookPayload {
            hook_event: "errorOccurred".to_string(),
            body: serde_json::json!({
                "error_type": "RateLimit",
                "error_message": "Too many requests"
            }),
            source: "copilot".to_string(),
            session_id: Some("s1".to_string()),
        };
        let event = provider.normalize_event(&raw).expect("should normalize");
        match &event.event_type {
            IslandEventType::Notification { title, body } => {
                assert_eq!(title, "RateLimit");
                assert_eq!(body, "Too many requests");
            }
            _ => panic!("Expected Notification"),
        }
    }

    #[test]
    fn yorling_entry_detection() {
        let entry = serde_json::json!({
            "type": "command",
            "bash": "/path/to/yorling-bridge --source copilot"
        });
        assert!(is_yorling_entry(&entry));

        let other = serde_json::json!({
            "type": "command",
            "bash": "/path/to/other-tool"
        });
        assert!(!is_yorling_entry(&other));
    }

    #[test]
    fn normalize_pre_tool_use_with_official_camel_case_fields() {
        let provider = CopilotProvider::new();
        let raw = RawHookPayload {
            hook_event: "preToolUse".to_string(),
            body: serde_json::json!({
                "sessionId": "session-42",
                "toolName": "bash",
                "toolArgs": "{\"command\":\"npm test\",\"description\":\"Run tests\"}"
            }),
            source: "copilot".to_string(),
            session_id: None,
        };

        let event = provider.normalize_event(&raw).expect("should normalize");
        assert_eq!(event.session_id, "session-42");
        match event.event_type {
            IslandEventType::ToolUseStart { tool, input } => {
                assert_eq!(tool, "bash");
                assert_eq!(
                    input.get("command").and_then(|v| v.as_str()),
                    Some("npm test")
                );
            }
            _ => panic!("Expected ToolUseStart"),
        }
    }

    #[test]
    fn normalize_post_tool_use_success_from_tool_result() {
        let provider = CopilotProvider::new();
        let raw = RawHookPayload {
            hook_event: "postToolUse".to_string(),
            body: serde_json::json!({
                "toolName": "view",
                "toolResult": {
                    "resultType": "failure"
                }
            }),
            source: "copilot".to_string(),
            session_id: Some("s1".to_string()),
        };

        let event = provider.normalize_event(&raw).expect("should normalize");
        match event.event_type {
            IslandEventType::ToolUseEnd { tool, success } => {
                assert_eq!(tool, "view");
                assert!(!success);
            }
            _ => panic!("Expected ToolUseEnd"),
        }
    }

    #[test]
    fn config_paths_include_managed_hook_file_and_legacy_path_for_diagnostics() {
        let provider = CopilotProvider::new();
        let paths = provider.config_paths();

        assert!(!paths.is_empty());
        assert!(paths.iter().any(|path| {
            path.to_string_lossy()
                .ends_with(".copilot/hooks/yorling-island.json")
        }));
    }

    #[tokio::test]
    async fn installs_copilot_hooks_into_dedicated_hooks_file() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_copilot_home("install");
        std::fs::create_dir_all(&home).expect("create temp home");
        unsafe {
            std::env::set_var("COPILOT_HOME", &home);
        }

        let provider = CopilotProvider::new();
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await
            .expect("install hooks");

        let hooks_path = home.join("hooks").join("yorling-island.json");
        assert!(hooks_path.is_file(), "expected dedicated hooks file");

        let contents = std::fs::read_to_string(&hooks_path).expect("read hooks file");
        assert!(contents.contains("\"version\": 1"));
        assert!(contents.contains("\"sessionStart\""));
        assert!(contents.contains("\"preToolUse\""));
        assert!(
            !home.join("config.json").exists(),
            "should not create config.json"
        );

        unsafe {
            std::env::remove_var("COPILOT_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn verify_hooks_reads_dedicated_hooks_file_and_config_paths_point_to_hooks_file() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_copilot_home("verify");
        let hooks_dir = home.join("hooks");
        std::fs::create_dir_all(&hooks_dir).expect("create hooks dir");
        unsafe {
            std::env::set_var("COPILOT_HOME", &home);
        }

        let provider = CopilotProvider::new();
        let bridge_path = Path::new("/tmp/yorling-bridge");
        let socket_path = Path::new("/tmp/island.sock");
        let expected_cmd = CopilotProvider::build_hook_command(bridge_path, socket_path);
        let hook_file = hooks_dir.join("yorling-island.json");
        std::fs::write(
            &hook_file,
            format!(
                r#"{{
  "version": 1,
  "hooks": {{
    "sessionStart": [{{ "type": "command", "bash": "{expected_cmd} --event sessionStart" }}],
    "userPromptSubmitted": [{{ "type": "command", "bash": "{expected_cmd} --event userPromptSubmitted" }}],
    "preToolUse": [{{ "type": "command", "bash": "{expected_cmd} --event preToolUse" }}],
    "agentStop": [{{ "type": "command", "bash": "{expected_cmd} --event agentStop" }}]
  }}
}}"#
            ),
        )
        .expect("write hook file");

        let status = provider.verify_hooks(bridge_path, socket_path).await;
        assert_eq!(status, HookStatus::Installed);

        let paths = provider.config_paths();
        assert!(paths.iter().any(|path| path == &hook_file));
        assert!(
            paths
                .iter()
                .all(|path| !path.to_string_lossy().ends_with(".copilot/config.json"))
        );

        unsafe {
            std::env::remove_var("COPILOT_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
