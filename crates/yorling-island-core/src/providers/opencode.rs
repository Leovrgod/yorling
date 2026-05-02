use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::event::{Decision, IslandEvent, RawHookPayload};
use crate::provider::{AgentProvider, HookStatus};

use super::claude_compat::normalize_claude_compatible_event;

const MANAGED_MARKER: &str = "Yorling managed integration: opencode";
const PLUGIN_FILE_NAME: &str = "yorling-island.js";
const LEGACY_PLUGIN_FILE_NAME: &str = concat!("ping", "-island.js");

pub struct OpenCodeProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedFileOwnership {
    Missing,
    YorlingManaged,
    Foreign,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self
    }

    fn opencode_root() -> anyhow::Result<PathBuf> {
        if let Some(path) = std::env::var_os("OPENCODE_HOME") {
            return Ok(PathBuf::from(path));
        }

        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".config").join("opencode"))
    }

    fn plugin_dir() -> anyhow::Result<PathBuf> {
        Ok(Self::opencode_root()?.join("plugins"))
    }

    fn plugin_file_path_for(file_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::plugin_dir()?.join(file_name))
    }

    fn plugin_file_path() -> anyhow::Result<PathBuf> {
        Self::plugin_file_path_for(PLUGIN_FILE_NAME)
    }

    fn legacy_plugin_file_path() -> anyhow::Result<PathBuf> {
        Self::plugin_file_path_for(LEGACY_PLUGIN_FILE_NAME)
    }

    fn config_json_path() -> anyhow::Result<PathBuf> {
        Ok(Self::opencode_root()?.join("config.json"))
    }

    fn bridge_base_args(bridge_path: &Path, transport_endpoint: &Path) -> Vec<String> {
        vec![
            bridge_path.display().to_string(),
            "--source".into(),
            "opencode".into(),
            super::transport_flag().into(),
            transport_endpoint.display().to_string(),
        ]
    }

    fn render_plugin_source(bridge_path: &Path, transport_endpoint: &Path) -> String {
        let args_json =
            serde_json::to_string(&Self::bridge_base_args(bridge_path, transport_endpoint))
                .expect("bridge args json");
        include_str!("assets/opencode_plugin.js")
            .replace("__YORLING_MARKER__", MANAGED_MARKER)
            .replace("__BRIDGE_BASE_ARGS_JSON__", &args_json)
    }

    async fn legacy_plugin_owned_by_yorling() -> anyhow::Result<bool> {
        Ok(matches!(
            managed_file_ownership(&Self::legacy_plugin_file_path()?).await?,
            ManagedFileOwnership::YorlingManaged
        ))
    }
}

#[async_trait]
impl AgentProvider for OpenCodeProvider {
    fn id(&self) -> &str {
        "opencode"
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let legacy_owned = Self::legacy_plugin_owned_by_yorling().await?;
        if legacy_owned {
            remove_file_if_exists(&Self::legacy_plugin_file_path()?).await?;
        }
        let plugin_file = Self::plugin_file_path()?;
        if let Some(parent) = plugin_file.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(
            &plugin_file,
            Self::render_plugin_source(bridge_path, transport_endpoint),
        )
        .await?;
        let mut extra_remove = Vec::new();
        if legacy_owned {
            extra_remove.extend(plugin_specifiers(&Self::legacy_plugin_file_path()?));
        }
        write_plugin_config(
            &Self::config_json_path()?,
            &plugin_file,
            true,
            &extra_remove,
        )
        .await
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let legacy_owned = Self::legacy_plugin_owned_by_yorling().await?;
        let plugin_file = Self::plugin_file_path()?;
        remove_file_if_exists(&plugin_file).await?;
        if legacy_owned {
            remove_file_if_exists(&Self::legacy_plugin_file_path()?).await?;
        }
        let mut extra_remove = Vec::new();
        if legacy_owned {
            extra_remove.extend(plugin_specifiers(&Self::legacy_plugin_file_path()?));
        }
        write_plugin_config(
            &Self::config_json_path()?,
            &plugin_file,
            false,
            &extra_remove,
        )
        .await
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let plugin_file = match Self::plugin_file_path() {
            Ok(path) => path,
            Err(_) => return HookStatus::NotInstalled,
        };
        let expected = Self::render_plugin_source(bridge_path, transport_endpoint);
        let legacy_owned = match Self::legacy_plugin_owned_by_yorling().await {
            Ok(owned) => owned,
            Err(error) => {
                return HookStatus::Broken {
                    reason: error.to_string(),
                };
            }
        };

        let file_status = if !plugin_file.exists() {
            HookStatus::Outdated
        } else {
            match tokio::fs::read_to_string(&plugin_file).await {
                Ok(contents) if contents == expected => HookStatus::Installed,
                Ok(contents)
                    if contents.contains(MANAGED_MARKER) || contents.contains("yorling-bridge") =>
                {
                    HookStatus::Outdated
                }
                Ok(_) => HookStatus::Broken {
                    reason: format!(
                        "Managed OpenCode plugin file {} is owned by another integration",
                        plugin_file.display()
                    ),
                },
                Err(error) => HookStatus::Broken {
                    reason: format!(
                        "Cannot read managed OpenCode plugin file {}: {}",
                        plugin_file.display(),
                        error
                    ),
                },
            }
        };

        if matches!(file_status, HookStatus::Broken { .. }) {
            return file_status;
        }

        let plugin_enabled = match is_plugin_enabled(Self::config_json_path(), &plugin_file).await {
            Ok(enabled) => enabled,
            Err(error) => {
                return HookStatus::Broken {
                    reason: error.to_string(),
                };
            }
        };
        let legacy_enabled = if legacy_owned {
            let legacy_plugin_file = match Self::legacy_plugin_file_path() {
                Ok(path) => path,
                Err(error) => {
                    return HookStatus::Broken {
                        reason: error.to_string(),
                    };
                }
            };
            match is_plugin_enabled(Self::config_json_path(), &legacy_plugin_file).await {
                Ok(enabled) => enabled,
                Err(error) => {
                    return HookStatus::Broken {
                        reason: error.to_string(),
                    };
                }
            }
        } else {
            false
        };

        if file_status == HookStatus::Installed && plugin_enabled {
            HookStatus::Installed
        } else if plugin_file.exists() || plugin_enabled || legacy_owned || legacy_enabled {
            HookStatus::Outdated
        } else {
            HookStatus::NotInstalled
        }
    }

    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
        normalize_claude_compatible_event(self.id(), self.display_name(), raw, &[], true)
    }

    fn supports_blocking_permission(&self) -> bool {
        true
    }

    fn encode_permission_response(&self, decision: Decision) -> anyhow::Result<Vec<u8>> {
        let behavior = match decision {
            Decision::Allow => "allow",
            Decision::AllowAlways => "always",
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
        _from_permission: bool,
        answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
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
    }

    fn config_paths(&self) -> Vec<PathBuf> {
        vec![Self::plugin_file_path(), Self::config_json_path()]
            .into_iter()
            .filter_map(Result::ok)
            .collect()
    }

    fn detection_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(root) = Self::opencode_root() {
            paths.push(root.clone());
            paths.push(root.join("config.json"));
            paths.push(root.join("plugins"));
            paths.push(root.join("plugins").join(PLUGIN_FILE_NAME));
            paths.push(root.join("plugins").join(LEGACY_PLUGIN_FILE_NAME));
        }
        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["opencode"]
    }
}

async fn write_plugin_config(
    path: &Path,
    plugin_file: &Path,
    installing: bool,
    extra_remove_specifiers: &[String],
) -> anyhow::Result<()> {
    let mut json = if path.exists() {
        let contents = tokio::fs::read_to_string(path).await?;
        serde_json::from_str::<serde_json::Value>(&contents)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let root = json
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("OpenCode config is not an object"))?;
    let plugin_specifier = plugin_specifiers(plugin_file);
    let existing = root
        .get("plugin")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let filtered = existing
        .into_iter()
        .filter(|entry| {
            entry.as_str().is_none_or(|value| {
                plugin_specifier.iter().all(|specifier| value != specifier)
                    && extra_remove_specifiers
                        .iter()
                        .all(|specifier| value != specifier)
            })
        })
        .collect::<Vec<_>>();

    if installing {
        let mut next = filtered;
        next.push(serde_json::json!(plugin_specifier[0]));
        root.insert("plugin".into(), serde_json::Value::Array(next));
    } else if filtered.is_empty() {
        root.remove("plugin");
    } else {
        root.insert("plugin".into(), serde_json::Value::Array(filtered));
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_string_pretty(&json)?).await?;
    Ok(())
}

async fn is_plugin_enabled(
    path: Result<PathBuf, anyhow::Error>,
    plugin_file: &Path,
) -> anyhow::Result<bool> {
    let path = path?;
    if !path.exists() {
        return Ok(false);
    }

    let contents = tokio::fs::read_to_string(&path).await?;
    let json: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
        anyhow::anyhow!(
            "Invalid JSON in OpenCode config {}: {}",
            path.display(),
            error
        )
    })?;
    let Some(entries) = json.get("plugin").and_then(|value| value.as_array()) else {
        return Ok(false);
    };
    let specifiers = plugin_specifiers(plugin_file);
    Ok(entries
        .iter()
        .filter_map(|entry| entry.as_str())
        .any(|value| specifiers.iter().any(|specifier| value == specifier)))
}

fn plugin_specifiers(plugin_file: &Path) -> [String; 2] {
    let file_path = plugin_file.display().to_string();
    let file_url = if cfg!(windows) {
        format!("file:///{}", file_path.replace('\\', "/"))
    } else {
        format!("file://{}", file_path)
    };
    [file_url, file_path]
}

async fn managed_file_ownership(path: &Path) -> anyhow::Result<ManagedFileOwnership> {
    if !path.exists() {
        return Ok(ManagedFileOwnership::Missing);
    }

    let contents = tokio::fs::read_to_string(path).await?;
    Ok(
        if contents.contains(MANAGED_MARKER) || contents.contains("yorling-bridge") {
            ManagedFileOwnership::YorlingManaged
        } else {
            ManagedFileOwnership::Foreign
        },
    )
}

async fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    use crate::event::{IslandEventType, RawHookPayload};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_opencode_home(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "yorling-opencode-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn installs_plugin_file_and_enables_it_in_config() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_opencode_home("install");
        std::fs::create_dir_all(&home).expect("create home");
        unsafe {
            std::env::set_var("OPENCODE_HOME", &home);
        }

        let provider = OpenCodeProvider::new();
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await
            .expect("install");

        let status = provider
            .verify_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await;
        assert_eq!(status, HookStatus::Installed);
        assert!(home.join("plugins").join(PLUGIN_FILE_NAME).exists());

        let config = std::fs::read_to_string(home.join("config.json")).expect("config");
        assert!(config.contains("file://"));
        assert!(config.contains(PLUGIN_FILE_NAME));
        assert!(!config.contains(LEGACY_PLUGIN_FILE_NAME));

        unsafe {
            std::env::remove_var("OPENCODE_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn install_migrates_legacy_yorling_plugin_file_name() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_opencode_home("migrate");
        let plugins_dir = home.join("plugins");
        std::fs::create_dir_all(&plugins_dir).expect("plugins dir");
        std::fs::write(
            plugins_dir.join(LEGACY_PLUGIN_FILE_NAME),
            format!("// {MANAGED_MARKER}\n// yorling-bridge\n"),
        )
        .expect("legacy plugin");
        std::fs::write(
            home.join("config.json"),
            serde_json::json!({
                "plugin": [
                    format!("file://{}", plugins_dir.join(LEGACY_PLUGIN_FILE_NAME).display())
                ]
            })
            .to_string(),
        )
        .expect("legacy config");
        unsafe {
            std::env::set_var("OPENCODE_HOME", &home);
        }

        let provider = OpenCodeProvider::new();
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await
            .expect("install");

        assert!(!plugins_dir.join(LEGACY_PLUGIN_FILE_NAME).exists());
        assert!(plugins_dir.join(PLUGIN_FILE_NAME).exists());

        let config = std::fs::read_to_string(home.join("config.json")).expect("config");
        assert!(config.contains(PLUGIN_FILE_NAME));
        assert!(!config.contains(LEGACY_PLUGIN_FILE_NAME));

        unsafe {
            std::env::remove_var("OPENCODE_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn pre_tool_question_becomes_question_event() {
        let provider = OpenCodeProvider::new();
        let raw = RawHookPayload {
            hook_event: "PreToolUse".into(),
            body: serde_json::json!({
                "session_id": "opencode-1",
                "tool_name": "AskUserQuestion",
                "tool_input": {
                    "questions": [{
                        "header": "color",
                        "question": "Pick a color",
                        "options": ["Red", "Blue"]
                    }]
                }
            }),
            source: "opencode".into(),
            session_id: Some("opencode-1".into()),
        };

        let event = provider.normalize_event(&raw).expect("normalize");
        match event.event_type {
            IslandEventType::AskQuestion {
                question,
                options,
                answer_key,
                ..
            } => {
                assert_eq!(question, "Pick a color");
                assert_eq!(options, vec!["Red".to_string(), "Blue".to_string()]);
                assert_eq!(answer_key.as_deref(), Some("color"));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
