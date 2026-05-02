import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  parseStoredKeyboardAutoEnable,
  parseStoredKeyboardAutoStart,
} from '../src/stores/keyboardStore.ts';

test('defaults missing persisted keyboard startup preferences to stopped but enabled-ready', () => {
  assert.strictEqual(parseStoredKeyboardAutoStart(null), false);
  assert.strictEqual(parseStoredKeyboardAutoStart(''), false);
  assert.strictEqual(parseStoredKeyboardAutoEnable(null), true);
  assert.strictEqual(parseStoredKeyboardAutoEnable(''), true);
});

test('restores the keyboard interceptor on app boot without persisting transient runtime changes', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const hookSource = readFileSync('src/hooks/useTauriCommand.ts', 'utf8');
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');

  assert.ok(appSource.includes('useKeyboardStartupRestore();'));
  assert.ok(hookSource.includes('export function useKeyboardStartupRestore()'));
  assert.ok(hookSource.includes("await startInterceptor({ persistPreference: false });"));
  assert.ok(hookSource.includes("await setEnabled(autoEnable, { persistPreference: false });"));
  assert.ok(musicSource.includes("setEnabled(false, { persistPreference: false });"));
  assert.ok(musicSource.includes("setEnabled(true, { persistPreference: false });"));
});
