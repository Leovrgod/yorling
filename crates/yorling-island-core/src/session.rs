use serde::{Deserialize, Serialize};

use crate::event::{IslandEvent, IslandEventType, TerminalContext};
use crate::transcript::TranscriptPreview;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub id: String,
    pub provider_id: String,
    pub phase: SessionPhase,
    pub task_title: Option<String>,
    pub provider_session_id: Option<String>,
    pub tools_in_flight: Vec<ToolCall>,
    pub pending_permission: Option<PendingPermission>,
    pub pending_question: Option<PendingQuestion>,
    pub chat_messages: Vec<ChatMessage>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub subagents: Vec<SubagentState>,
    pub terminal: Option<TerminalContext>,
    pub transcript_preview: Option<TranscriptPreview>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    Idle,
    Processing,
    WaitingForApproval,
    WaitingForAnswer,
    Compacting,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPermission {
    pub request_id: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub received_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingQuestion {
    pub request_id: String,
    pub question: String,
    pub options: Vec<String>,
    pub received_at: i64,
    pub from_permission: bool,
    pub answer_key: Option<String>,
    pub can_answer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentState {
    pub id: String,
    pub started_at: i64,
}

impl SessionState {
    pub fn new(id: String, provider_id: String, timestamp: i64) -> Self {
        Self {
            id,
            provider_id,
            phase: SessionPhase::Idle,
            task_title: None,
            provider_session_id: None,
            tools_in_flight: Vec::new(),
            pending_permission: None,
            pending_question: None,
            chat_messages: Vec::new(),
            started_at: timestamp,
            ended_at: None,
            subagents: Vec::new(),
            terminal: None,
            transcript_preview: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhaseHint {
    Idle,
    Processing,
    Compacting,
    DerivedIdle,
    DerivedProcessing,
    DerivedPreserve,
    Preserve,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ActivityKind {
    User,
    Tool,
    System,
    Assistant,
}

/// Pure-function reducer: applies an event to mutate session state.
pub fn reduce_event(state: &mut SessionState, event: &IslandEvent) {
    // Update terminal context if present in the event
    if let Some(ctx) = &event.terminal_context {
        merge_terminal_context(&mut state.terminal, ctx);
    }

    let phase_hint = match &event.event_type {
        IslandEventType::SessionStart => {
            state.ended_at = None;
            PhaseHint::Idle
        }
        IslandEventType::SessionEnd => {
            state.ended_at = Some(event.timestamp);
            state.tools_in_flight.clear();
            state.pending_permission = None;
            state.pending_question = None;
            PhaseHint::Ended
        }
        IslandEventType::SessionDiscard => PhaseHint::Ended,
        IslandEventType::UserPrompt { text } => {
            state.ended_at = None;
            clear_waiting_state(state);
            // Extract first line as task title if none set
            if state.task_title.is_none() {
                state.task_title = Some(text.lines().next().unwrap_or(text).to_string());
            }
            state.chat_messages.push(ChatMessage {
                role: ChatRole::User,
                text: text.clone(),
                timestamp: event.timestamp,
            });
            PhaseHint::Processing
        }
        IslandEventType::ToolUseStart { tool, .. } => {
            state.ended_at = None;
            state.tools_in_flight.push(ToolCall {
                tool: tool.clone(),
                started_at: event.timestamp,
            });
            PhaseHint::Processing
        }
        IslandEventType::ToolUseEnd { tool, .. } => {
            state.tools_in_flight.retain(|t| t.tool != *tool);
            PhaseHint::DerivedProcessing
        }
        IslandEventType::PermissionRequest {
            tool,
            input,
            request_id,
        } => {
            state.ended_at = None;
            state.pending_permission = Some(PendingPermission {
                request_id: request_id.clone(),
                tool: tool.clone(),
                input: input.clone(),
                received_at: event.timestamp,
            });
            PhaseHint::DerivedProcessing
        }
        IslandEventType::PermissionDecision { .. } => {
            state.pending_permission = None;
            PhaseHint::DerivedProcessing
        }
        IslandEventType::AskQuestion {
            question,
            options,
            request_id,
            from_permission,
            answer_key,
        } => {
            state.ended_at = None;
            state.pending_question = Some(PendingQuestion {
                request_id: request_id.clone(),
                question: question.clone(),
                options: options.clone(),
                received_at: event.timestamp,
                from_permission: *from_permission,
                answer_key: answer_key.clone(),
                can_answer: true,
            });
            PhaseHint::DerivedProcessing
        }
        IslandEventType::QuestionAnswer { .. } => {
            state.pending_question = None;
            PhaseHint::DerivedProcessing
        }
        IslandEventType::AgentThinking => PhaseHint::Processing,
        IslandEventType::AgentResponse { text } => {
            state.chat_messages.push(ChatMessage {
                role: ChatRole::Assistant,
                text: text.clone(),
                timestamp: event.timestamp,
            });
            PhaseHint::DerivedIdle
        }
        IslandEventType::SubagentStart { subagent_id } => {
            state.subagents.push(SubagentState {
                id: subagent_id.clone(),
                started_at: event.timestamp,
            });
            PhaseHint::Processing
        }
        IslandEventType::SubagentStop { subagent_id } => {
            state.subagents.retain(|s| s.id != *subagent_id);
            PhaseHint::DerivedProcessing
        }
        IslandEventType::ContextCompaction => PhaseHint::Compacting,
        IslandEventType::Stop => {
            clear_waiting_state(state);
            state.tools_in_flight.clear();
            PhaseHint::Idle
        }
        IslandEventType::Notification { .. } => PhaseHint::Preserve,
        IslandEventType::TranscriptSync { preview } => {
            if preview.provider_session_id.is_some() {
                state.provider_session_id = preview.provider_session_id.clone();
            }
            state.transcript_preview = Some(preview.clone());
            if let Some(question) = &preview.latest_pending_question {
                state.ended_at = None;
                state.pending_question = Some(PendingQuestion {
                    request_id: question.request_id.clone(),
                    question: question.question.clone(),
                    options: question.options.clone(),
                    received_at: question.received_at,
                    from_permission: false,
                    answer_key: None,
                    can_answer: false,
                });
                PhaseHint::DerivedProcessing
            } else if should_mark_transcript_task_ended(state, preview) {
                clear_transcript_derived_question(state);
                state.ended_at = preview.latest_task_finished_at;
                state.tools_in_flight.clear();
                state.pending_permission = None;
                state.pending_question = None;
                PhaseHint::Ended
            } else if clear_transcript_derived_question(state) {
                PhaseHint::DerivedProcessing
            } else {
                PhaseHint::DerivedPreserve
            }
        }
    };

    reconcile_phase(state, phase_hint);
}

fn clear_waiting_state(state: &mut SessionState) {
    state.pending_permission = None;
    state.pending_question = None;
}

fn reconcile_phase(state: &mut SessionState, hint: PhaseHint) {
    if hint == PhaseHint::Ended {
        state.phase = SessionPhase::Ended;
        return;
    }

    if state.pending_permission.is_some() {
        state.phase = SessionPhase::WaitingForApproval;
        return;
    }

    if state.pending_question.is_some() {
        state.phase = SessionPhase::WaitingForAnswer;
        return;
    }

    if !state.tools_in_flight.is_empty() || !state.subagents.is_empty() {
        state.phase = SessionPhase::Processing;
        return;
    }

    state.phase = match hint {
        PhaseHint::Idle => SessionPhase::Idle,
        PhaseHint::Processing => SessionPhase::Processing,
        PhaseHint::Compacting => SessionPhase::Compacting,
        PhaseHint::DerivedIdle => derive_phase_from_activity(state, SessionPhase::Idle),
        PhaseHint::DerivedProcessing => derive_phase_from_activity(state, SessionPhase::Processing),
        PhaseHint::DerivedPreserve => derive_phase_from_activity(state, state.phase),
        PhaseHint::Preserve => state.phase,
        PhaseHint::Ended => SessionPhase::Ended,
    };
}

fn derive_phase_from_activity(state: &SessionState, fallback: SessionPhase) -> SessionPhase {
    match latest_activity_kind(state) {
        Some(ActivityKind::User | ActivityKind::Tool) => SessionPhase::Processing,
        Some(ActivityKind::Assistant | ActivityKind::System) => SessionPhase::Idle,
        None => fallback,
    }
}

fn should_mark_transcript_task_ended(state: &SessionState, preview: &TranscriptPreview) -> bool {
    if state.provider_id != "codex" {
        return false;
    }

    let Some(finished_at) = preview.latest_task_finished_at else {
        return false;
    };

    if preview
        .latest_task_started_at
        .is_some_and(|started_at| started_at > finished_at)
    {
        return false;
    }

    state.pending_permission.is_none()
        && state
            .pending_question
            .as_ref()
            .is_none_or(|question| !question.can_answer)
        && state.tools_in_flight.is_empty()
        && state.subagents.is_empty()
}

fn clear_transcript_derived_question(state: &mut SessionState) -> bool {
    if state
        .pending_question
        .as_ref()
        .is_some_and(|question| !question.can_answer)
    {
        state.pending_question = None;
        return true;
    }
    false
}

fn latest_activity_kind(state: &SessionState) -> Option<ActivityKind> {
    let live_chat = state
        .chat_messages
        .iter()
        .map(|message| (message.timestamp, activity_kind_for_role(message.role)));
    let preview_chat = state
        .transcript_preview
        .as_ref()
        .into_iter()
        .flat_map(|preview| {
            preview
                .chat_preview
                .iter()
                .map(|message| (message.timestamp, activity_kind_for_role(message.role)))
        });
    let preview_tools = state
        .transcript_preview
        .as_ref()
        .into_iter()
        .flat_map(|preview| {
            preview.tool_history.iter().filter_map(|item| {
                item.finished_at
                    .or(item.started_at)
                    .map(|timestamp| (timestamp, ActivityKind::Tool))
            })
        });

    live_chat
        .chain(preview_chat)
        .chain(preview_tools)
        .max_by_key(|(timestamp, kind)| (*timestamp, *kind))
        .map(|(_, kind)| kind)
}

fn activity_kind_for_role(role: ChatRole) -> ActivityKind {
    match role {
        ChatRole::User => ActivityKind::User,
        ChatRole::Assistant => ActivityKind::Assistant,
        ChatRole::System => ActivityKind::System,
    }
}

fn merge_terminal_context(target: &mut Option<TerminalContext>, incoming: &TerminalContext) {
    match target {
        Some(existing) => {
            if incoming.pid.is_some() {
                existing.pid = incoming.pid;
            }
            if incoming.tty.is_some() {
                existing.tty = incoming.tty.clone();
            }
            if incoming.cwd.is_some() {
                existing.cwd = incoming.cwd.clone();
            }
            if incoming.terminal_app.is_some() {
                existing.terminal_app = incoming.terminal_app.clone();
            }
            if incoming.terminal_bundle_id.is_some() {
                existing.terminal_bundle_id = incoming.terminal_bundle_id.clone();
            }
            if incoming.terminal_session_id.is_some() {
                existing.terminal_session_id = incoming.terminal_session_id.clone();
            }
            if incoming.pane_title.is_some() {
                existing.pane_title = incoming.pane_title.clone();
            }
            if incoming.warp_pane_uuid.is_some() {
                existing.warp_pane_uuid = incoming.warp_pane_uuid.clone();
            }
        }
        None => *target = Some(incoming.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Decision, IslandEventType};
    use crate::transcript::{TranscriptPendingQuestion, TranscriptPreview, TranscriptSyncStatus};

    fn make_event(session_id: &str, event_type: IslandEventType) -> IslandEvent {
        IslandEvent {
            session_id: session_id.to_string(),
            provider_id: "test".to_string(),
            timestamp: 1000,
            event_type,
            terminal_context: None,
        }
    }

    #[test]
    fn session_start_sets_idle() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::Ended;
        reduce_event(&mut state, &make_event("s1", IslandEventType::SessionStart));
        assert_eq!(state.phase, SessionPhase::Idle);
    }

    #[test]
    fn user_prompt_sets_processing_and_title() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::UserPrompt {
                    text: "Fix the bug\nin auth module".into(),
                },
            ),
        );
        assert_eq!(state.phase, SessionPhase::Processing);
        assert_eq!(state.task_title.as_deref(), Some("Fix the bug"));
        assert_eq!(state.chat_messages.len(), 1);
    }

    #[test]
    fn tool_lifecycle() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::ToolUseStart {
                    tool: "Read".into(),
                    input: serde_json::json!({}),
                },
            ),
        );
        assert_eq!(state.tools_in_flight.len(), 1);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::ToolUseEnd {
                    tool: "Read".into(),
                    success: true,
                },
            ),
        );
        assert!(state.tools_in_flight.is_empty());
        assert_eq!(state.phase, SessionPhase::Processing);
    }

    #[test]
    fn permission_flow() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::PermissionRequest {
                    tool: "Bash".into(),
                    input: serde_json::json!({"command": "rm -rf /"}),
                    request_id: "req1".into(),
                },
            ),
        );
        assert_eq!(state.phase, SessionPhase::WaitingForApproval);
        assert!(state.pending_permission.is_some());

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::PermissionDecision {
                    request_id: "req1".into(),
                    decision: Decision::Allow,
                },
            ),
        );
        assert_eq!(state.phase, SessionPhase::Processing);
        assert!(state.pending_permission.is_none());
    }

    #[test]
    fn session_end_clears_state() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::Processing;
        state.tools_in_flight.push(ToolCall {
            tool: "Read".into(),
            started_at: 100,
        });
        reduce_event(&mut state, &make_event("s1", IslandEventType::SessionEnd));
        assert_eq!(state.phase, SessionPhase::Ended);
        assert!(state.tools_in_flight.is_empty());
        assert!(state.pending_permission.is_none());
    }

    #[test]
    fn transcript_sync_updates_provider_session_id() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/session.jsonl".into(),
                        provider_session_id: Some("provider-42".into()),
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(1000),
                        turn_count: 2,
                        latest_task_started_at: None,
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: Vec::new(),
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert_eq!(state.provider_session_id.as_deref(), Some("provider-42"));
        assert_eq!(
            state
                .transcript_preview
                .as_ref()
                .map(|preview| preview.transcript_path.as_str()),
            Some("/tmp/session.jsonl")
        );
    }

    #[test]
    fn transcript_progress_does_not_clear_pending_permission() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::WaitingForApproval;
        state.pending_permission = Some(PendingPermission {
            request_id: "req1".into(),
            tool: "Bash".into(),
            input: serde_json::json!({}),
            received_at: 1000,
        });

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/session.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(1500),
                        turn_count: 2,
                        latest_task_started_at: None,
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: vec![ChatMessage {
                            role: ChatRole::Assistant,
                            text: "Running Bash now".into(),
                            timestamp: 1200,
                        }],
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert!(state.pending_permission.is_some());
        assert_eq!(state.phase, SessionPhase::WaitingForApproval);
    }

    #[test]
    fn transcript_progress_does_not_clear_pending_question() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::WaitingForAnswer;
        state.pending_question = Some(PendingQuestion {
            request_id: "req1".into(),
            question: "Which one?".into(),
            options: vec!["Yes".into(), "No".into()],
            received_at: 1000,
            from_permission: true,
            answer_key: Some("answer".into()),
            can_answer: true,
        });

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/session.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(1500),
                        turn_count: 2,
                        latest_task_started_at: None,
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: vec![ChatMessage {
                            role: ChatRole::Assistant,
                            text: "Waiting on your answer".into(),
                            timestamp: 1200,
                        }],
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert!(state.pending_question.is_some());
        assert_eq!(state.phase, SessionPhase::WaitingForAnswer);
    }

    #[test]
    fn agent_response_does_not_clear_pending_permission() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::WaitingForApproval;
        state.pending_permission = Some(PendingPermission {
            request_id: "req1".into(),
            tool: "Bash".into(),
            input: serde_json::json!({}),
            received_at: 1000,
        });

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::AgentResponse {
                    text: "Still waiting for your approval".into(),
                },
            ),
        );

        assert!(state.pending_permission.is_some());
        assert_eq!(state.phase, SessionPhase::WaitingForApproval);
    }

    #[test]
    fn agent_response_with_no_remaining_work_sets_idle() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::UserPrompt {
                    text: "Summarize the repository".into(),
                },
            ),
        );
        assert_eq!(state.phase, SessionPhase::Processing);

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::AgentResponse {
                    text: "Done.".into(),
                },
            ),
        );

        assert_eq!(state.phase, SessionPhase::Idle);
    }

    #[test]
    fn tool_progress_does_not_clear_pending_question() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::WaitingForAnswer;
        state.pending_question = Some(PendingQuestion {
            request_id: "req1".into(),
            question: "Which one?".into(),
            options: vec!["Yes".into(), "No".into()],
            received_at: 1000,
            from_permission: true,
            answer_key: Some("answer".into()),
            can_answer: true,
        });
        state.tools_in_flight.push(ToolCall {
            tool: "Bash".into(),
            started_at: 900,
        });

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::ToolUseEnd {
                    tool: "Bash".into(),
                    success: true,
                },
            ),
        );

        assert!(state.pending_question.is_some());
        assert_eq!(state.phase, SessionPhase::WaitingForAnswer);
    }

    #[test]
    fn stop_clears_waiting_state() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::WaitingForApproval;
        state.pending_permission = Some(PendingPermission {
            request_id: "req1".into(),
            tool: "Bash".into(),
            input: serde_json::json!({}),
            received_at: 1000,
        });

        reduce_event(&mut state, &make_event("s1", IslandEventType::Stop));

        assert!(state.pending_permission.is_none());
        assert_eq!(state.phase, SessionPhase::Idle);
    }

    #[test]
    fn transcript_sync_with_latest_assistant_sets_idle() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::Processing;

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/session.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(1800),
                        turn_count: 2,
                        latest_task_started_at: None,
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: vec![
                            ChatMessage {
                                role: ChatRole::User,
                                text: "Check the island status".into(),
                                timestamp: 1200,
                            },
                            ChatMessage {
                                role: ChatRole::Assistant,
                                text: "The task is complete.".into(),
                                timestamp: 1700,
                            },
                        ],
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert_eq!(state.phase, SessionPhase::Idle);
    }

    #[test]
    fn transcript_sync_with_latest_user_keeps_processing() {
        let mut state = SessionState::new("s1".into(), "test".into(), 0);
        state.phase = SessionPhase::Processing;

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/session.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(1800),
                        turn_count: 1,
                        latest_task_started_at: None,
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: vec![ChatMessage {
                            role: ChatRole::User,
                            text: "Check the island status".into(),
                            timestamp: 1700,
                        }],
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert_eq!(state.phase, SessionPhase::Processing);
    }

    #[test]
    fn codex_transcript_finished_marker_sets_ended() {
        let mut state = SessionState::new("s1".into(), "codex".into(), 0);
        state.phase = SessionPhase::Processing;

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/codex.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(2200),
                        turn_count: 2,
                        latest_task_started_at: Some(1000),
                        latest_task_finished_at: Some(2000),
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: Vec::new(),
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert_eq!(state.phase, SessionPhase::Ended);
        assert_eq!(state.ended_at, Some(2000));
    }

    #[test]
    fn codex_transcript_pending_question_sets_waiting_without_bridge_reply() {
        let mut state = SessionState::new("s1".into(), "codex".into(), 0);

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/codex.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(2200),
                        turn_count: 1,
                        latest_task_started_at: Some(1000),
                        latest_task_finished_at: None,
                        latest_pending_question: Some(TranscriptPendingQuestion {
                            request_id: "codex-input:call_1".into(),
                            question: "Choose one".into(),
                            options: vec!["A".into(), "B".into()],
                            received_at: 1500,
                        }),
                        last_error: None,
                        chat_preview: Vec::new(),
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert_eq!(state.phase, SessionPhase::WaitingForAnswer);
        let question = state.pending_question.as_ref().expect("pending question");
        assert_eq!(question.request_id, "codex-input:call_1");
        assert!(!question.can_answer);
    }

    #[test]
    fn codex_transcript_clears_resolved_non_answerable_question() {
        let mut state = SessionState::new("s1".into(), "codex".into(), 0);
        state.pending_question = Some(PendingQuestion {
            request_id: "codex-input:call_1".into(),
            question: "Choose one".into(),
            options: vec!["A".into(), "B".into()],
            received_at: 1500,
            from_permission: false,
            answer_key: None,
            can_answer: false,
        });
        state.phase = SessionPhase::WaitingForAnswer;

        reduce_event(
            &mut state,
            &make_event(
                "s1",
                IslandEventType::TranscriptSync {
                    preview: TranscriptPreview {
                        transcript_path: "/tmp/codex.jsonl".into(),
                        provider_session_id: None,
                        status: TranscriptSyncStatus::Synced,
                        synced_at: Some(2200),
                        turn_count: 2,
                        latest_task_started_at: Some(1000),
                        latest_task_finished_at: None,
                        latest_pending_question: None,
                        last_error: None,
                        chat_preview: Vec::new(),
                        tool_history: Vec::new(),
                    },
                },
            ),
        );

        assert!(state.pending_question.is_none());
        assert_eq!(state.phase, SessionPhase::Processing);
    }
}
