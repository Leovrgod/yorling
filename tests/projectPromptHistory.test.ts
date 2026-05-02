import test from 'node:test';
import assert from 'node:assert/strict';
import type { TerminalProject } from '../src/stores/terminalStore.ts';
import type { IslandSession } from '../src/types/index.ts';
import {
  PROJECT_PROMPT_HISTORY_LIMIT,
  collectProjectPromptHistory,
  syncProjectPromptHistories,
} from '../src/components/terminals/projectPromptHistory.ts';

function makeProject(overrides: Partial<TerminalProject> & { id: string; path: string }): TerminalProject {
  return {
    id: overrides.id,
    name: overrides.name ?? 'demo',
    path: overrides.path,
    agents: overrides.agents ?? [],
    selectedAgentId: overrides.selectedAgentId ?? null,
    createdAt: overrides.createdAt ?? 1,
    updatedAt: overrides.updatedAt ?? 1,
  };
}

function makeSession(overrides: Partial<IslandSession> & { id: string }): IslandSession {
  return {
    id: overrides.id,
    provider_id: overrides.provider_id ?? 'claude-code',
    phase: overrides.phase ?? 'processing',
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

test('collects project prompt history from exact and descendant cwd matches', () => {
  const projectPath = '/Users/example/demo';
  const sessions = [
    makeSession({
      id: 'root-session',
      terminal_context: { cwd: projectPath },
      chat_messages: [
        { role: 'user', text: '先看多终端顶部区域', timestamp: 1000 },
      ],
      transcript_preview: {
        transcript_path: '/tmp/root.jsonl',
        provider_session_id: 'root-1',
        status: 'synced',
        synced_at: 1200,
        turn_count: 2,
        latest_task_started_at: null,
        latest_task_finished_at: null,
        latest_pending_question: null,
        last_error: null,
        chat_preview: [
          { role: 'user', text: '先看多终端顶部区域', timestamp: 1000 },
        ],
        tool_history: [],
      },
    }),
    makeSession({
      id: 'child-session',
      provider_id: 'codex',
      terminal_context: { cwd: '/Users/example/demo/packages/app' },
      chat_messages: [
        { role: 'user', text: '再补项目提示词历史', timestamp: 1100 },
      ],
    }),
    makeSession({
      id: 'other-session',
      terminal_context: { cwd: '/Users/example/elsewhere' },
      chat_messages: [
        { role: 'user', text: '这条不应该被归档', timestamp: 1300 },
      ],
    }),
  ];

  const history = collectProjectPromptHistory(sessions, projectPath);

  assert.deepEqual(
    history.map((item) => item.text),
    ['再补项目提示词历史', '先看多终端顶部区域'],
  );
  assert.deepEqual(
    history.map((item) => item.providerId),
    ['codex', 'claude-code'],
  );
});

test('syncs project prompt history and caps it to the latest 100 prompts', () => {
  const project = makeProject({
    id: 'project-1',
    path: '/Users/example/demo',
  });
  const sessions = Array.from({ length: PROJECT_PROMPT_HISTORY_LIMIT + 5 }, (_, index) => makeSession({
    id: `session-${index}`,
    provider_id: index % 2 === 0 ? 'claude-code' : 'codex',
    terminal_context: { cwd: project.path },
    chat_messages: [
      { role: 'user', text: `prompt-${index}`, timestamp: 1_000 + index },
    ],
  }));

  const synced = syncProjectPromptHistories(
    [project],
    sessions,
    {
      [project.id]: [{
        id: 'stale-item',
        sessionId: 'stale-session',
        providerId: 'claude-code',
        text: 'stale prompt',
        timestamp: 100,
        cwd: project.path,
      }],
    },
  );

  const history = synced[project.id];
  assert.strictEqual(history.length, PROJECT_PROMPT_HISTORY_LIMIT);
  assert.strictEqual(history[0]?.text, `prompt-${PROJECT_PROMPT_HISTORY_LIMIT + 4}`);
  assert.strictEqual(history[history.length - 1]?.text, 'prompt-5');
  assert.strictEqual(history.some((item) => item.text === 'stale prompt'), false);
});
