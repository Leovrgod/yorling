import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { defaultRules } from '../src/components/keyboard/keyboardMappingData.ts';

test('declares the windows screenshot shortcut in the keyboard mapping data', () => {
  const screenshotRule = defaultRules.find((rule) => rule.id === 'win-shift-screenshot');

  assert.ok(screenshotRule);
  assert.strictEqual(screenshotRule?.modifier, 'Win+Shift');
  assert.strictEqual(screenshotRule?.to, 'Cmd+Shift+4');
  assert.strictEqual(screenshotRule?.toDisplay, '⌘⇧4');
});

test('renders supplemental shortcut mappings outside the primary keyboard layers', () => {
  const mappingSource = readFileSync('src/components/keyboard/KeyboardMapping.tsx', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(mappingSource.includes('other-mappings-list'), true);
  assert.strictEqual(mappingSource.includes('otherRules.map'), true);
  assert.strictEqual(mappingSource.includes('运行状态'), false);
  assert.strictEqual(mappingSource.includes('Win + Shift + S'), false);
  assert.strictEqual(stylesSource.includes('--keyboard-unit'), true);
  assert.strictEqual(stylesSource.includes('overflow-x: auto;'), true);
  assert.strictEqual(stylesSource.includes('--keycap-bg-start'), true);
});
