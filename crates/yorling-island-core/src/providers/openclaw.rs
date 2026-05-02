use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus};

const MANAGED_MARKER: &str = "Yorling managed integration: openclaw";
const HOOK_NAME: &str = "yorling-island-openclaw";
const LEGACY_HOOK_NAME: &str = concat!("ping", "-island-openclaw");
const HOOK_MD_NAME: &str = "HOOK.md";
const HANDLER_TS_NAME: &str = "handler.ts";

pub struct OpenClawProvider;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedInstallOwnership {
    NotPresent,
    YorlingManaged,
    Foreign,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ManagedFileOwnership {
    Missing,
    YorlingManaged,
    Foreign,
}

impl OpenClawProvider {
    pub fn new() -> Self {
        Self
    }

    fn openclaw_home() -> anyhow::Result<PathBuf> {
        if let Some(path) = std::env::var_os("OPENCLAW_HOME") {
            return Ok(PathBuf::from(path));
        }

        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".openclaw"))
    }

    fn hook_dir_for(hook_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::openclaw_home()?.join("hooks").join(hook_name))
    }

    fn hook_dir() -> anyhow::Result<PathBuf> {
        Self::hook_dir_for(HOOK_NAME)
    }

    fn legacy_hook_dir() -> anyhow::Result<PathBuf> {
        Self::hook_dir_for(LEGACY_HOOK_NAME)
    }

    fn hook_md_path_for(hook_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::hook_dir_for(hook_name)?.join(HOOK_MD_NAME))
    }

    fn hook_md_path() -> anyhow::Result<PathBuf> {
        Self::hook_md_path_for(HOOK_NAME)
    }

    fn legacy_hook_md_path() -> anyhow::Result<PathBuf> {
        Self::hook_md_path_for(LEGACY_HOOK_NAME)
    }

    fn handler_ts_path_for(hook_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::hook_dir_for(hook_name)?.join(HANDLER_TS_NAME))
    }

    fn handler_ts_path() -> anyhow::Result<PathBuf> {
        Self::handler_ts_path_for(HOOK_NAME)
    }

    fn legacy_handler_ts_path() -> anyhow::Result<PathBuf> {
        Self::handler_ts_path_for(LEGACY_HOOK_NAME)
    }

    fn activation_config_path() -> anyhow::Result<PathBuf> {
        Ok(Self::openclaw_home()?.join("openclaw.json"))
    }

    fn transcript_dir() -> anyhow::Result<PathBuf> {
        Ok(Self::openclaw_home()?
            .join("agents")
            .join("main")
            .join("sessions"))
    }

    fn bridge_base_args(bridge_path: &Path, transport_endpoint: &Path) -> Vec<String> {
        vec![
            bridge_path.display().to_string(),
            "--source".into(),
            "openclaw".into(),
            super::transport_flag().into(),
            transport_endpoint.display().to_string(),
        ]
    }

    fn render_hook_md() -> String {
        include_str!("assets/openclaw_hook.md").replace("__YORLING_MARKER__", MANAGED_MARKER)
    }

    fn render_handler_ts(bridge_path: &Path, transport_endpoint: &Path) -> String {
        let args_json =
            serde_json::to_string(&Self::bridge_base_args(bridge_path, transport_endpoint))
                .expect("bridge args json");
        include_str!("assets/openclaw_handler.ts")
            .replace("__YORLING_MARKER__", MANAGED_MARKER)
            .replace("__BRIDGE_BASE_ARGS_JSON__", &args_json)
    }

    async fn hook_install_ownership(hook_name: &str) -> anyhow::Result<ManagedInstallOwnership> {
        let hook_dir = Self::hook_dir_for(hook_name)?;
        if !hook_dir.exists() {
            return Ok(ManagedInstallOwnership::NotPresent);
        }

        let md = managed_file_ownership(&Self::hook_md_path_for(hook_name)?).await?;
        let handler = managed_file_ownership(&Self::handler_ts_path_for(hook_name)?).await?;
        Ok(install_ownership_from_files(
            hook_dir.exists(),
            [md, handler],
        ))
    }

    async fn legacy_install_ownership() -> anyhow::Result<ManagedInstallOwnership> {
        Self::hook_install_ownership(LEGACY_HOOK_NAME).await
    }

    async fn cleanup_legacy_hook_if_managed() -> anyhow::Result<bool> {
        if Self::legacy_install_ownership().await? != ManagedInstallOwnership::YorlingManaged {
            return Ok(false);
        }

        remove_file_if_exists(&Self::legacy_hook_md_path()?).await?;
        remove_file_if_exists(&Self::legacy_handler_ts_path()?).await?;
        remove_dir_if_empty(&Self::legacy_hook_dir()?).await?;
        Ok(true)
    }
}

#[async_trait]
impl AgentProvider for OpenClawProvider {
    fn id(&self) -> &str {
        "openclaw"
    }

    fn display_name(&self) -> &str {
        "OpenClaw"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let removed_legacy = Self::cleanup_legacy_hook_if_managed().await?;
        let hook_dir = Self::hook_dir()?;
        tokio::fs::create_dir_all(&hook_dir).await?;
        tokio::fs::write(Self::hook_md_path()?, Self::render_hook_md()).await?;
        tokio::fs::write(
            Self::handler_ts_path()?,
            Self::render_handler_ts(bridge_path, transport_endpoint),
        )
        .await?;
        let activation_path = Self::activation_config_path()?;
        write_activation_config(&activation_path, HOOK_NAME, true).await?;
        if removed_legacy {
            remove_activation_entry(&activation_path, LEGACY_HOOK_NAME).await?;
        }
        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let removed_legacy = Self::cleanup_legacy_hook_if_managed().await?;
        remove_file_if_exists(&Self::hook_md_path()?).await?;
        remove_file_if_exists(&Self::handler_ts_path()?).await?;
        remove_dir_if_empty(&Self::hook_dir()?).await?;
        let activation_path = Self::activation_config_path()?;
        remove_activation_entry(&activation_path, HOOK_NAME).await?;
        if removed_legacy {
            remove_activation_entry(&activation_path, LEGACY_HOOK_NAME).await?;
        }
        Ok(())
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let hook_dir = match Self::hook_dir() {
            Ok(path) => path,
            Err(_) => return HookStatus::NotInstalled,
        };
        let legacy_ownership = match Self::legacy_install_ownership().await {
            Ok(ownership) => ownership,
            Err(error) => {
                return HookStatus::Broken {
                    reason: error.to_string(),
                };
            }
        };

        let md_status = verify_managed_file(Self::hook_md_path(), &Self::render_hook_md()).await;
        let handler_status = verify_managed_file(
            Self::handler_ts_path(),
            &Self::render_handler_ts(bridge_path, transport_endpoint),
        )
        .await;

        let hooks_present = hook_dir.exists();
        let activation_state =
            match activation_entry_state(Self::activation_config_path(), HOOK_NAME).await {
                Ok(state) => state,
                Err(error)
                    if hooks_present
                        || matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged) =>
                {
                    return HookStatus::Broken {
                        reason: error.to_string(),
                    };
                }
                Err(_) => None,
            };
        let legacy_activation_state =
            if matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged) {
                activation_entry_state(Self::activation_config_path(), LEGACY_HOOK_NAME)
                    .await
                    .unwrap_or(None)
            } else {
                None
            };

        if !hooks_present
            && activation_state.is_none()
            && !matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged)
        {
            return HookStatus::NotInstalled;
        }

        if matches!(md_status, HookStatus::Broken { .. }) {
            return md_status;
        }
        if matches!(handler_status, HookStatus::Broken { .. }) {
            return handler_status;
        }

        if md_status == HookStatus::Installed
            && handler_status == HookStatus::Installed
            && activation_state == Some(true)
        {
            HookStatus::Installed
        } else if hooks_present
            || activation_state.is_some()
            || matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged)
            || legacy_activation_state.is_some()
        {
            HookStatus::Outdated
        } else {
            HookStatus::NotInstalled
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

        let timestamp = current_timestamp();
        let event_type = match raw.hook_event.as_str() {
            "SessionStart" => IslandEventType::SessionStart,
            "UserPromptSubmit" => IslandEventType::UserPrompt {
                text: raw
                    .body
                    .get("message")
                    .or_else(|| raw.body.get("prompt"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            },
            "AgentResponse" => IslandEventType::AgentResponse {
                text: raw
                    .body
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            },
            "PreCompact" => IslandEventType::ContextCompaction,
            "Stop" => IslandEventType::Stop,
            "Notification" => IslandEventType::Notification {
                title: raw
                    .body
                    .get("title")
                    .or_else(|| raw.body.get("event"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("OpenClaw")
                    .to_string(),
                body: raw
                    .body
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            },
            other => anyhow::bail!("Unknown OpenClaw hook event: {other}"),
        };

        Ok(IslandEvent {
            session_id,
            provider_id: self.id().to_string(),
            timestamp,
            event_type,
            terminal_context: extract_terminal_context(&raw.body),
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

    fn config_paths(&self) -> Vec<PathBuf> {
        vec![
            Self::hook_md_path(),
            Self::handler_ts_path(),
            Self::activation_config_path(),
        ]
        .into_iter()
        .filter_map(Result::ok)
        .collect()
    }

    fn detection_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = Self::openclaw_home() {
            paths.push(home.clone());
            paths.push(home.join("openclaw.json"));
            paths.push(home.join("hooks"));
            paths.push(home.join("hooks").join(HOOK_NAME));
            paths.push(home.join("hooks").join(LEGACY_HOOK_NAME));
        }
        if let Ok(transcripts) = Self::transcript_dir() {
            paths.push(transcripts);
        }
        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["openclaw"]
    }
}

pub(crate) async fn activation_entry_state(
    path: Result<PathBuf, anyhow::Error>,
    hook_name: &str,
) -> anyhow::Result<Option<bool>> {
    let path = path?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = tokio::fs::read_to_string(&path).await?;
    let json: serde_json::Value = serde_json::from_str(&contents)?;
    Ok(json
        .pointer(&format!("/hooks/internal/entries/{hook_name}/enabled"))
        .and_then(|value| value.as_bool()))
}

async fn write_activation_config(
    path: &Path,
    hook_name: &str,
    enabled: bool,
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
        .ok_or_else(|| anyhow::anyhow!("OpenClaw activation config is not an object"))?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("OpenClaw hooks is not an object"))?;
    let internal = hooks
        .entry("internal")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("OpenClaw internal hooks is not an object"))?;
    let entries = internal
        .entry("entries")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("OpenClaw internal entries is not an object"))?;
    let entry = entries
        .entry(hook_name)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("OpenClaw internal entry is not an object"))?;

    entry.insert("enabled".into(), serde_json::json!(enabled));
    if enabled {
        internal.insert("enabled".into(), serde_json::json!(true));
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, serde_json::to_string_pretty(&json)?).await?;
    Ok(())
}

async fn remove_activation_entry(path: &Path, hook_name: &str) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let contents = tokio::fs::read_to_string(path).await?;
    let mut json = serde_json::from_str::<serde_json::Value>(&contents)
        .unwrap_or_else(|_| serde_json::json!({}));
    let Some(root) = json.as_object_mut() else {
        return Ok(());
    };
    let Some(hooks) = root
        .get_mut("hooks")
        .and_then(|value| value.as_object_mut())
    else {
        return Ok(());
    };
    let Some(internal) = hooks
        .get_mut("internal")
        .and_then(|value| value.as_object_mut())
    else {
        return Ok(());
    };
    let Some(entries) = internal
        .get_mut("entries")
        .and_then(|value| value.as_object_mut())
    else {
        return Ok(());
    };

    entries.remove(hook_name);
    tokio::fs::write(path, serde_json::to_string_pretty(&json)?).await?;
    Ok(())
}

async fn verify_managed_file(path: Result<PathBuf, anyhow::Error>, expected: &str) -> HookStatus {
    let path = match path {
        Ok(path) => path,
        Err(_) => return HookStatus::NotInstalled,
    };

    if !path.exists() {
        return HookStatus::Outdated;
    }

    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(error) => {
            return HookStatus::Broken {
                reason: format!(
                    "Cannot read managed OpenClaw file {}: {}",
                    path.display(),
                    error
                ),
            };
        }
    };

    if contents == expected {
        return HookStatus::Installed;
    }

    if contents.contains(MANAGED_MARKER) || contents.contains("yorling-bridge") {
        return HookStatus::Outdated;
    }

    HookStatus::Broken {
        reason: format!(
            "Managed OpenClaw file {} is owned by another integration",
            path.display()
        ),
    }
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

fn install_ownership_from_files(
    directory_exists: bool,
    files: impl IntoIterator<Item = ManagedFileOwnership>,
) -> ManagedInstallOwnership {
    if !directory_exists {
        return ManagedInstallOwnership::NotPresent;
    }

    let mut managed = false;
    for file in files {
        match file {
            ManagedFileOwnership::Missing => {}
            ManagedFileOwnership::YorlingManaged => managed = true,
            ManagedFileOwnership::Foreign => return ManagedInstallOwnership::Foreign,
        }
    }

    if managed {
        ManagedInstallOwnership::YorlingManaged
    } else {
        ManagedInstallOwnership::Foreign
    }
}

fn extract_terminal_context(body: &serde_json::Value) -> Option<TerminalContext> {
    let cwd = body
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(String::from);
    cwd.as_ref()?;
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
            .and_then(|value| value.as_str())
            .map(String::from),
        terminal_bundle_id: body
            .get("terminal_bundle_id")
            .and_then(|value| value.as_str())
            .map(String::from),
        terminal_session_id: body
            .get("terminal_session_id")
            .and_then(|value| value.as_str())
            .map(String::from),
        pane_title: body
            .get("title")
            .and_then(|value| value.as_str())
            .map(String::from),
        warp_pane_uuid: None,
    })
}

async fn remove_file_if_exists(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn remove_dir_if_empty(path: &Path) -> anyhow::Result<()> {
    match tokio::fs::remove_dir(path).await {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, OnceLock};

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_openclaw_home(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "yorling-openclaw-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn installs_hook_directory_and_enables_activation_entry() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_openclaw_home("install");
        std::fs::create_dir_all(&home).expect("create home");
        unsafe {
            std::env::set_var("OPENCLAW_HOME", &home);
        }

        let provider = OpenClawProvider::new();
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
        assert!(
            home.join("hooks")
                .join(HOOK_NAME)
                .join(HOOK_MD_NAME)
                .exists()
        );
        assert!(
            home.join("hooks")
                .join(HOOK_NAME)
                .join(HANDLER_TS_NAME)
                .exists()
        );

        let activation = std::fs::read_to_string(home.join("openclaw.json")).expect("activation");
        assert!(activation.contains("\"yorling-island-openclaw\""));
        assert!(activation.contains("\"enabled\": true"));

        unsafe {
            std::env::remove_var("OPENCLAW_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn install_migrates_legacy_yorling_hook_name() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_openclaw_home("migrate");
        let legacy_dir = home.join("hooks").join(LEGACY_HOOK_NAME);
        std::fs::create_dir_all(&legacy_dir).expect("legacy dir");
        std::fs::write(
            legacy_dir.join(HOOK_MD_NAME),
            format!("<!-- {MANAGED_MARKER} -->\n# legacy\n"),
        )
        .expect("legacy hook md");
        std::fs::write(
            legacy_dir.join(HANDLER_TS_NAME),
            format!("// {MANAGED_MARKER}\n// yorling-bridge\n"),
        )
        .expect("legacy handler");
        std::fs::write(
            home.join("openclaw.json"),
            serde_json::json!({
                "hooks": {
                    "internal": {
                        "enabled": true,
                        "entries": {
                            LEGACY_HOOK_NAME: {
                                "enabled": true
                            }
                        }
                    }
                }
            })
            .to_string(),
        )
        .expect("legacy config");
        unsafe {
            std::env::set_var("OPENCLAW_HOME", &home);
        }

        let provider = OpenClawProvider::new();
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await
            .expect("install");

        assert!(!legacy_dir.exists());
        let activation = std::fs::read_to_string(home.join("openclaw.json")).expect("activation");
        assert!(activation.contains(HOOK_NAME));
        assert!(!activation.contains(LEGACY_HOOK_NAME));

        unsafe {
            std::env::remove_var("OPENCLAW_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
