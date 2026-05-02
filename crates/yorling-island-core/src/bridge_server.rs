use std::{
    collections::HashSet,
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, UNIX_EPOCH},
};

use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::approval::{ApprovalPolicy, ApprovalPolicyStore};
use crate::event::{Decision, IslandEvent, IslandEventType, RawHookPayload, TerminalContext};
use crate::provider::{ProviderRegistry, SkipHookEvent};
use crate::session::SessionState;
use crate::store::SessionStore;
use crate::transcript::{
    TranscriptManager, TranscriptPreview, is_codex_title_generation_prompt,
    latest_preview_activity_timestamp,
};
use crate::transport::BridgeMessage;

const TRANSCRIPT_DISCOVERY_WINDOW_MS: i64 = 90 * 60 * 1000;
const TRANSCRIPT_DISCOVERY_REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const MAX_DISCOVERED_TRANSCRIPTS: usize = 8;
const TRANSCRIPT_ACTIVITY_SCAN_BYTES: u64 = 256 * 1024;
const TRANSCRIPT_ACTIVITY_MAX_LINES: usize = 256;
const TRANSCRIPT_TIMESTAMP_KEY_PATTERNS: &[&str] = &[
    "\"timestamp\"",
    "\"created_at\"",
    "\"createdAt\"",
    "\"updated_at\"",
    "\"updatedAt\"",
    "\"finished_at\"",
    "\"finishedAt\"",
    "\"started_at\"",
    "\"startedAt\"",
    "\"ts\"",
];

/// The bridge server connects the transport layer to the session store.
/// It receives raw messages from agent hooks, normalizes them via providers,
/// and manages blocking request/response flows.
pub struct BridgeServer {
    store: Arc<SessionStore>,
    registry: Arc<ProviderRegistry>,
    transcript_manager: Arc<TranscriptManager>,
    approval_store: Arc<ApprovalPolicyStore>,
    transcript_discovery_cache: Arc<Mutex<TranscriptDiscoveryCache>>,
    /// Maps request_id → reply channel for blocking responses
    pending_replies: Arc<DashMap<String, mpsc::Sender<Vec<u8>>>>,
}

impl BridgeServer {
    pub fn new(store: Arc<SessionStore>, registry: Arc<ProviderRegistry>) -> Self {
        Self {
            store,
            registry,
            transcript_manager: Arc::new(TranscriptManager::new()),
            approval_store: Arc::new(ApprovalPolicyStore::new()),
            transcript_discovery_cache: Arc::new(Mutex::new(TranscriptDiscoveryCache::default())),
            pending_replies: Arc::new(DashMap::new()),
        }
    }

    /// Get a reference to the approval policy store.
    pub fn approval_store(&self) -> &Arc<ApprovalPolicyStore> {
        &self.approval_store
    }

    /// Refresh known sessions from transcript files and discover recent sessions
    /// that were started before the bridge came online.
    pub async fn refresh_sessions_from_transcripts(&self) {
        let mut seen_sessions = HashSet::new();

        for session in self.store.all_sessions() {
            seen_sessions.insert((session.provider_id.clone(), session.id.clone()));

            let transcript_path = session
                .transcript_preview
                .as_ref()
                .map(|preview| preview.transcript_path.clone())
                .filter(|path| !path.trim().is_empty())
                .or_else(|| {
                    infer_transcript_path(
                        &session.provider_id,
                        &session.id,
                        session.provider_session_id.as_deref(),
                    )
                });

            if let Some(transcript_path) = transcript_path {
                self.hydrate_transcript_snapshot(
                    &session.id,
                    &session.provider_id,
                    &transcript_path,
                    session.provider_session_id.clone(),
                    session.terminal.clone(),
                )
                .await;
            }
        }

        for candidate in self.discover_recent_transcripts_cached() {
            if !seen_sessions.insert((candidate.provider_id.clone(), candidate.session_id.clone()))
            {
                continue;
            }

            self.hydrate_transcript_snapshot(
                &candidate.session_id,
                &candidate.provider_id,
                &candidate.transcript_path,
                candidate.provider_session_id,
                candidate.terminal_context,
            )
            .await;
        }
    }

    fn discover_recent_transcripts_cached(&self) -> Vec<DiscoveredTranscript> {
        let mut cache = self
            .transcript_discovery_cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.candidates()
    }

    /// Start processing messages from the transport. Runs until the receiver is closed.
    pub async fn run(&self, mut message_rx: mpsc::Receiver<BridgeMessage>) {
        info!("BridgeServer started");
        while let Some(msg) = message_rx.recv().await {
            self.handle_message(msg).await;
        }
        info!("BridgeServer stopped");
    }

    async fn handle_message(&self, msg: BridgeMessage) {
        // Parse the raw JSON
        let raw: RawHookPayload = match serde_json::from_slice(&msg.data) {
            Ok(raw) => raw,
            Err(e) => {
                warn!("Failed to parse bridge message: {}", e);
                // Close the reply channel so bridge doesn't hang
                drop(msg.reply_tx);
                return;
            }
        };

        // Find the provider and normalize
        let provider = match self.registry.get(&raw.source) {
            Some(p) => p,
            None => {
                warn!("Unknown provider: {}", raw.source);
                drop(msg.reply_tx);
                return;
            }
        };

        let event = match provider.normalize_event(&raw) {
            Ok(event) => event,
            Err(e) => {
                if let Some(skip) = e.downcast_ref::<SkipHookEvent>() {
                    info!(
                        "Skipping {} event from {}: {}",
                        raw.hook_event, raw.source, skip
                    );
                    drop(msg.reply_tx);
                    return;
                }
                warn!("Failed to normalize event from {}: {}", raw.source, e);
                drop(msg.reply_tx);
                return;
            }
        };

        // If the agent already resolved an older blocking prompt outside Yorling,
        // the reply channel will be closed by now. Clean those entries before this
        // new event updates session state so stale waiting cards don't linger.
        self.cleanup_stale_replies();

        if matches!(event.event_type, IslandEventType::SessionDiscard) {
            drop(msg.reply_tx);
            self.store.apply_event(event);
            return;
        }

        // Check if this is a blocking request that needs a reply
        let is_blocking = matches!(
            &event.event_type,
            IslandEventType::PermissionRequest { .. } | IslandEventType::AskQuestion { .. }
        );

        // Extract request_id for blocking requests
        let request_id = match &event.event_type {
            IslandEventType::PermissionRequest { request_id, .. } => Some(request_id.clone()),
            IslandEventType::AskQuestion { request_id, .. } => Some(request_id.clone()),
            _ => None,
        };

        // Check for auto-approval before storing reply channel
        if let IslandEventType::PermissionRequest {
            ref tool,
            ref input,
            ref request_id,
        } = event.event_type
        {
            if let Some(policy) =
                self.approval_store
                    .matching_policy(&event.provider_id, tool, input)
            {
                let auto_decision = match policy {
                    ApprovalPolicy::AllowAlways => Decision::Allow,
                    ApprovalPolicy::DenyAlways => Decision::Deny,
                };
                info!(
                    "Auto-applying {:?} policy for {} / {} (request {})",
                    policy, event.provider_id, tool, request_id
                );

                // Encode and send the response directly
                if let Some(reply_tx) = msg.reply_tx {
                    if let Some(provider) = self.registry.get(&event.provider_id) {
                        if let Ok(response) =
                            provider.encode_permission_response(auto_decision.clone())
                        {
                            let _ = reply_tx.send(response).await;
                        }
                    }
                }

                // Apply the event and decision to the store
                let session_id = event.session_id.clone();
                let provider_id = event.provider_id.clone();
                let req_id = request_id.clone();
                self.store.apply_event(event);

                self.store.apply_event(IslandEvent {
                    session_id: session_id.clone(),
                    provider_id: provider_id.clone(),
                    timestamp: current_timestamp(),
                    event_type: IslandEventType::PermissionDecision {
                        request_id: req_id,
                        decision: auto_decision,
                    },
                    terminal_context: None,
                });

                // Still sync transcript
                self.sync_transcript(&session_id, &provider_id, &raw.body)
                    .await;
                return;
            }
        }

        // Store the reply channel for blocking requests
        if is_blocking {
            if let (Some(req_id), Some(reply_tx)) = (request_id, msg.reply_tx) {
                self.pending_replies.insert(req_id, reply_tx);
            }
        } else {
            // Non-blocking: close the reply channel immediately
            drop(msg.reply_tx);
        }

        // Apply event to the store (this also broadcasts to subscribers)
        let session_id = event.session_id.clone();
        let provider_id = event.provider_id.clone();
        self.store.apply_event(event);

        self.sync_transcript(&session_id, &provider_id, &raw.body)
            .await;
    }

    async fn sync_transcript(&self, session_id: &str, provider_id: &str, body: &serde_json::Value) {
        let provider_session_id = extract_provider_session_id(body).or_else(|| {
            self.store
                .get_session(session_id)
                .and_then(|session| session.provider_session_id)
        });
        let transcript_path = extract_transcript_path(body)
            .or_else(|| {
                self.store.get_session(session_id).and_then(|session| {
                    session
                        .transcript_preview
                        .map(|preview| preview.transcript_path)
                })
            })
            .or_else(|| {
                infer_transcript_path(provider_id, session_id, provider_session_id.as_deref())
            });

        if let Some(transcript_path) = transcript_path.filter(|path| !path.is_empty()) {
            self.hydrate_transcript_snapshot(
                session_id,
                provider_id,
                &transcript_path,
                provider_session_id,
                None,
            )
            .await;
        }
    }

    async fn hydrate_transcript_snapshot(
        &self,
        session_id: &str,
        provider_id: &str,
        transcript_path: &str,
        provider_session_id: Option<String>,
        terminal_context: Option<TerminalContext>,
    ) {
        let preview = self
            .transcript_manager
            .sync_with_provider(
                session_id,
                transcript_path,
                provider_session_id.as_deref(),
                provider_id,
            )
            .await;

        if preview.turn_count == 0
            && preview.tool_history.is_empty()
            && preview.provider_session_id.is_none()
        {
            return;
        }

        let existing_session = self.store.get_session(session_id);
        if existing_session.is_none() && should_skip_new_transcript_session(provider_id, &preview) {
            return;
        }

        let preview_changed = existing_session
            .as_ref()
            .and_then(|session| session.transcript_preview.as_ref())
            .is_none_or(|existing| !same_preview_payload(existing, &preview));
        let context_adds_information =
            existing_session
                .as_ref()
                .map_or(terminal_context.is_some(), |session| {
                    terminal_context_adds_information(
                        session.terminal.as_ref(),
                        terminal_context.as_ref(),
                    )
                });

        if !preview_changed && !context_adds_information {
            return;
        }

        self.store.apply_event(IslandEvent {
            session_id: session_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: latest_preview_activity_timestamp(&preview)
                .unwrap_or_else(current_timestamp),
            event_type: IslandEventType::TranscriptSync { preview },
            terminal_context,
        });
    }

    /// Send a permission decision response back to the bridge.
    pub async fn respond_permission(
        &self,
        request_id: &str,
        decision: crate::event::Decision,
    ) -> anyhow::Result<()> {
        let session = self
            .find_session_with_pending_permission(request_id)
            .ok_or_else(|| {
                anyhow::anyhow!("No session found for pending permission: {}", request_id)
            })?;
        let provider = self
            .registry
            .get(&session.provider_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", session.provider_id))?;

        // If AllowAlways, persist the rule before responding
        if decision == Decision::AllowAlways {
            if let Some(ref pending) = session.pending_permission {
                self.approval_store.add_rule(
                    &session.provider_id,
                    &pending.tool,
                    &pending.input,
                    ApprovalPolicy::AllowAlways,
                );
            }
        }

        let response = provider.encode_permission_response(decision.clone())?;

        match self.send_reply(request_id, response).await {
            Ok(()) => {}
            Err(e) => {
                // The bridge connection has timed out or been lost.
                // Clean up the stale pending state so the UI doesn't show a dead approval card.
                warn!(
                    "Permission reply failed for request {}: {}. Clearing stale pending state.",
                    request_id, e
                );
                self.clear_stale_permission(&session.id, &session.provider_id, request_id);
                return Err(anyhow::anyhow!(
                    "The approval request has expired — the agent is no longer waiting for a response. \
                     Please check the terminal for the agent's current state."
                ));
            }
        }

        // Also apply the decision event to the store
        let event = IslandEvent {
            session_id: session.id.clone(),
            provider_id: session.provider_id.clone(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::PermissionDecision {
                request_id: request_id.to_string(),
                decision,
            },
            terminal_context: None,
        };
        self.store.apply_event(event);

        Ok(())
    }

    /// Send a question answer response back to the bridge.
    pub async fn respond_question(&self, request_id: &str, answer: String) -> anyhow::Result<()> {
        let session = self
            .find_session_with_pending_question(request_id)
            .ok_or_else(|| {
                anyhow::anyhow!("No session found for pending question: {}", request_id)
            })?;
        let pending_question = session.pending_question.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Question state disappeared for request: {}", request_id)
        })?;
        let provider = self
            .registry
            .get(&session.provider_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", session.provider_id))?;
        let response = provider.encode_question_response(
            &answer,
            pending_question.from_permission,
            pending_question.answer_key.as_deref(),
        )?;

        match self.send_reply(request_id, response).await {
            Ok(()) => {}
            Err(e) => {
                warn!(
                    "Question reply failed for request {}: {}. Clearing stale pending state.",
                    request_id, e
                );
                self.clear_stale_question(&session.id, &session.provider_id, request_id);
                return Err(anyhow::anyhow!(
                    "The question request has expired — the agent is no longer waiting for a response. \
                     Please check the terminal for the agent's current state."
                ));
            }
        }

        let event = IslandEvent {
            session_id: session.id.clone(),
            provider_id: session.provider_id.clone(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::QuestionAnswer {
                request_id: request_id.to_string(),
                answer,
            },
            terminal_context: None,
        };
        self.store.apply_event(event);

        Ok(())
    }

    async fn send_reply(&self, request_id: &str, response: Vec<u8>) -> anyhow::Result<()> {
        let reply_tx = self
            .pending_replies
            .remove(request_id)
            .map(|(_, tx)| tx)
            .ok_or_else(|| anyhow::anyhow!("No pending request with id: {}", request_id))?;

        reply_tx
            .send(response)
            .await
            .map_err(|_| anyhow::anyhow!("Reply channel closed for request: {}", request_id))?;

        Ok(())
    }

    fn find_session_with_pending_permission(&self, request_id: &str) -> Option<SessionState> {
        self.store.all_sessions().into_iter().find(|s| {
            s.pending_permission
                .as_ref()
                .is_some_and(|p| p.request_id == request_id)
        })
    }

    fn find_session_with_pending_question(&self, request_id: &str) -> Option<SessionState> {
        self.store.all_sessions().into_iter().find(|s| {
            s.pending_question
                .as_ref()
                .is_some_and(|q| q.request_id == request_id)
        })
    }

    /// Clear a stale permission from session state when the bridge connection has been lost.
    fn clear_stale_permission(&self, session_id: &str, provider_id: &str, request_id: &str) {
        // Emit a PermissionDecision(Deny) to clear the pending state
        self.store.apply_event(IslandEvent {
            session_id: session_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::PermissionDecision {
                request_id: request_id.to_string(),
                decision: Decision::Deny,
            },
            terminal_context: None,
        });
        // Notify the user that the request expired
        self.store.apply_event(IslandEvent {
            session_id: session_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::Notification {
                title: "Approval Expired".to_string(),
                body: "The approval request has expired because the agent stopped waiting."
                    .to_string(),
            },
            terminal_context: None,
        });
    }

    /// Clear a stale question from session state when the bridge connection has been lost.
    fn clear_stale_question(&self, session_id: &str, provider_id: &str, request_id: &str) {
        self.store.apply_event(IslandEvent {
            session_id: session_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::QuestionAnswer {
                request_id: request_id.to_string(),
                answer: String::new(),
            },
            terminal_context: None,
        });
        self.store.apply_event(IslandEvent {
            session_id: session_id.to_string(),
            provider_id: provider_id.to_string(),
            timestamp: current_timestamp(),
            event_type: IslandEventType::Notification {
                title: "Question Expired".to_string(),
                body: "The question has expired because the agent stopped waiting.".to_string(),
            },
            terminal_context: None,
        });
    }

    /// Proactively clean up stale pending replies whose channels have been dropped.
    /// This catches cases where the transport connection timed out but the user
    /// hasn't clicked the approval button yet.
    pub fn cleanup_stale_replies(&self) {
        let stale_request_ids: Vec<String> = self
            .pending_replies
            .iter()
            .filter(|entry| entry.value().is_closed())
            .map(|entry| entry.key().clone())
            .collect();

        for request_id in stale_request_ids {
            info!("Cleaning up stale pending reply: {}", request_id);
            self.pending_replies.remove(&request_id);

            // Clear corresponding session state
            if let Some(session) = self.find_session_with_pending_permission(&request_id) {
                self.clear_stale_permission(&session.id, &session.provider_id, &request_id);
            } else if let Some(session) = self.find_session_with_pending_question(&request_id) {
                self.clear_stale_question(&session.id, &session.provider_id, &request_id);
            }
        }
    }
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn extract_transcript_path(body: &serde_json::Value) -> Option<String> {
    for key in [
        "transcript_path",
        "transcriptPath",
        "session_file_path",
        "sessionFilePath",
    ] {
        if let Some(path) = body.get(key).and_then(|value| value.as_str()) {
            if !path.trim().is_empty() {
                return Some(path.to_string());
            }
        }
    }

    None
}

fn extract_provider_session_id(body: &serde_json::Value) -> Option<String> {
    for key in [
        "provider_session_id",
        "providerSessionId",
        "conversation_id",
        "conversationId",
        "session_id",
        "sessionId",
    ] {
        if let Some(value) = body.get(key).and_then(|value| value.as_str()) {
            if !value.trim().is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

fn infer_transcript_path(
    provider_id: &str,
    session_id: &str,
    provider_session_id: Option<&str>,
) -> Option<String> {
    match provider_id {
        "copilot" => infer_copilot_transcript_path(provider_session_id.unwrap_or(session_id)),
        "codex" => infer_codex_transcript_path(provider_session_id.unwrap_or(session_id)),
        "openclaw" => infer_openclaw_transcript_path(provider_session_id.unwrap_or(session_id)),
        _ => None,
    }
}

fn infer_copilot_transcript_path(session_id: &str) -> Option<String> {
    let root = copilot_sessions_root()?;
    let transcript_path = root.join(session_id).join("events.jsonl");
    transcript_path
        .is_file()
        .then(|| transcript_path.display().to_string())
}

fn infer_codex_transcript_path(session_id: &str) -> Option<String> {
    list_recent_codex_transcript_files()
        .into_iter()
        .find(|(path, _)| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.contains(session_id))
        })
        .map(|(path, _)| path.display().to_string())
}

fn infer_openclaw_transcript_path(session_id: &str) -> Option<String> {
    let root = openclaw_sessions_root()?;
    let transcript_path = root.join(format!("{session_id}.jsonl"));
    transcript_path
        .is_file()
        .then(|| transcript_path.display().to_string())
}

fn same_preview_payload(left: &TranscriptPreview, right: &TranscriptPreview) -> bool {
    left.transcript_path == right.transcript_path
        && left.provider_session_id == right.provider_session_id
        && left.status == right.status
        && left.turn_count == right.turn_count
        && left.latest_task_started_at == right.latest_task_started_at
        && left.latest_task_finished_at == right.latest_task_finished_at
        && left.latest_pending_question == right.latest_pending_question
        && left.last_error == right.last_error
        && left.chat_preview == right.chat_preview
        && left.tool_history == right.tool_history
}

fn should_skip_new_transcript_session(provider_id: &str, preview: &TranscriptPreview) -> bool {
    if provider_id != "codex" || preview.latest_pending_question.is_some() {
        return false;
    }

    let Some(finished_at) = preview.latest_task_finished_at else {
        return false;
    };

    !preview
        .latest_task_started_at
        .is_some_and(|started_at| started_at > finished_at)
}

fn terminal_context_adds_information(
    existing: Option<&TerminalContext>,
    incoming: Option<&TerminalContext>,
) -> bool {
    let Some(incoming) = incoming else {
        return false;
    };
    let Some(existing) = existing else {
        return true;
    };

    (existing.pid.is_none() && incoming.pid.is_some())
        || (existing.tty.is_none() && incoming.tty.is_some())
        || (existing.cwd.is_none() && incoming.cwd.is_some())
        || (existing.terminal_app.is_none() && incoming.terminal_app.is_some())
        || (existing.terminal_bundle_id.is_none() && incoming.terminal_bundle_id.is_some())
        || (existing.terminal_session_id.is_none() && incoming.terminal_session_id.is_some())
        || (existing.pane_title.is_none() && incoming.pane_title.is_some())
        || (existing.warp_pane_uuid.is_none() && incoming.warp_pane_uuid.is_some())
}

#[derive(Debug, Clone)]
struct DiscoveredTranscript {
    session_id: String,
    provider_id: String,
    transcript_path: String,
    provider_session_id: Option<String>,
    terminal_context: Option<TerminalContext>,
    last_activity: i64,
}

#[derive(Debug, Default)]
struct TranscriptDiscoveryCache {
    last_refresh: Option<Instant>,
    candidates: Vec<DiscoveredTranscript>,
}

impl TranscriptDiscoveryCache {
    fn candidates(&mut self) -> Vec<DiscoveredTranscript> {
        let should_refresh = match self.last_refresh {
            Some(last_refresh) => last_refresh.elapsed() >= TRANSCRIPT_DISCOVERY_REFRESH_INTERVAL,
            None => true,
        };

        if should_refresh {
            self.candidates = discover_recent_transcripts();
            self.last_refresh = Some(Instant::now());
        }

        self.candidates.clone()
    }
}

fn discover_recent_transcripts() -> Vec<DiscoveredTranscript> {
    let mut discovered = discover_recent_copilot_transcripts();
    discovered.extend(discover_recent_codex_transcripts());
    discovered.extend(discover_recent_openclaw_transcripts());
    discovered.sort_by(|left, right| right.last_activity.cmp(&left.last_activity));

    let mut seen = HashSet::new();
    discovered.retain(|candidate| {
        seen.insert((candidate.provider_id.clone(), candidate.session_id.clone()))
    });
    discovered.truncate(MAX_DISCOVERED_TRANSCRIPTS);
    discovered
}

fn discover_recent_copilot_transcripts() -> Vec<DiscoveredTranscript> {
    let Some(root) = copilot_sessions_root() else {
        return Vec::new();
    };
    discover_recent_copilot_transcripts_in(&root)
}

fn discover_recent_copilot_transcripts_in(root: &Path) -> Vec<DiscoveredTranscript> {
    list_sorted_children(&root, true)
        .into_iter()
        .filter_map(|dir| {
            let session_id = dir.file_name()?.to_string_lossy().to_string();
            let transcript_path = dir.join("events.jsonl");
            let last_activity = transcript_activity_timestamp(&transcript_path)?;
            if current_timestamp() - last_activity > TRANSCRIPT_DISCOVERY_WINDOW_MS {
                return None;
            }

            Some(DiscoveredTranscript {
                session_id: session_id.clone(),
                provider_id: "copilot".into(),
                transcript_path: transcript_path.display().to_string(),
                provider_session_id: Some(session_id),
                terminal_context: None,
                last_activity,
            })
        })
        .collect()
}

fn discover_recent_codex_transcripts() -> Vec<DiscoveredTranscript> {
    list_recent_codex_transcript_files()
        .into_iter()
        .filter_map(|(path, last_activity)| {
            let metadata = read_codex_session_meta(&path)?;
            let session_id = get_first_string(&metadata, &["id"])?;

            Some(DiscoveredTranscript {
                session_id: session_id.clone(),
                provider_id: "codex".into(),
                transcript_path: path.display().to_string(),
                provider_session_id: Some(session_id),
                terminal_context: metadata
                    .get("cwd")
                    .and_then(|value| value.as_str())
                    .map(|cwd| TerminalContext {
                        pid: None,
                        tty: None,
                        cwd: Some(cwd.to_string()),
                        terminal_app: None,
                        terminal_bundle_id: None,
                        terminal_session_id: None,
                        pane_title: None,
                        warp_pane_uuid: None,
                    }),
                last_activity,
            })
        })
        .collect()
}

fn list_recent_codex_transcript_files() -> Vec<(PathBuf, i64)> {
    let Some(root) = codex_sessions_root() else {
        return Vec::new();
    };

    list_recent_codex_transcript_files_in(&root)
}

fn list_recent_codex_transcript_files_in(root: &Path) -> Vec<(PathBuf, i64)> {
    let mut candidates = Vec::new();
    let cutoff = current_timestamp() - TRANSCRIPT_DISCOVERY_WINDOW_MS;
    for year in list_sorted_children(&root, true).into_iter().take(2) {
        for month in list_sorted_children(&year, true).into_iter().take(4) {
            for day in list_sorted_children(&month, true).into_iter().take(7) {
                for file in list_sorted_children(&day, false) {
                    let is_jsonl = file
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"));
                    if !is_jsonl {
                        continue;
                    }

                    let Some(last_modified) = file_modified_timestamp(&file) else {
                        continue;
                    };
                    if last_modified < cutoff {
                        continue;
                    }
                    candidates.push((file, last_modified));
                }
            }
        }
    }

    candidates.sort_by(|left, right| right.1.cmp(&left.1));

    let mut files = Vec::new();
    for (file, last_modified) in candidates {
        if is_auxiliary_codex_transcript_file(&file) {
            continue;
        }
        files.push((file, last_modified));
        if files.len() >= MAX_DISCOVERED_TRANSCRIPTS {
            break;
        }
    }

    files
}

fn discover_recent_openclaw_transcripts() -> Vec<DiscoveredTranscript> {
    let Some(root) = openclaw_sessions_root() else {
        return Vec::new();
    };

    discover_recent_openclaw_transcripts_in(&root)
}

fn discover_recent_openclaw_transcripts_in(root: &Path) -> Vec<DiscoveredTranscript> {
    let mut files = list_sorted_children(root, false)
        .into_iter()
        .filter(|file| {
            file.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
        })
        .filter_map(|file| {
            let session_id = file.file_stem()?.to_string_lossy().to_string();
            let last_activity = transcript_activity_timestamp(&file)?;
            if current_timestamp() - last_activity > TRANSCRIPT_DISCOVERY_WINDOW_MS {
                return None;
            }

            Some(DiscoveredTranscript {
                session_id: session_id.clone(),
                provider_id: "openclaw".into(),
                transcript_path: file.display().to_string(),
                provider_session_id: Some(session_id),
                terminal_context: None,
                last_activity,
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| right.last_activity.cmp(&left.last_activity));
    files.truncate(MAX_DISCOVERED_TRANSCRIPTS);
    files
}

fn list_sorted_children(path: &Path, directories_only: bool) -> Vec<PathBuf> {
    let mut children = fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if directories_only && !path.is_dir() {
                return None;
            }
            if !directories_only && !path.is_file() {
                return None;
            }
            Some(path)
        })
        .collect::<Vec<_>>();

    children.sort_by(|left, right| {
        right
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
            .cmp(
                &left
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
    });
    children
}

fn file_modified_timestamp(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn transcript_activity_timestamp(path: &Path) -> Option<i64> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(TRANSCRIPT_ACTIVITY_SCAN_BYTES);
    if start > 0 {
        file.seek(SeekFrom::Start(start)).ok()?;
    }

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    let text = String::from_utf8_lossy(&buffer);
    let scan_text = if start > 0 {
        text.find('\n')
            .map(|index| &text[index + 1..])
            .unwrap_or(text.as_ref())
    } else {
        text.as_ref()
    };

    let mut latest = None;

    for line in scan_text.lines().rev().take(TRANSCRIPT_ACTIVITY_MAX_LINES) {
        if line.trim().is_empty() {
            continue;
        }

        if let Some(timestamp) = latest_timestamp_in_line(line) {
            latest = Some(latest.map_or(timestamp, |current: i64| current.max(timestamp)));
        }
    }

    latest.or_else(|| file_modified_timestamp(path))
}

fn latest_timestamp_in_line(line: &str) -> Option<i64> {
    TRANSCRIPT_TIMESTAMP_KEY_PATTERNS
        .iter()
        .filter_map(|pattern| latest_timestamp_for_key(line, pattern))
        .max()
}

fn latest_timestamp_for_key(line: &str, key_pattern: &str) -> Option<i64> {
    let mut latest = None;
    let mut search_start = 0;

    while let Some(relative_index) = line[search_start..].find(key_pattern) {
        let key_end = search_start + relative_index + key_pattern.len();
        let after_key = &line[key_end..];
        let Some(colon_index) = after_key.find(':') else {
            break;
        };
        let value = &after_key[colon_index + 1..];

        if let Some((timestamp, consumed)) = parse_inline_timestamp_value(value) {
            latest = Some(latest.map_or(timestamp, |current: i64| current.max(timestamp)));
            search_start = key_end + colon_index + 1 + consumed;
        } else {
            search_start = key_end;
        }
    }

    latest
}

fn parse_inline_timestamp_value(value: &str) -> Option<(i64, usize)> {
    let leading_whitespace = value.len() - value.trim_start().len();
    let value = value.trim_start();
    let first = value.as_bytes().first()?;

    if *first == b'"' {
        return parse_quoted_inline_timestamp(value)
            .map(|(timestamp, consumed)| (timestamp, leading_whitespace + consumed));
    }

    if *first == b'-' || first.is_ascii_digit() {
        return parse_numeric_inline_timestamp(value)
            .map(|(timestamp, consumed)| (timestamp, leading_whitespace + consumed));
    }

    None
}

fn parse_quoted_inline_timestamp(value: &str) -> Option<(i64, usize)> {
    let mut escaped = false;
    for (index, character) in value.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }

        match character {
            '\\' => escaped = true,
            '"' => {
                let timestamp = parse_timestamp_string(&value[1..index])?;
                return Some((timestamp, index + character.len_utf8()));
            }
            _ => {}
        }
    }

    None
}

fn parse_numeric_inline_timestamp(value: &str) -> Option<(i64, usize)> {
    let mut end = 0;
    for (index, character) in value.char_indices() {
        if (index == 0 && character == '-') || character.is_ascii_digit() {
            end = index + character.len_utf8();
            continue;
        }
        break;
    }

    if end == 0 || &value[..end] == "-" {
        return None;
    }

    let number = value[..end].parse::<i64>().ok()?;
    Some((normalize_unix_timestamp(number), end))
}

fn normalize_unix_timestamp(number: i64) -> i64 {
    if number > 1_000_000_000_000 {
        number
    } else {
        number * 1000
    }
}

fn parse_timestamp_string(value: &str) -> Option<i64> {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|timestamp| (timestamp.unix_timestamp_nanos() / 1_000_000) as i64)
}

fn read_codex_session_meta(path: &Path) -> Option<serde_json::Value> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(4) {
        let Ok(line) = line else {
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value
            .get("type")
            .and_then(|value| value.as_str())
            .is_some_and(|line_type| line_type == "session_meta")
        {
            return value.get("payload").cloned();
        }
    }

    None
}

fn is_auxiliary_codex_transcript_file(path: &Path) -> bool {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let reader = BufReader::new(file);

    for line in reader.lines().map_while(Result::ok).take(160) {
        if line.trim().is_empty() {
            continue;
        }

        if line.contains("/.codex/memories")
            || line.contains("Memory Writing Agent")
            || is_codex_title_generation_prompt(&line)
        {
            return true;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let payload = value.get("payload").unwrap_or(&value);

        if payload
            .get("cwd")
            .and_then(|value| value.as_str())
            .is_some_and(is_codex_internal_memory_cwd)
        {
            return true;
        }

        for key in ["prompt", "message", "content", "text"] {
            if let Some(text) = payload.get(key).and_then(extract_text_from_json)
                && is_codex_title_generation_prompt(&text)
            {
                return true;
            }
        }
    }

    false
}

fn is_codex_internal_memory_cwd(cwd: &str) -> bool {
    let normalized = cwd.replace('\\', "/");
    normalized.ends_with("/.codex/memories") || normalized.contains("/.codex/memories/")
}

fn extract_text_from_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.to_string()),
        serde_json::Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(extract_text_from_json)
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        serde_json::Value::Object(map) => {
            for key in ["text", "content", "message", "body"] {
                if let Some(text) = map.get(key).and_then(extract_text_from_json) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

fn get_first_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    })
}

fn copilot_sessions_root() -> Option<PathBuf> {
    std::env::var_os("COPILOT_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("session-state"))
        .or_else(|| dirs::home_dir().map(|home| home.join(".copilot").join("session-state")))
}

fn codex_sessions_root() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("sessions"))
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex").join("sessions")))
}

fn openclaw_sessions_root() -> Option<PathBuf> {
    std::env::var_os("OPENCLAW_HOME")
        .map(PathBuf::from)
        .map(|root| root.join("agents").join("main").join("sessions"))
        .or_else(|| {
            dirs::home_dir().map(|home| {
                home.join(".openclaw")
                    .join("agents")
                    .join("main")
                    .join("sessions")
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "yorling-bridge-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn transcript_activity_timestamp_prefers_embedded_event_time() {
        let root = temp_root("activity");
        std::fs::create_dir_all(&root).expect("create root");
        let path = root.join("events.jsonl");
        std::fs::write(
            &path,
            r#"{"type":"assistant.message","timestamp":"2026-04-01T10:00:00Z","data":{"content":"Old output"}}"#,
        )
        .expect("write transcript");

        let activity = transcript_activity_timestamp(&path).expect("activity timestamp");
        assert_eq!(activity, 1_775_037_600_000);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn transcript_backfill_uses_transcript_activity_for_new_session_timestamp() {
        let root = temp_root("hydrate");
        std::fs::create_dir_all(&root).expect("create root");
        let path = root.join("events.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"type":"session.start","timestamp":"2026-04-01T10:00:00Z","data":{"sessionId":"copilot-session"}}"#,
                "\n",
                r#"{"type":"user.message","timestamp":"2026-04-01T10:02:00Z","data":{"content":"Ship it"}}"#,
                "\n",
                r#"{"type":"assistant.message","timestamp":"2026-04-01T10:03:00Z","data":{"content":"Done."}}"#
            ),
        )
        .expect("write transcript");

        let store = Arc::new(SessionStore::new());
        let server = BridgeServer::new(store.clone(), Arc::new(ProviderRegistry::new()));
        server
            .hydrate_transcript_snapshot(
                "copilot-session",
                "copilot",
                &path.display().to_string(),
                Some("copilot-session".into()),
                None,
            )
            .await;

        let session = store.get_session("copilot-session").expect("session");
        assert_eq!(session.started_at, 1_775_037_780_000);
        assert_eq!(
            session
                .transcript_preview
                .as_ref()
                .map(|preview| preview.turn_count),
            Some(2)
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn codex_backfill_skips_finished_sessions_that_were_not_live() {
        let root = temp_root("codex-finished-backfill");
        std::fs::create_dir_all(&root).expect("create root");
        let path = root.join("codex.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"timestamp":"2026-04-29T03:55:04.974Z","type":"session_meta","payload":{"id":"codex-finished","cwd":"/Users/example/claude_code"}}"#,
                "\n",
                r#"{"timestamp":"2026-04-29T03:55:04.976Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}"#,
                "\n",
                r#"{"timestamp":"2026-04-29T03:55:05.023Z","type":"event_msg","payload":{"type":"user_message","message":"你好"}}"#,
                "\n",
                r#"{"timestamp":"2026-04-29T03:55:11.550Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"你好呀，我在。"}}"#
            ),
        )
        .expect("write transcript");

        let store = Arc::new(SessionStore::new());
        let server = BridgeServer::new(store.clone(), Arc::new(ProviderRegistry::new()));
        server
            .hydrate_transcript_snapshot(
                "codex-finished",
                "codex",
                &path.display().to_string(),
                Some("codex-finished".into()),
                None,
            )
            .await;

        assert!(store.get_session("codex-finished").is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn codex_live_session_can_end_from_transcript_sync() {
        let root = temp_root("codex-live-finished");
        std::fs::create_dir_all(&root).expect("create root");
        let path = root.join("codex.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"timestamp":"2026-04-29T03:55:04.974Z","type":"session_meta","payload":{"id":"codex-live","cwd":"/Users/example/yorling/yorling"}}"#,
                "\n",
                r#"{"timestamp":"2026-04-29T03:55:04.976Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}"#,
                "\n",
                r#"{"timestamp":"2026-04-29T03:55:11.550Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Done."}}"#
            ),
        )
        .expect("write transcript");

        let store = Arc::new(SessionStore::new());
        store.apply_event(IslandEvent {
            session_id: "codex-live".into(),
            provider_id: "codex".into(),
            timestamp: 1_777_430_000_000,
            event_type: IslandEventType::SessionStart,
            terminal_context: None,
        });

        let server = BridgeServer::new(store.clone(), Arc::new(ProviderRegistry::new()));
        server
            .hydrate_transcript_snapshot(
                "codex-live",
                "codex",
                &path.display().to_string(),
                Some("codex-live".into()),
                None,
            )
            .await;

        let session = store.get_session("codex-live").expect("session remains");
        assert!(matches!(session.phase, crate::session::SessionPhase::Ended));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn openclaw_discovery_uses_transcript_activity_not_file_mtime() {
        let root = temp_root("openclaw");
        std::fs::create_dir_all(&root).expect("create root");
        let stale = root.join("stale.jsonl");
        let fresh = root.join("fresh.jsonl");

        std::fs::write(
            &stale,
            r#"{"type":"message","id":"m1","timestamp":"2026-04-01T09:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"old"}]}}"#,
        )
        .expect("write stale");
        std::fs::write(
            &fresh,
            format!(
                r#"{{"type":"message","id":"m2","timestamp":"{}","message":{{"role":"assistant","content":[{{"type":"text","text":"fresh"}}]}}}}"#,
                time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).expect("rfc3339")
            ),
        )
        .expect("write fresh");

        let discovered = discover_recent_openclaw_transcripts_in(&root);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].session_id, "fresh");

        let _ = std::fs::remove_dir_all(&root);
    }
}
