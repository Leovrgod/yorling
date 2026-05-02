import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('syncs island overlay language with the shared app language setting', () => {
  const islandI18nSource = readFileSync('src/island/i18n.ts', 'utf8');
  const islandWindowSource = readFileSync('src/island/IslandWindow.tsx', 'utf8');
  const appCopySource = readFileSync('src/i18n/copy.ts', 'utf8');

  assert.ok(appCopySource.includes("LANGUAGE_STORAGE_KEY = 'yorling.language'"));
  assert.ok(islandI18nSource.includes('getStoredLanguage'));
  assert.ok(islandWindowSource.includes("window.addEventListener('storage'"));
  assert.ok(islandWindowSource.includes('setIslandLang'));
});

test('localizes expanded session list copy and removes the Yorling Island eyebrow', () => {
  const sessionListSource = readFileSync('src/island/components/SessionList.tsx', 'utf8');
  const presentationSource = readFileSync('src/island/presentation.ts', 'utf8');

  assert.ok(!sessionListSource.includes('Yorling Island'));
  assert.ok(!sessionListSource.includes('No live sessions'));
  assert.ok(!sessionListSource.includes('No active sessions'));
  assert.ok(sessionListSource.includes("t('sessions.no_active'"));
  assert.ok(presentationSource.includes("t('sessions.no_live'"));
  assert.ok(presentationSource.includes("t('sessions.live_count'"));
});

test('uses colored phase accents and unit-only elapsed time badges in island cards', () => {
  const sessionCardSource = readFileSync('src/island/components/SessionCard.tsx', 'utf8');
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');

  assert.ok(!sessionCardSource.includes('m ago'));
  assert.ok(!sessionCardSource.includes('h ago'));
  assert.ok(!sessionCardSource.includes('just now'));
  assert.ok(!sessionCardSource.includes("case 'idle': return 'Idle';"));
  assert.ok(islandStyles.includes('--phase-idle-bg'));
  assert.ok(islandStyles.includes('--phase-processing-bg'));
  assert.ok(islandStyles.includes('--phase-waiting-bg'));
});

test('keeps island drill-down focused on user prompts and agent choice requests', () => {
  const chatViewSource = readFileSync('src/island/components/ChatView.tsx', 'utf8');
  const presentationSource = readFileSync('src/island/presentation.ts', 'utf8');

  assert.ok(!chatViewSource.includes('Session Info'));
  assert.ok(!chatViewSource.includes('Tool Calls'));
  assert.ok(!chatViewSource.includes('ToolHistoryEntry'));
  assert.ok(!chatViewSource.includes("msg.role === 'assistant'"));
  assert.ok(!chatViewSource.includes("msg.role === 'system'"));
  assert.ok(presentationSource.includes("message.role === 'user'"));
  assert.ok(chatViewSource.includes('pending_permission'));
  assert.ok(chatViewSource.includes('pending_question'));
  assert.ok(chatViewSource.includes('navigator.clipboard'));
  assert.ok(chatViewSource.includes("t('chatview.copy'"));
});

test('softens island permission danger accents and avoids surfacing raw unknown tool labels', () => {
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');
  const permissionCardSource = readFileSync('src/island/components/PermissionCard.tsx', 'utf8');
  const presentationSource = readFileSync('src/island/presentation.ts', 'utf8');

  assert.ok(islandStyles.includes('--island-alert-fg'));
  assert.ok(islandStyles.includes('--island-alert-bg'));
  assert.ok(islandStyles.includes('--island-alert-border'));
  assert.ok(!islandStyles.includes('.island-permission__btn--deny {\n   background: transparent;\n   border: 1px solid var(--nothing-red);'));
  assert.ok(permissionCardSource.includes('formatToolLabel'));
  assert.ok(presentationSource.includes("const UNKNOWN_TOOL_LABELS = new Set(['', 'unknown', 'unknown tool'])"));
});
