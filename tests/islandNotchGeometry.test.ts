import test from 'node:test';
import assert from 'node:assert/strict';
import {
  DEFAULT_MAX_PANEL_HEIGHT,
  ISLAND_FALLBACK_CLOSED_HEIGHT,
  ISLAND_FALLBACK_CLOSED_WIDTH,
  getIslandSurfaceMetrics,
  getOpenedIslandSize,
} from '../src/island/notchGeometry.ts';
import type { IslandScreenInfo } from '../src/types/index.ts';

function makeScreenInfo(overrides: Partial<IslandScreenInfo> = {}): IslandScreenInfo {
  return {
    screen_name: 'Built-in Display',
    has_notch: false,
    screen_width: 1440,
    screen_height: 900,
    notch_rect: null,
    scale_factor: 2,
    is_builtin: true,
    placement_mode: 'top_bar',
    top_inset: 32,
    closed_width: 266,
    closed_height: 32,
    ...overrides,
  };
}

test('keeps the collapsed island defaults and width stable', () => {
  const metrics = getIslandSurfaceMetrics({
    progress: 0,
    sessionCount: 4,
    screenInfo: null,
    openReason: null,
    panelMode: 'sessions',
    measuredExpandedHeight: null,
  });

  assert.strictEqual(ISLAND_FALLBACK_CLOSED_WIDTH, 266);
  assert.strictEqual(ISLAND_FALLBACK_CLOSED_HEIGHT, 32);
  assert.strictEqual(metrics.width, 266);
  assert.strictEqual(metrics.height, 32);
});

test('keeps the island top corners rounded even when macOS reserves a top inset', () => {
  const topBarMetrics = getIslandSurfaceMetrics({
    progress: 0,
    sessionCount: 0,
    screenInfo: makeScreenInfo({ placement_mode: 'top_bar', top_inset: 32 }),
    openReason: null,
    panelMode: 'sessions',
    measuredExpandedHeight: null,
  });
  const floatingTopBarMetrics = getIslandSurfaceMetrics({
    progress: 0,
    sessionCount: 0,
    screenInfo: makeScreenInfo({ placement_mode: 'top_bar', top_inset: 0 }),
    openReason: null,
    panelMode: 'sessions',
    measuredExpandedHeight: null,
  });
  const expandedTopBarMetrics = getIslandSurfaceMetrics({
    progress: 1,
    sessionCount: 0,
    screenInfo: makeScreenInfo({ placement_mode: 'top_bar', top_inset: 32 }),
    openReason: 'click',
    panelMode: 'sessions',
    measuredExpandedHeight: null,
  });
  const notchMetrics = getIslandSurfaceMetrics({
    progress: 0,
    sessionCount: 0,
    screenInfo: makeScreenInfo({ placement_mode: 'notch' }),
    openReason: null,
    panelMode: 'sessions',
    measuredExpandedHeight: null,
  });

  assert.strictEqual(topBarMetrics.topRadius, 6);
  assert.strictEqual(floatingTopBarMetrics.topRadius, 6);
  assert.strictEqual(notchMetrics.topRadius, 6);
  assert.strictEqual(expandedTopBarMetrics.topRadius, 19);
});


test('keeps sessions and chat presentations on the optimized opened widths', () => {
  const screenInfo = makeScreenInfo({
    has_notch: true,
    placement_mode: 'notch',
    screen_width: 1512,
    screen_height: 982,
    top_inset: 38,
    closed_width: 276,
    closed_height: 38,
    notch_rect: {
      x: 618,
      y: 0,
      width: 276,
      height: 38,
    },
  });

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'hover',
      panelMode: 'sessions',
      measuredExpandedHeight: null,
    }),
    { width: 680, height: 200 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'attention',
      panelMode: 'sessions',
      measuredExpandedHeight: 332,
    }),
    { width: 680, height: 332 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'click',
      panelMode: 'sessions',
      measuredExpandedHeight: 332,
    }),
    { width: 680, height: 332 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'shortcut',
      panelMode: 'sessions',
      measuredExpandedHeight: 332,
    }),
    { width: 680, height: 332 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'click',
      panelMode: 'chatview',
      measuredExpandedHeight: null,
    }),
    { width: 680, height: 340 },
  );
});

test('caps opened island dimensions against smaller screens', () => {
  const screenInfo = makeScreenInfo({
    screen_width: 960,
    screen_height: 640,
    top_inset: 32,
    closed_width: 266,
    closed_height: 32,
  });

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'click',
      panelMode: 'sessions',
      measuredExpandedHeight: 420,
    }),
    { width: 680, height: 420 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'click',
      panelMode: 'chatview',
      measuredExpandedHeight: null,
    }),
    { width: 680, height: 340 },
  );
});

test('ignores transient tiny measured heights while the island is animating open', () => {
  const screenInfo = makeScreenInfo({
    screen_width: 1512,
    screen_height: 982,
    top_inset: 38,
    closed_width: 276,
    closed_height: 38,
  });

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'hover',
      panelMode: 'sessions',
      measuredExpandedHeight: 52,
    }),
    { width: 680, height: 200 },
  );

  assert.deepEqual(
    getOpenedIslandSize({
      screenInfo,
      openReason: 'click',
      panelMode: 'sessions',
      measuredExpandedHeight: 64,
    }),
    { width: 680, height: 200 },
  );
});
