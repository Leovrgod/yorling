use dashmap::DashMap;
use tokio::sync::broadcast;

use crate::event::IslandEvent;
use crate::session::{SessionState, reduce_event};

const EVENT_CHANNEL_CAPACITY: usize = 256;

pub struct SessionStore {
    sessions: DashMap<String, SessionState>,
    event_tx: broadcast::Sender<IslandEvent>,
}

impl SessionStore {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            sessions: DashMap::new(),
            event_tx,
        }
    }

    /// Apply an event: create session if needed, reduce, then broadcast.
    pub fn apply_event(&self, event: IslandEvent) {
        let session_id = event.session_id.clone();

        if matches!(
            event.event_type,
            crate::event::IslandEventType::SessionDiscard
        ) {
            self.sessions.remove(&session_id);
            let _ = self.event_tx.send(event);
            return;
        }

        let mut entry = self.sessions.entry(session_id).or_insert_with(|| {
            SessionState::new(
                event.session_id.clone(),
                event.provider_id.clone(),
                event.timestamp,
            )
        });

        reduce_event(entry.value_mut(), &event);

        // Best-effort broadcast — no receivers is fine
        let _ = self.event_tx.send(event);
    }

    /// Subscribe to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<IslandEvent> {
        self.event_tx.subscribe()
    }

    /// Get a snapshot of a session.
    pub fn get_session(&self, id: &str) -> Option<SessionState> {
        self.sessions.get(id).map(|r| r.value().clone())
    }

    /// Get snapshots of all sessions.
    pub fn all_sessions(&self) -> Vec<SessionState> {
        self.sessions.iter().map(|r| r.value().clone()).collect()
    }

    /// Remove ended sessions older than the given timestamp.
    pub fn cleanup_ended_before(&self, before: i64) {
        self.sessions
            .retain(|_, s| s.ended_at.map_or(true, |ended| ended >= before));
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}
