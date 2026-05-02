use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_yaml::{Mapping, Value};

use crate::event::{Decision, IslandEvent, RawHookPayload};
use crate::provider::{AgentProvider, HookStatus};

use super::claude_compat::normalize_claude_compatible_event;

const MANAGED_MARKER: &str = "Yorling managed integration: hermes";
const PLUGIN_DIR_NAME: &str = "yorling_island";
const LEGACY_PLUGIN_DIR_NAME: &str = concat!("ping", "_island");
const PLUGIN_NAME: &str = "yorling-island";
const LEGACY_PLUGIN_NAME: &str = concat!("ping", "-island");
const PLUGIN_YAML_NAME: &str = "plugin.yaml";
const INIT_PY_NAME: &str = "__init__.py";
const CONFIG_YAML_NAME: &str = "config.yaml";

pub struct HermesProvider;

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

#[derive(Default)]
struct HermesPluginConfigState {
    enabled: BTreeSet<String>,
    disabled: BTreeSet<String>,
    legacy_entries: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HermesPluginLoadState {
    Enabled,
    Disabled,
    LegacyListed,
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HermesPluginConfigMutation {
    Enable,
    Remove,
}

impl HermesProvider {
    pub fn new() -> Self {
        Self
    }

    fn hermes_home() -> anyhow::Result<PathBuf> {
        if let Some(path) = std::env::var_os("HERMES_HOME") {
            return Ok(PathBuf::from(path));
        }

        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".hermes"))
    }

    fn config_yaml_path() -> anyhow::Result<PathBuf> {
        Ok(Self::hermes_home()?.join(CONFIG_YAML_NAME))
    }

    fn plugin_dir_for(dir_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::hermes_home()?.join("plugins").join(dir_name))
    }

    fn plugin_dir() -> anyhow::Result<PathBuf> {
        Self::plugin_dir_for(PLUGIN_DIR_NAME)
    }

    fn legacy_plugin_dir() -> anyhow::Result<PathBuf> {
        Self::plugin_dir_for(LEGACY_PLUGIN_DIR_NAME)
    }

    fn plugin_yaml_path_for(dir_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::plugin_dir_for(dir_name)?.join(PLUGIN_YAML_NAME))
    }

    fn plugin_yaml_path() -> anyhow::Result<PathBuf> {
        Self::plugin_yaml_path_for(PLUGIN_DIR_NAME)
    }

    fn legacy_plugin_yaml_path() -> anyhow::Result<PathBuf> {
        Self::plugin_yaml_path_for(LEGACY_PLUGIN_DIR_NAME)
    }

    fn init_py_path_for(dir_name: &str) -> anyhow::Result<PathBuf> {
        Ok(Self::plugin_dir_for(dir_name)?.join(INIT_PY_NAME))
    }

    fn init_py_path() -> anyhow::Result<PathBuf> {
        Self::init_py_path_for(PLUGIN_DIR_NAME)
    }

    fn legacy_init_py_path() -> anyhow::Result<PathBuf> {
        Self::init_py_path_for(LEGACY_PLUGIN_DIR_NAME)
    }

    fn bridge_base_args(bridge_path: &Path, transport_endpoint: &Path) -> Vec<String> {
        vec![
            bridge_path.display().to_string(),
            "--source".into(),
            "hermes".into(),
            super::transport_flag().into(),
            transport_endpoint.display().to_string(),
        ]
    }

    fn render_plugin_yaml() -> String {
        include_str!("assets/hermes_plugin.yaml").replace("__YORLING_MARKER__", MANAGED_MARKER)
    }

    fn render_init_py(bridge_path: &Path, transport_endpoint: &Path) -> String {
        let args_json =
            serde_json::to_string(&Self::bridge_base_args(bridge_path, transport_endpoint))
                .expect("bridge args json");
        include_str!("assets/hermes_init.py")
            .replace("__YORLING_MARKER__", MANAGED_MARKER)
            .replace("__BRIDGE_BASE_ARGS_JSON__", &args_json)
    }

    async fn cleanup_legacy_install_if_managed() -> anyhow::Result<bool> {
        if Self::legacy_install_ownership().await? != ManagedInstallOwnership::YorlingManaged {
            return Ok(false);
        }

        let legacy_dir = Self::legacy_plugin_dir()?;
        remove_file_if_exists(&Self::legacy_plugin_yaml_path()?).await?;
        remove_file_if_exists(&Self::legacy_init_py_path()?).await?;
        remove_dir_if_empty(&legacy_dir).await?;
        Ok(true)
    }

    async fn legacy_install_ownership() -> anyhow::Result<ManagedInstallOwnership> {
        Self::plugin_install_ownership(LEGACY_PLUGIN_DIR_NAME).await
    }

    async fn plugin_install_ownership(dir_name: &str) -> anyhow::Result<ManagedInstallOwnership> {
        let plugin_dir = Self::plugin_dir_for(dir_name)?;
        if !plugin_dir.exists() {
            return Ok(ManagedInstallOwnership::NotPresent);
        }

        let yaml = managed_file_ownership(&Self::plugin_yaml_path_for(dir_name)?).await?;
        let init = managed_file_ownership(&Self::init_py_path_for(dir_name)?).await?;
        Ok(install_ownership_from_files(
            plugin_dir.exists(),
            [yaml, init],
        ))
    }

    async fn sync_plugin_config(
        mutation: HermesPluginConfigMutation,
        plugin_name: &str,
        extra_remove_names: &[&str],
    ) -> anyhow::Result<()> {
        let config_path = Self::config_yaml_path()?;
        if !config_path.exists() && mutation == HermesPluginConfigMutation::Remove {
            return Ok(());
        }

        let mut root = if config_path.exists() {
            let contents = tokio::fs::read_to_string(&config_path).await?;
            serde_yaml::from_str::<Value>(&contents).map_err(|error| {
                anyhow::anyhow!("Invalid Hermes config {}: {}", config_path.display(), error)
            })?
        } else {
            Value::Mapping(Mapping::new())
        };

        let root_map = root
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("Hermes config root is not an object"))?;
        let existing_key = Value::String("plugins".into());
        let existing = root_map.get(&existing_key);
        let mut state = extract_plugin_config_state(existing)?;

        for name in extra_remove_names {
            state.enabled.remove(*name);
            state.disabled.remove(*name);
            state.legacy_entries.remove(*name);
        }

        state.enabled.remove(plugin_name);
        state.disabled.remove(plugin_name);
        state.legacy_entries.remove(plugin_name);

        if mutation == HermesPluginConfigMutation::Enable {
            state.enabled.insert(plugin_name.to_string());
        }

        let mut plugins_map = Mapping::new();
        plugins_map.insert(
            Value::String("enabled".into()),
            yaml_sequence_from_set(&state.enabled),
        );
        plugins_map.insert(
            Value::String("disabled".into()),
            yaml_sequence_from_set(&state.disabled),
        );
        root_map.insert(Value::String("plugins".into()), Value::Mapping(plugins_map));

        if let Some(parent) = config_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&config_path, serde_yaml::to_string(&root)?).await?;
        Ok(())
    }

    async fn load_plugin_config_state() -> anyhow::Result<Option<HermesPluginConfigState>> {
        let config_path = Self::config_yaml_path()?;
        if !config_path.exists() {
            return Ok(None);
        }

        let contents = tokio::fs::read_to_string(&config_path).await?;
        let yaml = serde_yaml::from_str::<Value>(&contents).map_err(|error| {
            anyhow::anyhow!("Invalid Hermes config {}: {}", config_path.display(), error)
        })?;
        let plugins = yaml
            .as_mapping()
            .and_then(|mapping| mapping.get(&Value::String("plugins".into())));
        Ok(Some(extract_plugin_config_state(plugins)?))
    }
}

#[async_trait]
impl AgentProvider for HermesProvider {
    fn id(&self) -> &str {
        "hermes"
    }

    fn display_name(&self) -> &str {
        "Hermes"
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let removed_legacy = Self::cleanup_legacy_install_if_managed().await?;
        let plugin_dir = Self::plugin_dir()?;
        tokio::fs::create_dir_all(&plugin_dir).await?;
        tokio::fs::write(Self::plugin_yaml_path()?, Self::render_plugin_yaml()).await?;
        tokio::fs::write(
            Self::init_py_path()?,
            Self::render_init_py(bridge_path, transport_endpoint),
        )
        .await?;

        let extra_remove_names = if removed_legacy {
            vec![LEGACY_PLUGIN_NAME]
        } else {
            Vec::new()
        };
        Self::sync_plugin_config(
            HermesPluginConfigMutation::Enable,
            PLUGIN_NAME,
            &extra_remove_names,
        )
        .await
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let removed_legacy = Self::cleanup_legacy_install_if_managed().await?;
        let plugin_dir = Self::plugin_dir()?;
        remove_file_if_exists(&Self::plugin_yaml_path()?).await?;
        remove_file_if_exists(&Self::init_py_path()?).await?;
        remove_dir_if_empty(&plugin_dir).await?;

        let extra_remove_names = if removed_legacy {
            vec![LEGACY_PLUGIN_NAME]
        } else {
            Vec::new()
        };
        Self::sync_plugin_config(
            HermesPluginConfigMutation::Remove,
            PLUGIN_NAME,
            &extra_remove_names,
        )
        .await
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        let plugin_dir = match Self::plugin_dir() {
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

        let config_state = match Self::load_plugin_config_state().await {
            Ok(state) => state,
            Err(error)
                if plugin_dir.exists()
                    || matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged) =>
            {
                return HookStatus::Broken {
                    reason: error.to_string(),
                };
            }
            Err(_) => None,
        };

        if !plugin_dir.exists() {
            let new_configured = config_state.as_ref().is_some_and(|state| {
                plugin_load_state(state, PLUGIN_NAME) != HermesPluginLoadState::Missing
            });
            let legacy_configured =
                matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged)
                    && config_state.as_ref().is_some_and(|state| {
                        plugin_load_state(state, LEGACY_PLUGIN_NAME)
                            != HermesPluginLoadState::Missing
                    });

            return if matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged)
                || new_configured
                || legacy_configured
            {
                HookStatus::Outdated
            } else {
                HookStatus::NotInstalled
            };
        }

        let yaml_status =
            verify_managed_file(Self::plugin_yaml_path(), &Self::render_plugin_yaml()).await;
        let init_status = verify_managed_file(
            Self::init_py_path(),
            &Self::render_init_py(bridge_path, transport_endpoint),
        )
        .await;

        match (yaml_status, init_status) {
            (Err(HookStatus::Broken { reason }), _) | (_, Err(HookStatus::Broken { reason })) => {
                return HookStatus::Broken { reason };
            }
            (Ok(HookStatus::Installed), Ok(HookStatus::Installed)) => {}
            _ => return HookStatus::Outdated,
        }

        let plugin_enabled = config_state
            .as_ref()
            .map(|state| plugin_load_state(state, PLUGIN_NAME))
            .unwrap_or(HermesPluginLoadState::Missing);
        let legacy_configured =
            if matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged) {
                config_state
                    .as_ref()
                    .map(|state| plugin_load_state(state, LEGACY_PLUGIN_NAME))
                    .unwrap_or(HermesPluginLoadState::Missing)
            } else {
                HermesPluginLoadState::Missing
            };

        if plugin_enabled == HermesPluginLoadState::Enabled {
            HookStatus::Installed
        } else if plugin_dir.exists()
            || plugin_enabled != HermesPluginLoadState::Missing
            || matches!(legacy_ownership, ManagedInstallOwnership::YorlingManaged)
            || legacy_configured != HermesPluginLoadState::Missing
        {
            HookStatus::Outdated
        } else {
            HookStatus::NotInstalled
        }
    }

    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
        normalize_claude_compatible_event(
            self.id(),
            self.display_name(),
            raw,
            &["assistant_message"],
            false,
        )
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
            Self::plugin_yaml_path(),
            Self::init_py_path(),
            Self::config_yaml_path(),
        ]
        .into_iter()
        .filter_map(Result::ok)
        .collect()
    }

    fn detection_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(home) = Self::hermes_home() {
            paths.push(home.clone());
            paths.push(home.join("plugins"));
            paths.push(home.join("plugins").join(PLUGIN_DIR_NAME));
            paths.push(home.join("plugins").join(LEGACY_PLUGIN_DIR_NAME));
        }
        paths
    }

    fn detection_commands(&self) -> &'static [&'static str] {
        &["hermes"]
    }
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

fn extract_plugin_config_state(
    plugins_value: Option<&Value>,
) -> anyhow::Result<HermesPluginConfigState> {
    let mut state = HermesPluginConfigState::default();
    let Some(plugins_value) = plugins_value else {
        return Ok(state);
    };

    match plugins_value {
        Value::Null => {}
        Value::Sequence(sequence) => {
            state.legacy_entries = yaml_string_set(sequence.iter());
        }
        Value::Mapping(mapping) => {
            state.enabled = yaml_value_string_set(mapping.get(&Value::String("enabled".into())));
            state.disabled = yaml_value_string_set(mapping.get(&Value::String("disabled".into())));
        }
        _ => anyhow::bail!("Hermes plugins config is not a list or object"),
    }

    Ok(state)
}

fn yaml_value_string_set(value: Option<&Value>) -> BTreeSet<String> {
    match value {
        Some(Value::Sequence(sequence)) => yaml_string_set(sequence.iter()),
        Some(Value::String(single)) => BTreeSet::from([single.clone()]),
        _ => BTreeSet::new(),
    }
}

fn yaml_string_set<'a>(values: impl IntoIterator<Item = &'a Value>) -> BTreeSet<String> {
    values
        .into_iter()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn yaml_sequence_from_set(values: &BTreeSet<String>) -> Value {
    Value::Sequence(
        values
            .iter()
            .cloned()
            .map(Value::String)
            .collect::<Vec<_>>(),
    )
}

fn plugin_load_state(state: &HermesPluginConfigState, plugin_name: &str) -> HermesPluginLoadState {
    if state.enabled.contains(plugin_name) {
        HermesPluginLoadState::Enabled
    } else if state.disabled.contains(plugin_name) {
        HermesPluginLoadState::Disabled
    } else if state.legacy_entries.contains(plugin_name) {
        HermesPluginLoadState::LegacyListed
    } else {
        HermesPluginLoadState::Missing
    }
}

async fn verify_managed_file(
    path: Result<PathBuf, anyhow::Error>,
    expected: &str,
) -> Result<HookStatus, HookStatus> {
    let path = match path {
        Ok(path) => path,
        Err(_) => return Err(HookStatus::NotInstalled),
    };

    if !path.exists() {
        return Ok(HookStatus::Outdated);
    }

    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(error) => {
            return Err(HookStatus::Broken {
                reason: format!(
                    "Cannot read managed Hermes file {}: {}",
                    path.display(),
                    error
                ),
            });
        }
    };

    if contents == expected {
        return Ok(HookStatus::Installed);
    }

    if contents.contains(MANAGED_MARKER) || contents.contains("yorling-bridge") {
        return Ok(HookStatus::Outdated);
    }

    Err(HookStatus::Broken {
        reason: format!(
            "Managed Hermes file {} is owned by another integration",
            path.display()
        ),
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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};

    use crate::event::IslandEventType;

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_hermes_home(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "yorling-hermes-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn installs_and_verifies_managed_plugin_directory() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_hermes_home("install");
        std::fs::create_dir_all(&home).expect("create home");
        unsafe {
            std::env::set_var("HERMES_HOME", &home);
        }

        let provider = HermesProvider::new();
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
            home.join("plugins")
                .join(PLUGIN_DIR_NAME)
                .join(PLUGIN_YAML_NAME)
                .exists()
        );
        assert!(
            home.join("plugins")
                .join(PLUGIN_DIR_NAME)
                .join(INIT_PY_NAME)
                .exists()
        );

        let config = std::fs::read_to_string(home.join(CONFIG_YAML_NAME)).expect("config");
        assert!(config.contains("enabled:"));
        assert!(config.contains(PLUGIN_NAME));

        unsafe {
            std::env::remove_var("HERMES_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn install_migrates_legacy_yorling_plugin_name_and_config_shape() {
        let _guard = env_lock().lock().expect("env lock");
        let home = temp_hermes_home("migrate");
        let legacy_dir = home.join("plugins").join(LEGACY_PLUGIN_DIR_NAME);
        std::fs::create_dir_all(&legacy_dir).expect("legacy dir");
        std::fs::write(
            legacy_dir.join(PLUGIN_YAML_NAME),
            format!("# {MANAGED_MARKER}\nname: {LEGACY_PLUGIN_NAME}\n"),
        )
        .expect("legacy yaml");
        std::fs::write(
            legacy_dir.join(INIT_PY_NAME),
            format!("# {MANAGED_MARKER}\n# yorling-bridge\n"),
        )
        .expect("legacy init");
        std::fs::write(
            home.join(CONFIG_YAML_NAME),
            format!("plugins:\n- {LEGACY_PLUGIN_NAME}\n"),
        )
        .expect("legacy config");
        unsafe {
            std::env::set_var("HERMES_HOME", &home);
        }

        let provider = HermesProvider::new();
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/island.sock"),
            )
            .await
            .expect("install");

        assert!(!legacy_dir.exists());
        assert!(home.join("plugins").join(PLUGIN_DIR_NAME).exists());

        let config = std::fs::read_to_string(home.join(CONFIG_YAML_NAME)).expect("config");
        assert!(config.contains("enabled:"));
        assert!(config.contains(PLUGIN_NAME));
        assert!(!config.contains(LEGACY_PLUGIN_NAME));

        unsafe {
            std::env::remove_var("HERMES_HOME");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn assistant_notifications_become_agent_responses() {
        let provider = HermesProvider::new();
        let raw = RawHookPayload {
            hook_event: "Notification".into(),
            body: serde_json::json!({
                "session_id": "hermes-1",
                "notification_type": "assistant_message",
                "message": "Done.",
            }),
            source: "hermes".into(),
            session_id: Some("hermes-1".into()),
        };

        let event = provider.normalize_event(&raw).expect("normalize");
        match event.event_type {
            IslandEventType::AgentResponse { text } => assert_eq!(text, "Done."),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
