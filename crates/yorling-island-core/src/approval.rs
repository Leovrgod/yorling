use std::path::PathBuf;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// A single persisted approval rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub provider_id: String,
    pub tool_name: String,
    /// Deterministic JSON signature of the tool input for exact matching.
    /// `None` means "match any input for this tool".
    pub input_signature: Option<String>,
    pub policy: ApprovalPolicy,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalPolicy {
    AllowAlways,
    DenyAlways,
}

/// Persistent store for "Allow Always" / "Deny Always" approval rules.
pub struct ApprovalPolicyStore {
    rules: RwLock<Vec<ApprovalRule>>,
    file_path: PathBuf,
}

impl ApprovalPolicyStore {
    pub fn new() -> Self {
        let file_path = Self::default_path();
        let rules = Self::load_from_disk(&file_path);
        Self {
            rules: RwLock::new(rules),
            file_path,
        }
    }

    fn default_path() -> PathBuf {
        let base = std::env::var_os("YORLING_DATA_DIR")
            .map(PathBuf::from)
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("yorling").join("approval-policies.json")
    }

    fn load_from_disk(path: &PathBuf) -> Vec<ApprovalRule> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                warn!(
                    "Failed to parse approval policies from {}: {}",
                    path.display(),
                    e
                );
                Vec::new()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => {
                warn!(
                    "Failed to read approval policies from {}: {}",
                    path.display(),
                    e
                );
                Vec::new()
            }
        }
    }

    fn persist(&self, rules: &[ApprovalRule]) {
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string_pretty(rules) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.file_path, json) {
                    warn!("Failed to persist approval policies: {}", e);
                }
            }
            Err(e) => warn!("Failed to serialize approval policies: {}", e),
        }
    }

    /// Compute a deterministic signature for tool input.
    pub fn input_signature(input: &serde_json::Value) -> String {
        normalize_and_hash(input)
    }

    /// Add or update an approval rule. Most recent rule wins for matching.
    pub fn add_rule(
        &self,
        provider_id: &str,
        tool_name: &str,
        input: &serde_json::Value,
        policy: ApprovalPolicy,
    ) {
        let signature = Self::input_signature(input);
        let rule = ApprovalRule {
            provider_id: provider_id.to_string(),
            tool_name: tool_name.to_string(),
            input_signature: Some(signature),
            policy,
            created_at: current_timestamp(),
        };

        let mut rules = self.rules.write().unwrap();
        // Remove any existing rule with same provider/tool/signature
        rules.retain(|r| {
            !(r.provider_id == rule.provider_id
                && r.tool_name == rule.tool_name
                && r.input_signature == rule.input_signature)
        });
        rules.push(rule);
        self.persist(&rules);
        info!(
            "Approval rule added: {} / {} -> {:?}",
            provider_id, tool_name, policy
        );
    }

    /// Check if there's a matching policy for the given permission request.
    /// Returns the most recent matching rule's policy.
    pub fn matching_policy(
        &self,
        provider_id: &str,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Option<ApprovalPolicy> {
        let signature = Self::input_signature(input);
        let rules = self.rules.read().unwrap();

        // Find exact match (provider + tool + input signature)
        let exact = rules
            .iter()
            .rev()
            .find(|r| {
                r.provider_id == provider_id
                    && r.tool_name == tool_name
                    && r.input_signature.as_deref() == Some(signature.as_str())
            })
            .map(|r| r.policy);

        if exact.is_some() {
            return exact;
        }

        // Find tool-level match (provider + tool, any input)
        rules
            .iter()
            .rev()
            .find(|r| {
                r.provider_id == provider_id
                    && r.tool_name == tool_name
                    && r.input_signature.is_none()
            })
            .map(|r| r.policy)
    }

    /// Get all stored rules (for UI display).
    pub fn all_rules(&self) -> Vec<ApprovalRule> {
        self.rules.read().unwrap().clone()
    }

    /// Remove a specific rule by index.
    pub fn remove_rule(&self, index: usize) {
        let mut rules = self.rules.write().unwrap();
        if index < rules.len() {
            rules.remove(index);
            self.persist(&rules);
        }
    }

    /// Clear all rules.
    pub fn clear_all(&self) {
        let mut rules = self.rules.write().unwrap();
        rules.clear();
        self.persist(&rules);
    }
}

/// Normalize a JSON value for deterministic comparison:
/// sort object keys recursively, then serialize with sorted keys.
fn normalize_and_hash(value: &serde_json::Value) -> String {
    let normalized = normalize_value(value);
    // Use the normalized JSON as the signature directly (deterministic)
    serde_json::to_string(&normalized).unwrap_or_default()
}

fn normalize_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut sorted: Vec<_> = map.iter().collect();
            sorted.sort_by_key(|(k, _)| (*k).clone());
            let normalized_map: serde_json::Map<String, serde_json::Value> = sorted
                .into_iter()
                .map(|(k, v)| (k.clone(), normalize_value(v)))
                .collect();
            serde_json::Value::Object(normalized_map)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(normalize_value).collect())
        }
        other => other.clone(),
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
    use super::*;

    #[test]
    fn input_signature_is_deterministic() {
        let input1 = serde_json::json!({"b": 2, "a": 1});
        let input2 = serde_json::json!({"a": 1, "b": 2});
        assert_eq!(
            ApprovalPolicyStore::input_signature(&input1),
            ApprovalPolicyStore::input_signature(&input2),
        );
    }

    #[test]
    fn matching_policy_returns_most_recent() {
        let store = ApprovalPolicyStore {
            rules: RwLock::new(Vec::new()),
            file_path: PathBuf::from("/tmp/test-approval-policies.json"),
        };
        let input = serde_json::json!({"command": "rm -rf /"});

        store.add_rule("claude-code", "Bash", &input, ApprovalPolicy::AllowAlways);
        assert_eq!(
            store.matching_policy("claude-code", "Bash", &input),
            Some(ApprovalPolicy::AllowAlways),
        );

        // Different input should not match
        let other_input = serde_json::json!({"command": "ls"});
        assert_eq!(
            store.matching_policy("claude-code", "Bash", &other_input),
            None,
        );

        let _ = std::fs::remove_file("/tmp/test-approval-policies.json");
    }
}
