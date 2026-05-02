import test from 'node:test';
import assert from 'node:assert/strict';
import { toggleWindowZoom } from '../src/utils/windowControls.ts';

function createWindowTarget({
  fullscreen = false,
  maximized = false,
}: {
  fullscreen?: boolean;
  maximized?: boolean;
}) {
  const calls: Array<string> = [];

  return {
    calls,
    window: {
      async isFullscreen() {
        calls.push('isFullscreen');
        return fullscreen;
      },
      async setFullscreen(next: boolean) {
        calls.push(`setFullscreen:${String(next)}`);
      },
      async isMaximized() {
        calls.push('isMaximized');
        return maximized;
      },
      async maximize() {
        calls.push('maximize');
      },
      async unmaximize() {
        calls.push('unmaximize');
      },
    },
  };
}

test('uses native fullscreen entry for the macOS green window control', async () => {
  const target = createWindowTarget({ fullscreen: false });

  await toggleWindowZoom(target.window, 'MacIntel');

  assert.deepEqual(target.calls, ['isFullscreen', 'setFullscreen:true']);
});

test('exits native fullscreen on macOS before any maximize fallback', async () => {
  const target = createWindowTarget({ fullscreen: true });

  await toggleWindowZoom(target.window, 'MacIntel');

  assert.deepEqual(target.calls, ['isFullscreen', 'setFullscreen:false']);
});

test('restores a maximized window on non-mac platforms', async () => {
  const target = createWindowTarget({ maximized: true });

  await toggleWindowZoom(target.window, 'Win32');

  assert.deepEqual(target.calls, ['isMaximized', 'unmaximize']);
});

test('maximizes a restored window on non-mac platforms', async () => {
  const target = createWindowTarget({ maximized: false });

  await toggleWindowZoom(target.window, 'Linux x86_64');

  assert.deepEqual(target.calls, ['isMaximized', 'maximize']);
});
