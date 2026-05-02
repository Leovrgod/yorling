import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  getAltTabAppLabel,
  getAltTabTitleBarHeight,
} from '../src/components/keyboard/altTabOverlayTileMeta.ts';

test('uses the application name for the thumbnail title bar when available', () => {
  assert.strictEqual(
    getAltTabAppLabel({
      app_name: 'Microsoft Edge',
      title: '哎呦 - 小红书 和另外 5 个页面',
    }),
    'Microsoft Edge',
  );
});

test('falls back to the trimmed window title when the application name is blank', () => {
  assert.strictEqual(
    getAltTabAppLabel({
      app_name: '   ',
      title: '  豆包  ',
    }),
    '豆包',
  );
});

test('falls back to a stable generic label when both names are blank', () => {
  assert.strictEqual(
    getAltTabAppLabel({
      app_name: '   ',
      title: '   ',
    }),
    'Window',
  );
});

test('keeps the title bar in a thin readable height range', () => {
  assert.strictEqual(getAltTabTitleBarHeight(96), 22);
  assert.strictEqual(getAltTabTitleBarHeight(240), 26);
  assert.strictEqual(getAltTabTitleBarHeight(480), 30);
  assert.strictEqual(getAltTabTitleBarHeight(0), 22);
});

test('scales the title bar app name font with the thumbnail height and bumps it up two sizes', async () => {
  const meta = (await import('../src/components/keyboard/altTabOverlayTileMeta.ts')) as Record<
    string,
    unknown
  >;
  const getAltTabTitleBarFontSize = meta.getAltTabTitleBarFontSize;

  assert.strictEqual(typeof getAltTabTitleBarFontSize, 'function');

  if (typeof getAltTabTitleBarFontSize !== 'function') {
    return;
  }

  assert.strictEqual(getAltTabTitleBarFontSize(96), 12);
  assert.strictEqual(getAltTabTitleBarFontSize(240), 14);
  assert.strictEqual(getAltTabTitleBarFontSize(480), 16);
  assert.strictEqual(getAltTabTitleBarFontSize(0), 12);
});

test('wires the responsive app name font size into the alt-tab title bar styles', () => {
  const overlaySource = readFileSync('src/components/keyboard/AltTabOverlay.tsx', 'utf8');
  const componentStyles = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(overlaySource.includes("'--alt-tab-titlebar-font-size'"), true);
  assert.strictEqual(
    componentStyles.includes('font-size: var(--alt-tab-titlebar-font-size'),
    true,
  );
});

test('renders a small app icon slot to the left of the title bar label', () => {
  const overlaySource = readFileSync('src/components/keyboard/AltTabOverlay.tsx', 'utf8');
  const componentStyles = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(overlaySource.includes('alt-tab-overlay-app-icon'), true);
  assert.strictEqual(overlaySource.includes('iconsByPid.get(windowInfo.owner_pid)'), true);
  assert.strictEqual(overlaySource.includes('alt-tab-item-app-icon-placeholder'), true);
  assert.strictEqual(componentStyles.includes('.alt-tab-item-titlebar-app'), true);
  assert.strictEqual(componentStyles.includes('.alt-tab-item-app-icon'), true);
});
