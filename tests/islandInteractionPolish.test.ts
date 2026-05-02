import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('keeps island detail scrolling natural while staying anchored to the latest activity', () => {
  const chatSource = readFileSync('src/island/components/ChatView.tsx', 'utf8');
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');

  assert.ok(!chatSource.includes('[...chatPreview].reverse()'));
  assert.ok(chatSource.includes('el.scrollHeight - el.clientHeight - el.scrollTop'));
  assert.ok(chatSource.includes('scrollTo({ top: el.scrollHeight'));
  assert.ok(!islandStyles.includes('scaleY(-1)'));
});

test('hands off to collapsed content during close and adds hover-leave recovery for hover-opened islands', () => {
  const islandContentSource = readFileSync('src/island/IslandContent.tsx', 'utf8');

  assert.ok(islandContentSource.includes("viewMode === 'collapsed' && progress > 0.02"));
  assert.ok(islandContentSource.includes('isMorphingToCollapsed'));
  assert.ok(islandContentSource.includes("window.addEventListener('mouseleave', handleWindowMouseLeave)"));
  assert.ok(islandContentSource.includes('getBoundingClientRect()'));
  assert.ok(islandContentSource.includes('island-content-surface--ghost'));
});

test('pins a hover-opened expanded island when the user clicks inside it', () => {
  const islandContentSource = readFileSync('src/island/IslandContent.tsx', 'utf8');
  const panelStateSource = readFileSync('src/island/panelState.ts', 'utf8');

  assert.ok(
    islandContentSource.includes('onMouseDownCapture={handleExpandedMouseDownCapture}'),
  );
  assert.ok(
    islandContentSource.includes('promoteIslandOpenReasonForInternalInteraction'),
  );
  assert.ok(
    panelStateSource.includes('promoteIslandOpenReasonForInternalInteraction'),
  );
});

test('lets island panels use viewport-driven height caps and lifts the tutorial results card upward', () => {
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');
  const geometrySource = readFileSync('src/island/notchGeometry.ts', 'utf8');
  const tutorialStyles = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(!islandStyles.includes('max-height: min(420px, calc(100vh - 170px));'));
  assert.ok(!islandStyles.includes('max-height: min(360px, calc(100vh - 220px));'));
  assert.ok(islandStyles.includes('max-height: calc(100vh - 170px);'));
  assert.ok(islandStyles.includes('max-height: calc(100vh - 190px);'));
  assert.ok(geometrySource.includes('DEFAULT_CHATVIEW_HEIGHT = 340'));
  assert.ok(tutorialStyles.includes('padding: 24px 18px 56px;'));
});
