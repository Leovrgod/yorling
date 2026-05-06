import test from 'node:test';
import assert from 'node:assert/strict';
import {
  promoteIslandOpenReasonForInternalInteraction,
  resolveExpandedPanelHeight,
  shouldAutoCollapseExpandedIsland,
  shouldUseDomMouseLeave,
  shouldExpandForSessionSelection,
} from '../src/island/panelState.ts';

test('does not redundantly re-expand the island when selecting a session from an already expanded panel', () => {
  assert.strictEqual(shouldExpandForSessionSelection('expanded'), false);
  assert.strictEqual(shouldExpandForSessionSelection('collapsed'), true);
});

test('uses DOM mouseleave for bounded Windows island windows but not macOS full-width panels', () => {
  assert.strictEqual(shouldUseDomMouseLeave('expanded', 'hover'), false);
  assert.strictEqual(shouldUseDomMouseLeave('expanded', 'hover', true), true);
  assert.strictEqual(shouldUseDomMouseLeave('collapsed', 'hover'), true);
  assert.strictEqual(shouldUseDomMouseLeave('expanded', 'click'), true);
});

test('promotes internal interaction on a hover-opened expanded island to click mode', () => {
  assert.strictEqual(
    promoteIslandOpenReasonForInternalInteraction('expanded', 'hover'),
    'click',
  );
  assert.strictEqual(
    promoteIslandOpenReasonForInternalInteraction('expanded', 'click'),
    'click',
  );
  assert.strictEqual(
    promoteIslandOpenReasonForInternalInteraction('collapsed', 'hover'),
    'hover',
  );
});

test('keeps attention panels open until the request is resolved or explicitly dismissed', () => {
  assert.strictEqual(shouldAutoCollapseExpandedIsland(true), false);
  assert.strictEqual(shouldAutoCollapseExpandedIsland(false), true);
});

test('prefers the active panel height so returning from chat can shrink immediately', () => {
  assert.strictEqual(
    resolveExpandedPanelHeight({
      activeSurfaceScrollHeight: 228,
      activeSurfaceOffsetHeight: 224,
      activeInnerScrollHeight: 216,
      previousMeasuredHeight: 548,
    }),
    228,
  );
});

test('keeps the previous measured height when the active surface has not rendered usable dimensions yet', () => {
  assert.strictEqual(
    resolveExpandedPanelHeight({
      activeSurfaceScrollHeight: 0,
      activeSurfaceOffsetHeight: 0,
      activeInnerScrollHeight: 0,
      previousMeasuredHeight: 548,
    }),
    548,
  );
});
