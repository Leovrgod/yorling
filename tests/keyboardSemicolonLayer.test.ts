import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { defaultRules } from '../src/components/keyboard/keyboardMappingData.ts';

test('declares the semicolon symbol layer rules in the keyboard mapping data', () => {
  const symbolRules = defaultRules.filter((rule) => rule.category === 'symbol');

  assert.strictEqual(symbolRules.some((rule) => rule.id === 'sym-semicolon-a'), true);
  assert.strictEqual(symbolRules.some((rule) => rule.id === 'sym-semicolon-j'), true);
  assert.strictEqual(symbolRules.some((rule) => rule.id === 'sym-semicolon-z'), true);
  assert.strictEqual(symbolRules.some((rule) => rule.id === 'sym-semicolon-tap-round'), true);
  assert.strictEqual(symbolRules.some((rule) => rule.toDisplay === '@'), true);
  assert.strictEqual(symbolRules.some((rule) => rule.toDisplay === '\\'), true);
});

test('builds the keyboard mapping view around layer switching instead of quick-reference prose', () => {
  const mappingSource = readFileSync('src/components/keyboard/KeyboardMapping.tsx', 'utf8');

  assert.strictEqual(mappingSource.includes('layer-switcher'), true);
  assert.strictEqual(mappingSource.includes('keyboard-stage'), true);
  assert.strictEqual(mappingSource.includes('keyboard-viewport'), true);
  assert.strictEqual(mappingSource.includes('buildKeyboardLayerViews'), true);
  assert.strictEqual(mappingSource.includes('Layer Notes'), false);
  assert.strictEqual(mappingSource.includes('Space / 3 / ; + Key'), false);
});
