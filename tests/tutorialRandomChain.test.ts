import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync('src/components/music/TutorialPanel.tsx', 'utf8');

test('keeps random chaining in demo mode after a demo song finishes', () => {
  assert.strictEqual(source.includes("continueWithRandomSong('demo')"), true);
  assert.strictEqual(source.includes("if (pendingAutoStartMode === 'demo')"), true);
  assert.strictEqual(source.includes('startDemo();'), true);
});
