import test from 'node:test';
import assert from 'node:assert/strict';
import type { IslandSession } from '../src/types/index.ts';
import {
  COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS,
  filterCollapsedIslandSessions,
  filterVisibleIslandSessions,
  getLatestAttentionSessionId,
  getPrimaryIslandSession,
  IDLE_SESSION_VISIBILITY_TIMEOUT_MS,
  sortIslandSessionsForList,
} from '../src/island/sessionQueue.ts';

function makeSession(overrides: Partial<IslandSession> & { id: string }): IslandSession {
  return {
    id: overrides.id,
    provider_id: overrides.provider_id ?? 'claude-code',
    phase: overrides.phase ?? 'idle',
    task_title: overrides.task_title ?? null,
    provider_session_id: overrides.provider_session_id ?? null,
    chat_messages: overrides.chat_messages ?? [],
    tools_in_flight: overrides.tools_in_flight ?? [],
    pending_permission: overrides.pending_permission ?? null,
    pending_question: overrides.pending_question ?? null,
    subagent_count: overrides.subagent_count ?? 0,
    started_at: overrides.started_at ?? 100,
    ended_at: overrides.ended_at ?? null,
    terminal_context: overrides.terminal_context ?? null,
    transcript_preview: overrides.transcript_preview ?? null,
    richer_snapshot: overrides.richer_snapshot ?? false,
  };
}

test('sorts session list so the newest attention request lands at the visual bottom', () => {
  const sessions = [
    makeSession({
      id: 'processing-old',
      phase: 'processing',
      started_at: 100,
      transcript_preview: {
        transcript_path: '/tmp/a.jsonl',
        provider_session_id: null,
        status: 'synced',
        synced_at: 130,
        turn_count: 1,
        latest_task_started_at: null,
        latest_task_finished_at: null,
        latest_pending_question: null,
        last_error: null,
        chat_preview: [],
        tool_history: [],
      },
    }),
    makeSession({
      id: 'attention-older',
      phase: 'waitingforapproval',
      started_at: 90,
      pending_permission: {
        request_id: 'perm-1',
        tool: 'Bash',
        input: {},
        received_at: 200,
      },
    }),
    makeSession({
      id: 'attention-newest',
      phase: 'waitingforanswer',
      started_at: 80,
      pending_question: {
        request_id: 'q-1',
        question: 'Continue?',
        options: ['yes', 'no'],
        received_at: 300,
      },
    }),
  ];

  const ordered = sortIslandSessionsForList(sessions);

  assert.deepEqual(
    ordered.map((session) => session.id),
    ['processing-old', 'attention-older', 'attention-newest'],
  );
  assert.strictEqual(getLatestAttentionSessionId(sessions), 'attention-newest');
});

test('picks the latest pending request as the primary island session', () => {
  const sessions = [
    makeSession({
      id: 'processing-latest',
      phase: 'processing',
      started_at: 100,
      transcript_preview: {
        transcript_path: '/tmp/b.jsonl',
        provider_session_id: null,
        status: 'synced',
        synced_at: 400,
        turn_count: 3,
        latest_task_started_at: null,
        latest_task_finished_at: null,
        latest_pending_question: null,
        last_error: null,
        chat_preview: [],
        tool_history: [],
      },
    }),
    makeSession({
      id: 'attention-old',
      phase: 'waitingforapproval',
      pending_permission: {
        request_id: 'perm-2',
        tool: 'Edit',
        input: {},
        received_at: 250,
      },
    }),
    makeSession({
      id: 'attention-new',
      phase: 'waitingforanswer',
      pending_question: {
        request_id: 'q-2',
        question: 'Ship it?',
        options: ['ship', 'wait'],
        received_at: 500,
      },
    }),
  ];

  assert.strictEqual(getPrimaryIslandSession(sessions)?.id, 'attention-new');
});

test('falls back to the most recently active session when no request is pending', () => {
  const sessions = [
    makeSession({ id: 'idle-early', started_at: 100, phase: 'idle' }),
    makeSession({
      id: 'processing-new',
      started_at: 90,
      phase: 'processing',
      transcript_preview: {
        transcript_path: '/tmp/c.jsonl',
        provider_session_id: null,
        status: 'synced',
        synced_at: 420,
        turn_count: 2,
        latest_task_started_at: null,
        latest_task_finished_at: null,
        latest_pending_question: null,
        last_error: null,
        chat_preview: [],
        tool_history: [],
      },
    }),
  ];

  assert.strictEqual(getPrimaryIslandSession(sessions)?.id, 'processing-new');
  assert.strictEqual(getLatestAttentionSessionId(sessions), 'processing-new');
});

test('keeps same provider sessions grouped together in the expanded list order', () => {
  const sessions = [
    makeSession({
      id: 'codex-older',
      provider_id: 'codex',
      phase: 'idle',
      started_at: 100,
    }),
    makeSession({
      id: 'claude-older',
      provider_id: 'claude-code',
      phase: 'idle',
      started_at: 120,
    }),
    makeSession({
      id: 'codex-newer',
      provider_id: 'codex',
      phase: 'waitingforanswer',
      started_at: 130,
      pending_question: {
        request_id: 'q-codex',
        question: '继续吗？',
        options: ['继续', '暂停'],
        received_at: 320,
      },
    }),
    makeSession({
      id: 'claude-newer',
      provider_id: 'claude-code',
      phase: 'idle',
      started_at: 140,
    }),
  ];

  const ordered = sortIslandSessionsForList(sessions);

  assert.deepEqual(
    ordered.map((session) => session.id),
    ['claude-older', 'claude-newer', 'codex-older', 'codex-newer'],
  );
});

test('hides only sessions that have been idle for more than twenty minutes', () => {
  const now = 2_000_000;
  const sessions = [
    makeSession({
      id: 'idle-stale',
      phase: 'idle',
      started_at: now - IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 1,
    }),
    makeSession({
      id: 'idle-fresh',
      phase: 'idle',
      started_at: now - IDLE_SESSION_VISIBILITY_TIMEOUT_MS + 5_000,
    }),
    makeSession({
      id: 'processing-stale',
      phase: 'processing',
      started_at: now - IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 60_000,
    }),
    makeSession({
      id: 'ended-fresh',
      phase: 'ended',
      started_at: now - 1_000,
      ended_at: now - 500,
    }),
    makeSession({
      id: 'ended-stale',
      phase: 'ended',
      started_at: now - IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 60_000,
      ended_at: now - IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 1,
    }),
  ];

  const visible = filterVisibleIslandSessions(sessions, now);

  assert.deepEqual(
    visible.map((session) => session.id),
    ['idle-fresh', 'processing-stale', 'ended-fresh'],
  );
});

test('hides idle sessions from the collapsed island after ten minutes while keeping active ones', () => {
  const now = 3_000_000;
  const sessions = [
    makeSession({
      id: 'idle-collapsed-stale',
      phase: 'idle',
      started_at: now - COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 1,
    }),
    makeSession({
      id: 'idle-collapsed-fresh',
      phase: 'idle',
      started_at: now - COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS + 5_000,
    }),
    makeSession({
      id: 'waiting-visible',
      phase: 'waitingforanswer',
      pending_question: {
        request_id: 'q-collapsed',
        question: '继续执行吗？',
        options: ['继续', '停止'],
        received_at: now - COLLAPSED_IDLE_SESSION_VISIBILITY_TIMEOUT_MS - 60_000,
      },
    }),
  ];

  const visible = filterCollapsedIslandSessions(sessions, now);

  assert.deepEqual(
    visible.map((session) => session.id),
    ['idle-collapsed-fresh', 'waiting-visible'],
  );
});
