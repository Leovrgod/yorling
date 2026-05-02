use std::path::{Path, PathBuf};

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{AgentProvider, HookStatus, ProviderOrigin};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IslandPluginSource {
    BuiltIn,
    Manifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IslandPluginSlot {
    pub id: String,
    pub slot: String,
    pub title: Option<String>,
    pub template: String,
    pub provider_ids: Vec<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IslandPluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub source: IslandPluginSource,
    pub provider_id: Option<String>,
    pub manifest_path: Option<String>,
    pub slots: Vec<IslandPluginSlot>,
}

pub struct PluginLoadResult {
    pub plugins: Vec<IslandPluginInfo>,
    pub providers: Vec<Box<dyn AgentProvider>>,
}

/// Registry for managing loaded plugins with hot-reload support.
pub struct PluginRegistry {
    plugins: DashMap<String, IslandPluginInfo>,
    search_dirs: Vec<PathBuf>,
}

impl PluginRegistry {
    pub fn new(search_dirs: Vec<PathBuf>) -> Self {
        Self {
            plugins: DashMap::new(),
            search_dirs,
        }
    }

    /// Load or reload all plugins from search directories.
    /// Returns newly discovered providers that should be registered.
    pub fn reload(&self) -> PluginLoadResult {
        let result = load_plugins(&self.search_dirs);
        self.plugins.clear();
        for plugin in &result.plugins {
            self.plugins.insert(plugin.id.clone(), plugin.clone());
        }
        result
    }

    pub fn get(&self, plugin_id: &str) -> Option<IslandPluginInfo> {
        self.plugins.get(plugin_id).map(|r| r.value().clone())
    }

    pub fn list(&self) -> Vec<IslandPluginInfo> {
        self.plugins.iter().map(|r| r.value().clone()).collect()
    }

    /// List all slots for a given slot name, filtered by provider_id if applicable.
    pub fn slots_for(&self, slot_name: &str, provider_id: Option<&str>) -> Vec<IslandPluginSlot> {
        let mut slots: Vec<IslandPluginSlot> = self
            .plugins
            .iter()
            .flat_map(|entry| {
                entry
                    .value()
                    .slots
                    .iter()
                    .filter(|s| {
                        s.slot == slot_name
                            && (s.provider_ids.is_empty()
                                || provider_id
                                    .is_some_and(|pid| s.provider_ids.iter().any(|id| id == pid)))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();
        slots.sort_by_key(|s| s.priority);
        slots
    }

    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PluginManifestFile {
    id: String,
    name: String,
    #[serde(default = "default_plugin_version")]
    version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    slots: Vec<ManifestSlotSpec>,
    #[serde(default)]
    provider: Option<ManifestProviderSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestSlotSpec {
    id: String,
    slot: String,
    #[serde(default)]
    title: Option<String>,
    template: String,
    #[serde(default)]
    provider_ids: Vec<String>,
    #[serde(default)]
    priority: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestProviderSpec {
    id: String,
    display_name: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    supports_blocking_permission: bool,
    #[serde(default)]
    config_paths: Vec<String>,
    #[serde(default)]
    event_mappings: Vec<ManifestEventMapping>,
    #[serde(default)]
    field_keys: ManifestFieldKeys,
    #[serde(default)]
    install: Option<ManifestInstallSpec>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ManifestInstallSpec {
    path: String,
    #[serde(default)]
    contents: String,
    /// Path to a template file relative to the bundle directory.
    /// If set, its content is used instead of `contents`.
    #[serde(default)]
    contents_file: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestEventMapping {
    hook_event: String,
    event_type: ManifestEventType,
    #[serde(default)]
    tool_key: Option<String>,
    #[serde(default)]
    input_key: Option<String>,
    #[serde(default)]
    text_key: Option<String>,
    #[serde(default)]
    success_key: Option<String>,
    #[serde(default)]
    question_key: Option<String>,
    #[serde(default)]
    options_key: Option<String>,
    #[serde(default)]
    request_id_key: Option<String>,
    #[serde(default)]
    title_key: Option<String>,
    #[serde(default)]
    body_key: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ManifestEventType {
    SessionStart,
    SessionEnd,
    UserPrompt,
    ToolUseStart,
    ToolUseEnd,
    PermissionRequest,
    AskQuestion,
    AgentResponse,
    Notification,
    ContextCompaction,
    Stop,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestFieldKeys {
    #[serde(default = "default_session_id_keys")]
    session_id: Vec<String>,
    #[serde(default = "default_provider_session_id_keys")]
    provider_session_id: Vec<String>,
    #[serde(default = "default_transcript_path_keys")]
    transcript_path: Vec<String>,
    #[serde(default = "default_pid_keys")]
    pid: Vec<String>,
    #[serde(default = "default_tty_keys")]
    tty: Vec<String>,
    #[serde(default = "default_cwd_keys")]
    cwd: Vec<String>,
    #[serde(default = "default_terminal_app_keys")]
    terminal_app: Vec<String>,
    #[serde(default = "default_terminal_bundle_id_keys")]
    terminal_bundle_id: Vec<String>,
    #[serde(default = "default_terminal_session_id_keys")]
    terminal_session_id: Vec<String>,
    #[serde(default = "default_pane_title_keys")]
    pane_title: Vec<String>,
    #[serde(default = "default_warp_pane_uuid_keys")]
    warp_pane_uuid: Vec<String>,
}

impl Default for ManifestFieldKeys {
    fn default() -> Self {
        Self {
            session_id: default_session_id_keys(),
            provider_session_id: default_provider_session_id_keys(),
            transcript_path: default_transcript_path_keys(),
            pid: default_pid_keys(),
            tty: default_tty_keys(),
            cwd: default_cwd_keys(),
            terminal_app: default_terminal_app_keys(),
            terminal_bundle_id: default_terminal_bundle_id_keys(),
            terminal_session_id: default_terminal_session_id_keys(),
            pane_title: default_pane_title_keys(),
            warp_pane_uuid: default_warp_pane_uuid_keys(),
        }
    }
}

#[derive(Debug, Clone)]
struct ManifestProvider {
    plugin_id: String,
    manifest_path: PathBuf,
    /// Parent directory of the manifest, used to resolve `contents_file` paths
    bundle_dir: Option<PathBuf>,
    slot_count: usize,
    spec: ManifestProviderSpec,
}

impl PluginLoadResult {
    pub fn empty() -> Self {
        Self {
            plugins: Vec::new(),
            providers: Vec::new(),
        }
    }
}

pub fn load_plugins(search_dirs: &[PathBuf]) -> PluginLoadResult {
    let mut result = PluginLoadResult::empty();
    result.plugins.extend(builtin_plugins());

    for dir in search_dirs {
        if !dir.is_dir() {
            continue;
        }

        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Case 1: .yorling-plugin bundle directory containing plugin.json
            if path.is_dir()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("yorling-plugin"))
            {
                let manifest_path = path.join("plugin.json");
                if manifest_path.is_file() {
                    load_manifest_file(&manifest_path, Some(&path), &mut result);
                }
                continue;
            }

            // Case 2: standalone .json manifest file
            if !path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                continue;
            }

            load_manifest_file(&path, None, &mut result);
        }
    }

    result
}

fn load_manifest_file(path: &Path, bundle_dir: Option<&Path>, result: &mut PluginLoadResult) {
    let Ok(contents) = std::fs::read_to_string(path) else {
        warn!(
            "Skipping unreadable island plugin manifest: {}",
            path.display()
        );
        return;
    };

    let Ok(manifest) = serde_json::from_str::<PluginManifestFile>(&contents) else {
        warn!(
            "Skipping malformed island plugin manifest: {}",
            path.display()
        );
        return;
    };

    let plugin_info = manifest_to_plugin_info(&manifest, Some(path.to_path_buf()));
    if result
        .plugins
        .iter()
        .any(|plugin| plugin.id == plugin_info.id)
    {
        warn!("Skipping duplicate island plugin id '{}'", plugin_info.id);
        return;
    }

    if let Some(provider_spec) = manifest.provider.clone() {
        result.providers.push(Box::new(ManifestProvider {
            plugin_id: manifest.id.clone(),
            manifest_path: path.to_path_buf(),
            bundle_dir: bundle_dir.map(Path::to_path_buf),
            slot_count: manifest.slots.len(),
            spec: provider_spec,
        }));
    }

    result.plugins.push(plugin_info);
}

pub fn builtin_plugins() -> Vec<IslandPluginInfo> {
    vec![
        IslandPluginInfo {
            id: "core.transcript-preview".into(),
            name: "Transcript Preview".into(),
            version: "1.0.0".into(),
            description: Some(
                "Expose transcript freshness and provider session IDs inside session cards.".into(),
            ),
            source: IslandPluginSource::BuiltIn,
            provider_id: None,
            manifest_path: None,
            slots: vec![
                IslandPluginSlot {
                    id: "core.transcript-preview.status".into(),
                    slot: "session_footer".into(),
                    title: Some("Transcript".into()),
                    template: "TX {transcript_status_label}".into(),
                    provider_ids: Vec::new(),
                    priority: 10,
                },
                IslandPluginSlot {
                    id: "core.transcript-preview.session-id".into(),
                    slot: "session_footer".into(),
                    title: Some("Session".into()),
                    template: "SID {provider_session_id_short}".into(),
                    provider_ids: Vec::new(),
                    priority: 20,
                },
            ],
        },
        IslandPluginInfo {
            id: "core.tool-history".into(),
            name: "Tool History Summary".into(),
            version: "1.0.0".into(),
            description: Some("Highlight the last parsed tool result in the island footer.".into()),
            source: IslandPluginSource::BuiltIn,
            provider_id: None,
            manifest_path: None,
            slots: vec![IslandPluginSlot {
                id: "core.tool-history.latest".into(),
                slot: "session_footer".into(),
                title: Some("Tool".into()),
                template: "{latest_tool_label}".into(),
                provider_ids: Vec::new(),
                priority: 30,
            }],
        },
    ]
}

fn manifest_to_plugin_info(
    manifest: &PluginManifestFile,
    manifest_path: Option<PathBuf>,
) -> IslandPluginInfo {
    IslandPluginInfo {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        description: manifest.description.clone(),
        source: IslandPluginSource::Manifest,
        provider_id: manifest.provider.as_ref().map(|provider| {
            provider
                .source
                .clone()
                .unwrap_or_else(|| provider.id.clone())
        }),
        manifest_path: manifest_path.map(|path| path.display().to_string()),
        slots: manifest
            .slots
            .iter()
            .map(|slot| IslandPluginSlot {
                id: slot.id.clone(),
                slot: slot.slot.clone(),
                title: slot.title.clone(),
                template: slot.template.clone(),
                provider_ids: slot.provider_ids.clone(),
                priority: slot.priority,
            })
            .collect(),
    }
}

#[async_trait]
impl AgentProvider for ManifestProvider {
    fn id(&self) -> &str {
        self.spec.source.as_deref().unwrap_or(&self.spec.id)
    }

    fn display_name(&self) -> &str {
        &self.spec.display_name
    }

    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()> {
        let install = self
            .spec
            .install
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{} does not support managed install", self.id()))?;
        let path = expand_path(&install.path);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Resolve template: prefer contents_file (relative to bundle dir), then inline contents
        let template = if let Some(contents_file) = &install.contents_file {
            let file_path = if let Some(bundle_dir) = &self.bundle_dir {
                bundle_dir.join(contents_file)
            } else {
                self.manifest_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(contents_file)
            };
            tokio::fs::read_to_string(&file_path).await.map_err(|e| {
                anyhow::anyhow!("Failed to read contents_file {}: {e}", file_path.display())
            })?
        } else {
            install.contents.clone()
        };

        let contents = template
            .replace("{bridge_path}", &bridge_path.display().to_string())
            .replace(
                "{transport_endpoint}",
                &transport_endpoint.display().to_string(),
            )
            .replace("{transport_flag}", transport_flag());

        tokio::fs::write(&path, contents).await?;
        Ok(())
    }

    async fn uninstall_hooks(&self) -> anyhow::Result<()> {
        let install =
            self.spec.install.as_ref().ok_or_else(|| {
                anyhow::anyhow!("{} does not support managed uninstall", self.id())
            })?;
        let path = expand_path(&install.path);
        match tokio::fs::remove_file(&path).await {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus {
        if let Some(install) = &self.spec.install {
            let path = expand_path(&install.path);
            if !path.exists() {
                return HookStatus::NotInstalled;
            }

            // Resolve expected contents from contents_file or inline
            let template = if let Some(contents_file) = &install.contents_file {
                let file_path = if let Some(bundle_dir) = &self.bundle_dir {
                    bundle_dir.join(contents_file)
                } else {
                    self.manifest_path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(contents_file)
                };
                match tokio::fs::read_to_string(&file_path).await {
                    Ok(contents) => contents,
                    Err(_) => {
                        return HookStatus::Broken {
                            reason: format!("Cannot read contents_file: {}", file_path.display()),
                        };
                    }
                }
            } else {
                install.contents.clone()
            };

            let expected = template
                .replace("{bridge_path}", &bridge_path.display().to_string())
                .replace(
                    "{transport_endpoint}",
                    &transport_endpoint.display().to_string(),
                )
                .replace("{transport_flag}", transport_flag());
            return match tokio::fs::read_to_string(&path).await {
                Ok(contents) if contents == expected => HookStatus::Installed,
                Ok(contents) if contents.contains("yorling-bridge") => HookStatus::Outdated,
                Ok(_) => HookStatus::Broken {
                    reason: format!(
                        "Managed install target {} is owned by another plugin",
                        path.display()
                    ),
                },
                Err(error) => HookStatus::Broken {
                    reason: format!(
                        "Cannot read managed install target {}: {}",
                        path.display(),
                        error
                    ),
                },
            };
        }

        let config_paths = self.config_paths();
        if config_paths.iter().any(|path| path.exists()) {
            HookStatus::Installed
        } else {
            HookStatus::NotInstalled
        }
    }

    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
        let mapping = self
            .spec
            .event_mappings
            .iter()
            .find(|mapping| mapping.hook_event == raw.hook_event)
            .ok_or_else(|| anyhow::anyhow!("Unknown manifest hook event: {}", raw.hook_event))?;

        let timestamp = current_timestamp();
        let session_id = raw
            .session_id
            .clone()
            .or_else(|| get_first_string(&raw.body, &self.spec.field_keys.session_id))
            .or_else(|| get_first_string(&raw.body, &self.spec.field_keys.provider_session_id))
            .unwrap_or_else(|| format!("{}:{}", self.id(), timestamp));
        let _transcript_path = get_first_string(&raw.body, &self.spec.field_keys.transcript_path);
        let terminal_context = extract_terminal_context(&raw.body, &self.spec.field_keys, None);
        let event_type = mapping_to_event(mapping, &raw.body, &session_id, timestamp)?;

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
        Ok(serde_json::to_vec(&serde_json::json!({
            "decision": match decision {
                Decision::Allow => "allow",
                Decision::Deny => "deny",
                Decision::AllowAlways => "allow_always",
            }
        }))?)
    }

    fn encode_question_response(
        &self,
        answer: &str,
        _from_permission: bool,
        answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>> {
        let mut payload = serde_json::Map::new();
        payload.insert("answer".into(), Value::String(answer.to_string()));
        if let Some(answer_key) = answer_key {
            payload.insert("answer_key".into(), Value::String(answer_key.to_string()));
        }
        Ok(serde_json::to_vec(&Value::Object(payload))?)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::External
    }

    fn plugin_id(&self) -> Option<&str> {
        Some(&self.plugin_id)
    }

    fn config_paths(&self) -> Vec<PathBuf> {
        let mut paths = self
            .spec
            .config_paths
            .iter()
            .map(|path| expand_path(path))
            .collect::<Vec<_>>();
        if let Some(install) = &self.spec.install {
            paths.push(expand_path(&install.path));
        }
        paths.push(self.manifest_path.clone());
        paths
    }

    fn manages_hooks(&self) -> bool {
        self.spec.install.is_some()
    }

    fn slot_count(&self) -> usize {
        self.slot_count
    }
}

fn mapping_to_event(
    mapping: &ManifestEventMapping,
    body: &Value,
    session_id: &str,
    timestamp: i64,
) -> anyhow::Result<IslandEventType> {
    Ok(match mapping.event_type {
        ManifestEventType::SessionStart => IslandEventType::SessionStart,
        ManifestEventType::SessionEnd => IslandEventType::SessionEnd,
        ManifestEventType::UserPrompt => IslandEventType::UserPrompt {
            text: get_string_with_fallback(
                body,
                mapping.text_key.as_deref(),
                &["prompt", "text", "message"],
            )
            .unwrap_or_default(),
        },
        ManifestEventType::ToolUseStart => IslandEventType::ToolUseStart {
            tool: get_string_with_fallback(
                body,
                mapping.tool_key.as_deref(),
                &["tool_name", "tool", "name"],
            )
            .unwrap_or_else(|| "unknown".into()),
            input: get_value_with_fallback(
                body,
                mapping.input_key.as_deref(),
                &["tool_input", "input"],
            )
            .unwrap_or_else(|| serde_json::json!({})),
        },
        ManifestEventType::ToolUseEnd => IslandEventType::ToolUseEnd {
            tool: get_string_with_fallback(
                body,
                mapping.tool_key.as_deref(),
                &["tool_name", "tool", "name"],
            )
            .unwrap_or_else(|| "unknown".into()),
            success: get_bool_with_fallback(
                body,
                mapping.success_key.as_deref(),
                &["success", "ok"],
            )
            .unwrap_or(true),
        },
        ManifestEventType::PermissionRequest => IslandEventType::PermissionRequest {
            tool: get_string_with_fallback(
                body,
                mapping.tool_key.as_deref(),
                &["tool_name", "tool", "name"],
            )
            .unwrap_or_else(|| "unknown".into()),
            input: get_value_with_fallback(
                body,
                mapping.input_key.as_deref(),
                &["tool_input", "input"],
            )
            .unwrap_or_else(|| serde_json::json!({})),
            request_id: get_string_with_fallback(
                body,
                mapping.request_id_key.as_deref(),
                &["request_id", "requestId"],
            )
            .unwrap_or_else(|| format!("{session_id}:{timestamp}")),
        },
        ManifestEventType::AskQuestion => IslandEventType::AskQuestion {
            question: get_string_with_fallback(
                body,
                mapping.question_key.as_deref(),
                &["question", "prompt"],
            )
            .unwrap_or_default(),
            options: get_options_with_fallback(body, mapping.options_key.as_deref(), &["options"]),
            request_id: get_string_with_fallback(
                body,
                mapping.request_id_key.as_deref(),
                &["request_id", "requestId"],
            )
            .unwrap_or_else(|| format!("{session_id}:{timestamp}")),
            from_permission: false,
            answer_key: None,
        },
        ManifestEventType::AgentResponse => IslandEventType::AgentResponse {
            text: get_string_with_fallback(
                body,
                mapping.text_key.as_deref(),
                &["response", "text", "message"],
            )
            .unwrap_or_default(),
        },
        ManifestEventType::Notification => IslandEventType::Notification {
            title: get_string_with_fallback(body, mapping.title_key.as_deref(), &["title"])
                .unwrap_or_default(),
            body: get_string_with_fallback(
                body,
                mapping.body_key.as_deref(),
                &["message", "body", "text"],
            )
            .unwrap_or_default(),
        },
        ManifestEventType::ContextCompaction => IslandEventType::ContextCompaction,
        ManifestEventType::Stop => IslandEventType::Stop,
    })
}

fn get_string_with_fallback(
    body: &Value,
    preferred_key: Option<&str>,
    fallback: &[&str],
) -> Option<String> {
    if let Some(preferred_key) = preferred_key {
        if let Some(value) = body.get(preferred_key) {
            return value.as_str().map(str::to_string);
        }
    }
    get_first_string(
        body,
        fallback
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
            .as_slice(),
    )
}

fn get_value_with_fallback(
    body: &Value,
    preferred_key: Option<&str>,
    fallback: &[&str],
) -> Option<Value> {
    if let Some(preferred_key) = preferred_key {
        if let Some(value) = body.get(preferred_key) {
            return Some(value.clone());
        }
    }

    for key in fallback {
        if let Some(value) = body.get(*key) {
            return Some(value.clone());
        }
    }

    None
}

fn get_bool_with_fallback(
    body: &Value,
    preferred_key: Option<&str>,
    fallback: &[&str],
) -> Option<bool> {
    if let Some(preferred_key) = preferred_key {
        if let Some(value) = body.get(preferred_key).and_then(|value| value.as_bool()) {
            return Some(value);
        }
    }

    for key in fallback {
        if let Some(value) = body.get(*key).and_then(|value| value.as_bool()) {
            return Some(value);
        }
    }

    None
}

fn get_options_with_fallback(
    body: &Value,
    preferred_key: Option<&str>,
    fallback: &[&str],
) -> Vec<String> {
    let value = get_value_with_fallback(body, preferred_key, fallback);
    let Some(Value::Array(options)) = value else {
        return Vec::new();
    };

    options
        .into_iter()
        .filter_map(|option| match option {
            Value::String(text) => Some(text),
            Value::Object(map) => map
                .get("label")
                .and_then(|value| value.as_str())
                .map(str::to_string)
                .or_else(|| {
                    map.get("title")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                }),
            _ => None,
        })
        .collect()
}

fn extract_terminal_context(
    body: &Value,
    field_keys: &ManifestFieldKeys,
    default_terminal_app: Option<&str>,
) -> Option<TerminalContext> {
    let cwd = get_first_string(body, &field_keys.cwd);
    let terminal_app = get_first_string(body, &field_keys.terminal_app)
        .or_else(|| default_terminal_app.map(str::to_string));
    let terminal_session_id = get_first_string(body, &field_keys.terminal_session_id);
    let pane_title = get_first_string(body, &field_keys.pane_title);

    if cwd.is_none()
        && terminal_app.is_none()
        && terminal_session_id.is_none()
        && pane_title.is_none()
    {
        return None;
    }

    Some(TerminalContext {
        pid: get_first_string(body, &field_keys.pid).and_then(|value| value.parse::<u32>().ok()),
        tty: get_first_string(body, &field_keys.tty),
        cwd,
        terminal_app,
        terminal_bundle_id: get_first_string(body, &field_keys.terminal_bundle_id),
        terminal_session_id,
        pane_title,
        warp_pane_uuid: get_first_string(body, &field_keys.warp_pane_uuid),
    })
}

fn expand_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn get_first_string(body: &Value, keys: &[String]) -> Option<String> {
    keys.iter().find_map(|key| {
        body.get(key)
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    })
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn transport_flag() -> &'static str {
    #[cfg(windows)]
    {
        "--pipe"
    }
    #[cfg(not(windows))]
    {
        "--socket"
    }
}

fn default_plugin_version() -> String {
    "1.0.0".into()
}

fn default_session_id_keys() -> Vec<String> {
    vec!["session_id".into(), "sessionId".into()]
}

fn default_provider_session_id_keys() -> Vec<String> {
    vec![
        "provider_session_id".into(),
        "providerSessionId".into(),
        "conversation_id".into(),
        "conversationId".into(),
    ]
}

fn default_transcript_path_keys() -> Vec<String> {
    vec!["transcript_path".into(), "transcriptPath".into()]
}

fn default_pid_keys() -> Vec<String> {
    vec!["pid".into(), "process_id".into(), "processId".into()]
}

fn default_tty_keys() -> Vec<String> {
    vec!["tty".into(), "terminal_tty".into(), "terminalTty".into()]
}

fn default_cwd_keys() -> Vec<String> {
    vec![
        "cwd".into(),
        "working_directory".into(),
        "workingDirectory".into(),
    ]
}

fn default_terminal_app_keys() -> Vec<String> {
    vec!["terminal_app".into(), "terminalApp".into()]
}

fn default_terminal_bundle_id_keys() -> Vec<String> {
    vec!["terminal_bundle_id".into(), "terminalBundleId".into()]
}

fn default_terminal_session_id_keys() -> Vec<String> {
    vec![
        "terminal_session_id".into(),
        "terminalSessionId".into(),
        "terminal_id".into(),
        "terminalId".into(),
        "pane_uuid".into(),
        "paneUuid".into(),
    ]
}

fn default_pane_title_keys() -> Vec<String> {
    vec!["pane_title".into(), "paneTitle".into(), "title".into()]
}

fn default_warp_pane_uuid_keys() -> Vec<String> {
    vec!["warp_pane_uuid".into(), "warpPaneUuid".into()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn builtins_expose_session_footer_slots() {
        let plugins = builtin_plugins();
        assert!(
            plugins
                .iter()
                .flat_map(|plugin| plugin.slots.iter())
                .any(|slot| slot.slot == "session_footer")
        );
    }

    #[test]
    fn manifest_event_mapping_builds_prompt_event() {
        let mapping = ManifestEventMapping {
            hook_event: "user_prompt".into(),
            event_type: ManifestEventType::UserPrompt,
            tool_key: None,
            input_key: None,
            text_key: Some("prompt".into()),
            success_key: None,
            question_key: None,
            options_key: None,
            request_id_key: None,
            title_key: None,
            body_key: None,
        };
        let event = mapping_to_event(
            &mapping,
            &serde_json::json!({ "prompt": "Ship the plugin manager" }),
            "s1",
            1,
        )
        .expect("event");

        match event {
            IslandEventType::UserPrompt { text } => assert_eq!(text, "Ship the plugin manager"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn loads_plugin_bundle_and_installs_from_contents_file() {
        let root = std::env::temp_dir().join(format!(
            "yorling-plugin-bundle-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let bundle_dir = root.join("demo.yorling-plugin");
        let resources_dir = bundle_dir.join("resources");
        let install_target = root.join("installed-hook.sh");

        std::fs::create_dir_all(&resources_dir).unwrap();
        std::fs::write(
            resources_dir.join("hook.sh"),
            "bridge={bridge_path}\ntransport={transport_endpoint}\nflag={transport_flag}\n",
        )
        .unwrap();
        std::fs::write(
            bundle_dir.join("plugin.json"),
            format!(
                r#"{{
  "id": "demo.bundle",
  "name": "Demo Bundle",
  "slots": [],
  "provider": {{
    "id": "demo-provider",
    "display_name": "Demo Provider",
    "install": {{
      "path": "{}",
      "contents_file": "resources/hook.sh"
    }}
  }}
}}"#,
                install_target.display().to_string().replace('\\', "\\\\")
            ),
        )
        .unwrap();

        let loaded = load_plugins(std::slice::from_ref(&root));
        assert!(
            loaded
                .plugins
                .iter()
                .any(|plugin| plugin.id == "demo.bundle")
        );

        let provider = loaded
            .providers
            .into_iter()
            .find(|provider| provider.id() == "demo-provider")
            .expect("provider");
        provider
            .install_hooks(
                Path::new("/tmp/yorling-bridge"),
                Path::new("/tmp/yorling-island.sock"),
            )
            .await
            .expect("install hooks");

        let installed = tokio::fs::read_to_string(&install_target).await.unwrap();
        assert!(installed.contains("/tmp/yorling-bridge"));
        assert!(installed.contains("/tmp/yorling-island.sock"));
        assert!(installed.contains("--socket"));

        let _ = std::fs::remove_dir_all(root);
    }
}
