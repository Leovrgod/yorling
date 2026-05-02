import test from 'node:test';
import assert from 'node:assert/strict';
import type { IslandSession } from '../src/types/index.ts';
import {
  formatCwdSegments,
  getCollapsedTickerItems,
  getSessionProjectLabel,
  getSessionActivity,
  getSessionTitle,
  getVisiblePromptMessages,
  isGenericSessionHeading,
} from '../src/island/presentation.ts';

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

test('merges runtime chat history with transcript preview while deduplicating prompts', () => {
  const session = makeSession({
    id: 's1',
    chat_messages: [
      { role: 'user', text: '先看灵动岛状态', timestamp: 1000 },
      { role: 'assistant', text: '我先检查一下。', timestamp: 1010 },
    ],
    transcript_preview: {
      transcript_path: '/tmp/session.jsonl',
      provider_session_id: null,
      status: 'synced',
      synced_at: 2000,
      turn_count: 2,
      latest_task_started_at: null,
      latest_task_finished_at: null,
      latest_pending_question: null,
      last_error: null,
      chat_preview: [
        { role: 'user', text: '先看灵动岛状态', timestamp: 1000 },
        { role: 'user', text: '再看 Copilot 适配', timestamp: 1020 },
      ],
      tool_history: [],
    },
  });

  const prompts = getVisiblePromptMessages(session);

  assert.deepEqual(
    prompts.map((message) => message.text),
    ['先看灵动岛状态', '再看 Copilot 适配'],
  );
});

test('humanizes live activity and varies the wording by provider', () => {
  const toolHistory = [
    {
      id: 'tool-1',
      tool: 'Bash',
      state: 'started' as const,
      input_preview: '{"command":"rg -n \\"activity\\" src/island"}',
      output_preview: null,
      started_at: 1115,
      finished_at: null,
    },
  ];
  const codexSession = makeSession({
    id: 'codex',
    provider_id: 'codex',
    transcript_preview: {
      transcript_path: '/tmp/codex.jsonl',
      provider_session_id: 'codex-1',
      status: 'synced',
      synced_at: 2000,
      turn_count: 1,
      latest_task_started_at: null,
      latest_task_finished_at: null,
      latest_pending_question: null,
      last_error: null,
      chat_preview: [],
      tool_history: toolHistory,
    },
  });
  const claudeSession = makeSession({
    id: 'claude',
    provider_id: 'claude-code',
    transcript_preview: {
      transcript_path: '/tmp/claude.jsonl',
      provider_session_id: 'claude-1',
      status: 'synced',
      synced_at: 2000,
      turn_count: 1,
      latest_task_started_at: null,
      latest_task_finished_at: null,
      latest_pending_question: null,
      last_error: null,
      chat_preview: [],
      tool_history: toolHistory,
    },
  });

  const codexActivity = getSessionActivity(codexSession);
  const claudeActivity = getSessionActivity(claudeSession);

  assert.ok(codexActivity?.isHumanized);
  assert.ok(claudeActivity?.isHumanized);
  assert.ok(codexActivity?.text.startsWith('+ '));
  assert.ok(!codexActivity?.text.includes('rg -n'));
  assert.ok(codexActivity?.text !== claudeActivity?.text);
});

test('falls back to the concrete command when no humanized phrase can be inferred', () => {
  const session = makeSession({
    id: 's-fallback',
    provider_id: 'copilot',
    transcript_preview: {
      transcript_path: '/tmp/fallback.jsonl',
      provider_session_id: 'copilot-1',
      status: 'synced',
      synced_at: 2000,
      turn_count: 1,
      latest_task_started_at: null,
      latest_task_finished_at: null,
      latest_pending_question: null,
      last_error: null,
      chat_preview: [],
      tool_history: [
        {
          id: 'tool-1',
          tool: 'mystery_tool',
          state: 'started',
          input_preview: '{"command":"custom-build --fast"}',
          output_preview: null,
          started_at: 1115,
          finished_at: null,
        },
      ],
    },
  });

  const activity = getSessionActivity(session);

  assert.ok(activity);
  assert.strictEqual(activity?.isHumanized, false);
  assert.strictEqual(activity?.text, 'custom-build --fast');
});

test('collapsed ticker prefers short humanized activity over raw prompts when available', () => {
  const sessions = [
    makeSession({
      id: 'copilot',
      provider_id: 'copilot',
      chat_messages: [
        { role: 'user', text: '检查命令选择适配', timestamp: 1100 },
        { role: 'assistant', text: '我先定位 Copilot 的 hook 与 transcript。', timestamp: 1110 },
      ],
      transcript_preview: {
        transcript_path: '/tmp/copilot.jsonl',
        provider_session_id: 'copilot-1',
        status: 'synced',
        synced_at: 2000,
        turn_count: 2,
        latest_task_started_at: null,
        latest_task_finished_at: null,
        latest_pending_question: null,
        last_error: null,
        chat_preview: [],
        tool_history: [
          {
            id: 'tool-1',
            tool: 'view',
            state: 'started',
            input_preview: '{"path":"src/island/components/ChatView.tsx"}',
            output_preview: null,
            started_at: 1115,
            finished_at: null,
          },
        ],
      },
    }),
  ];

  const items = getCollapsedTickerItems(sessions);

  assert.strictEqual(items.length, 1);
  assert.ok(items[0].isHumanized);
  assert.ok(items[0].text.startsWith('+ '));
  assert.ok(!items[0].text.includes('检查命令选择适配'));
});

test('falls back to the latest prompt as the session title when task_title is missing', () => {
  const session = makeSession({
    id: 's2',
    task_title: null,
    chat_messages: [
      { role: 'user', text: '让灵动岛真正灵动起来', timestamp: 1000 },
    ],
  });

  assert.strictEqual(getSessionTitle(session), '让灵动岛真正灵动起来');
});

test('formats cwd labels using the last two path segments', () => {
  assert.strictEqual(formatCwdSegments('/Users/example/yorling/yorling'), 'yorling/yorling');
  assert.strictEqual(formatCwdSegments('/Users/example/proj/sub'), 'proj/sub');
  assert.strictEqual(formatCwdSegments('/foo'), 'foo');
  assert.strictEqual(formatCwdSegments(''), null);
  assert.strictEqual(formatCwdSegments(undefined), null);
});

test('session project label uses cwd segments and hides exact title duplicates', () => {
  const session = makeSession({
    id: 's-project',
    task_title: '修复灵动岛',
    terminal_context: {
      cwd: '/Users/example/yorling/yorling',
    },
  });
  const duplicateTitleSession = makeSession({
    id: 's-duplicate-project',
    task_title: 'yorling/yorling',
    terminal_context: {
      cwd: '/Users/example/yorling/yorling',
    },
  });

  assert.strictEqual(getSessionProjectLabel(session), 'yorling/yorling');
  assert.strictEqual(getSessionProjectLabel(duplicateTitleSession), null);
});

test('treats provider-only headings as generic across agent categories', () => {
  assert.strictEqual(isGenericSessionHeading('Claude Code'), true);
  assert.strictEqual(isGenericSessionHeading('Hermes'), true);
  assert.strictEqual(isGenericSessionHeading('OpenCode'), true);
  assert.strictEqual(isGenericSessionHeading(' Qwen Code · '), true);
  assert.strictEqual(isGenericSessionHeading('调查一下今天的 AI 新闻'), false);
});
