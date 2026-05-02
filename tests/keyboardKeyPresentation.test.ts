import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { estimateVisualLength, getKeyboardTextDensity } from '../src/components/keyboard/keyboardKeyMetrics.ts';

test('uses a single keyboard unit so center alpha keys render as square caps', () => {
  const stylesSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(stylesSource.includes('--keyboard-unit: clamp('), true);
  assert.strictEqual(stylesSource.includes('width: calc(var(--keyboard-unit) * var(--key-flex));'), true);
  assert.strictEqual(stylesSource.includes('height: var(--keyboard-unit);'), true);
  assert.strictEqual(stylesSource.includes('--keyboard-unit-y'), false);
});

test('shrinks long keyboard labels gently across English and Chinese copy', () => {
  assert.strictEqual(getKeyboardTextDensity('Hold', 'display'), 'regular');
  assert.strictEqual(getKeyboardTextDensity('⌘⇧Tab', 'display'), 'balanced');
  assert.strictEqual(getKeyboardTextDensity('Delete Word', 'description'), 'balanced');
  assert.strictEqual(getKeyboardTextDensity('Window Switcher', 'description'), 'compact');
  assert.strictEqual(getKeyboardTextDensity('窗口切换器', 'description'), 'regular');
  assert.strictEqual(estimateVisualLength('Delete Word') > estimateVisualLength('删除单词'), true);
});
