use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::event::{Decision, IslandEvent, RawHookPayload};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookStatus {
    Installed,
    NotInstalled,
    Outdated,
    Broken { reason: String },
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{reason}")]
pub struct SkipHookEvent {
    reason: String,
}

impl SkipHookEvent {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOrigin {
    BuiltIn,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookHealthSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookHealthIssue {
    pub code: String,
    pub message: String,
    pub severity: HookHealthSeverity,
    pub repairable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookHealthReport {
    pub healthy: bool,
    pub issues: Vec<HookHealthIssue>,
    pub config_paths: Vec<String>,
    pub bridge_path: Option<String>,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub hook_status: HookStatus,
    pub supports_blocking: bool,
    pub origin: ProviderOrigin,
    pub plugin_id: Option<String>,
    pub manages_hooks: bool,
    pub config_paths: Vec<String>,
    pub health_report: HookHealthReport,
    pub slot_count: usize,
}

/// Trait for agent providers (Claude Code, Codex, Gemini, etc.)
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Unique identifier, e.g. "claude-code"
    fn id(&self) -> &str;

    /// Display name for the UI
    fn display_name(&self) -> &str;

    /// Install hooks into the agent's configuration, pointing to the bridge binary
    async fn install_hooks(
        &self,
        bridge_path: &Path,
        transport_endpoint: &Path,
    ) -> anyhow::Result<()>;

    /// Remove hooks from the agent's configuration
    async fn uninstall_hooks(&self) -> anyhow::Result<()>;

    /// Check current hook installation status
    async fn verify_hooks(&self, bridge_path: &Path, transport_endpoint: &Path) -> HookStatus;

    /// Convert a raw hook payload to a normalized IslandEvent
    fn normalize_event(&self, raw: &RawHookPayload) -> anyhow::Result<IslandEvent>;

    /// Whether this provider supports blocking permission requests
    fn supports_blocking_permission(&self) -> bool;

    /// Encode a permission decision into the provider's native hook response format.
    fn encode_permission_response(&self, decision: Decision) -> anyhow::Result<Vec<u8>>;

    /// Encode a question answer into the provider's native hook response format.
    fn encode_question_response(
        &self,
        answer: &str,
        from_permission: bool,
        answer_key: Option<&str>,
    ) -> anyhow::Result<Vec<u8>>;

    /// Where this provider came from.
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::BuiltIn
    }

    /// External plugin identifier when applicable.
    fn plugin_id(&self) -> Option<&str> {
        None
    }

    /// Config or managed install paths used for diagnostics.
    fn config_paths(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Paths whose existence suggests the agent is installed or has been initialized locally.
    fn detection_paths(&self) -> Vec<PathBuf> {
        self.config_paths()
    }

    /// Commands whose availability on PATH suggests the agent is installed.
    fn detection_commands(&self) -> &'static [&'static str] {
        &[]
    }

    /// Whether this provider appears to be present on the current machine.
    fn is_available_on_system(&self) -> bool {
        self.detection_paths().into_iter().any(|path| path.exists())
            || self
                .detection_commands()
                .iter()
                .copied()
                .any(command_in_path)
    }

    /// Whether Yorling can install/uninstall hooks for this provider automatically.
    fn manages_hooks(&self) -> bool {
        true
    }

    /// Number of UI slot contributions owned by this provider's plugin.
    fn slot_count(&self) -> usize {
        0
    }
}

/// Registry of all known agent providers.
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn AgentProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Box<dyn AgentProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn get(&self, id: &str) -> Option<&dyn AgentProvider> {
        self.providers.get(id).map(|p| p.as_ref())
    }

    pub fn list(&self) -> Vec<&dyn AgentProvider> {
        self.providers.values().map(|p| p.as_ref()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_health_report(
    provider: &dyn AgentProvider,
    hook_status: &HookStatus,
    bridge_path: Result<&Path, &str>,
) -> HookHealthReport {
    let checked_at = current_timestamp();
    let mut issues = Vec::new();
    let config_paths = provider
        .config_paths()
        .into_iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();

    let bridge_path_string = match bridge_path {
        Ok(path) => {
            if !path.is_file() {
                issues.push(HookHealthIssue {
                    code: "bridge_missing".into(),
                    message: format!("Bridge binary is missing at {}", path.display()),
                    severity: HookHealthSeverity::Error,
                    repairable: false,
                });
            } else if !is_executable(path) {
                issues.push(HookHealthIssue {
                    code: "bridge_not_executable".into(),
                    message: format!("Bridge binary is not executable at {}", path.display()),
                    severity: HookHealthSeverity::Error,
                    repairable: true,
                });
            }

            Some(path.display().to_string())
        }
        Err(error) => {
            issues.push(HookHealthIssue {
                code: "bridge_unavailable".into(),
                message: error.to_string(),
                severity: HookHealthSeverity::Error,
                repairable: false,
            });
            None
        }
    };

    for config_path in &config_paths {
        let path = Path::new(config_path);
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            continue;
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => {
                let is_json = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
                if is_json && serde_json::from_str::<serde_json::Value>(&contents).is_err() {
                    issues.push(HookHealthIssue {
                        code: "config_malformed".into(),
                        message: format!("Config file is not valid JSON: {}", path.display()),
                        severity: HookHealthSeverity::Error,
                        repairable: false,
                    });
                }
            }
            Err(error) => issues.push(HookHealthIssue {
                code: "config_unreadable".into(),
                message: format!("Cannot read config {}: {}", path.display(), error),
                severity: HookHealthSeverity::Error,
                repairable: false,
            }),
        }
    }

    match hook_status {
        HookStatus::Installed => {}
        HookStatus::NotInstalled => issues.push(HookHealthIssue {
            code: "hooks_not_installed".into(),
            message: "Managed hooks are not installed yet.".into(),
            severity: HookHealthSeverity::Warning,
            repairable: provider.manages_hooks(),
        }),
        HookStatus::Outdated => issues.push(HookHealthIssue {
            code: "hooks_outdated".into(),
            message: "Hook configuration is outdated and should be reinstalled.".into(),
            severity: HookHealthSeverity::Warning,
            repairable: provider.manages_hooks(),
        }),
        HookStatus::Broken { reason } => issues.push(HookHealthIssue {
            code: "hooks_broken".into(),
            message: reason.clone(),
            severity: HookHealthSeverity::Error,
            repairable: provider.manages_hooks(),
        }),
    }

    let healthy = !issues.iter().any(|issue| {
        matches!(
            issue.severity,
            HookHealthSeverity::Error | HookHealthSeverity::Warning
        )
    });

    HookHealthReport {
        healthy,
        issues,
        config_paths,
        bridge_path: bridge_path_string,
        checked_at,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use async_trait::async_trait;

    use crate::event::{Decision, IslandEvent, RawHookPayload};

    use super::*;

    struct DirectoryConfigProvider;

    #[async_trait]
    impl AgentProvider for DirectoryConfigProvider {
        fn id(&self) -> &str {
            "directory-config"
        }

        fn display_name(&self) -> &str {
            "DirectoryConfig"
        }

        async fn install_hooks(
            &self,
            _bridge_path: &Path,
            _transport_endpoint: &Path,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn uninstall_hooks(&self) -> anyhow::Result<()> {
            Ok(())
        }

        async fn verify_hooks(
            &self,
            _bridge_path: &Path,
            _transport_endpoint: &Path,
        ) -> HookStatus {
            HookStatus::Installed
        }

        fn normalize_event(&self, _raw: &RawHookPayload) -> anyhow::Result<IslandEvent> {
            anyhow::bail!("unused")
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
            let root = std::env::temp_dir().join(format!(
                "yorling-provider-dir-config-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let directory = root.join("managed-dir");
            let file = root.join("managed.json");
            std::fs::create_dir_all(&directory).expect("directory");
            std::fs::write(&file, "{}").expect("file");
            vec![directory, file]
        }
    }

    #[test]
    fn build_health_report_ignores_directory_paths() {
        let provider = DirectoryConfigProvider;
        let bridge = std::env::current_exe().expect("current exe");
        let report = build_health_report(&provider, &HookStatus::Installed, Ok(bridge.as_path()));

        assert!(
            report
                .issues
                .iter()
                .all(|issue| issue.code != "config_unreadable")
        );
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn command_in_path(command: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };

    std::env::split_paths(&path_var)
        .flat_map(|dir| command_candidates(&dir, command))
        .any(|candidate| is_executable(&candidate))
}

fn command_candidates(dir: &Path, command: &str) -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".into());
        let extensions = pathext
            .split(';')
            .filter(|ext| !ext.is_empty())
            .collect::<Vec<_>>();

        if command.contains('.') {
            return vec![dir.join(command)];
        }

        let mut candidates = Vec::with_capacity(extensions.len() + 1);
        candidates.push(dir.join(command));
        candidates.extend(
            extensions
                .into_iter()
                .map(|ext| dir.join(format!("{command}{ext}"))),
        );
        candidates
    }

    #[cfg(not(windows))]
    {
        vec![dir.join(command)]
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
