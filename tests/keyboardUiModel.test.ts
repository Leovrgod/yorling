import test from 'node:test';
import assert from 'node:assert/strict';
import { buildKeyboardLayerViews } from '../src/components/keyboard/keyboardUiModel.ts';
import { defaultRules } from '../src/components/keyboard/keyboardMappingData.ts';

test('builds four primary keyboard layers including the Tab mouse mode', () => {
  const { layers } = buildKeyboardLayerViews(defaultRules, 'en');

  assert.deepEqual(
    layers.map((layer) => layer.id),
    ['space', 'number', 'symbol', 'mouse'],
  );

  const mouseLayer = layers.find((layer) => layer.id === 'mouse');
  assert.ok(mouseLayer);
  assert.strictEqual(mouseLayer?.modifierKey, 'Tab');
  assert.strictEqual(mouseLayer?.keyTargets.J?.display, '←');
  assert.strictEqual(mouseLayer?.keyTargets.J?.description, 'Mouse Left');
  assert.strictEqual(mouseLayer?.keyTargets.N?.description, 'Left Click');
  assert.strictEqual(mouseLayer?.keyTargets.M?.description, 'Right Click');
  assert.strictEqual(mouseLayer?.keyTargets.U?.display, '⇡');
  assert.strictEqual(mouseLayer?.keyTargets.U?.description, 'Scroll Up');
  assert.strictEqual(mouseLayer?.keyTargets.O?.display, '⇣');
  assert.strictEqual(mouseLayer?.keyTargets.O?.description, 'Scroll Down');
});

test('includes mouse back navigation on R in the Tab mouse mode', () => {
  const { layers } = buildKeyboardLayerViews(defaultRules, 'en');
  const mouseLayer = layers.find((layer) => layer.id === 'mouse');

  assert.ok(mouseLayer);
  assert.strictEqual(mouseLayer?.keyTargets.R?.display, 'Back');
  assert.strictEqual(mouseLayer?.keyTargets.R?.description, 'Mouse Back');
});

test('includes sidebar toggling on C in the Tab mouse mode', () => {
  const { layers } = buildKeyboardLayerViews(defaultRules, 'en');
  const mouseLayer = layers.find((layer) => layer.id === 'mouse');

  assert.ok(mouseLayer);
  assert.strictEqual(mouseLayer?.keyTargets.C?.display, '⌘[');
  assert.strictEqual(mouseLayer?.keyTargets.C?.description, 'Toggle Sidebar');
});

test('keeps semicolon tap combos as supplemental combos instead of fake physical keys', () => {
  const { layers } = buildKeyboardLayerViews(defaultRules, 'en');
  const symbolLayer = layers.find((layer) => layer.id === 'symbol');

  assert.ok(symbolLayer);
  assert.deepEqual(
    symbolLayer?.combos.map((combo) => [combo.trigger, combo.output]),
    [
      ['XK', '()'],
      ['ZK', '[]'],
      ['DK', '{}'],
    ],
  );
  assert.strictEqual(symbolLayer?.keyTargets.XK, undefined);
});

test('separates shortcut-style mappings into the supplemental other rules panel', () => {
  const { otherRules } = buildKeyboardLayerViews(defaultRules, 'en');

  assert.deepEqual(
    otherRules.map((rule) => rule.id),
    ['win-ctrl-c', 'win-ctrl-v', 'win-ctrl-x', 'win-ctrl-s', 'win-shift-screenshot', 'win-ctrl-z', 'win-ctrl-a', 'win-alt-tab'],
  );
});
