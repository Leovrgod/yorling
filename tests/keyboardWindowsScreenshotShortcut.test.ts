import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  defaultRules,
  getKeyboardMappingRules,
} from '../src/components/keyboard/keyboardMappingData.ts';
import { getEnabledModulesForPlatform } from '../src/utils/platform.ts';

test('keeps the mac compatibility screenshot shortcut out of Windows rules', () => {
  const screenshotRule = defaultRules.find((rule) => rule.id === 'win-shift-screenshot');
  const windowsRules = getKeyboardMappingRules('windows');

  assert.ok(screenshotRule);
  assert.strictEqual(screenshotRule?.modifier, 'Win+Shift');
  assert.strictEqual(screenshotRule?.to, 'Cmd+Shift+4');
  assert.strictEqual(screenshotRule?.toDisplay, '⌘⇧4');
  assert.strictEqual(windowsRules.some((rule) => rule.id === 'win-shift-screenshot'), false);
  assert.strictEqual(windowsRules.some((rule) => rule.id === 'win-alt-tab'), false);
  assert.strictEqual(windowsRules.some((rule) => rule.id === 'win-ctrl-c'), false);
});

test('adapts layer targets to Windows-native editing semantics', () => {
  const windowsRules = getKeyboardMappingRules('windows');
  const byId = new Map(windowsRules.map((rule) => [rule.id, rule]));

  assert.strictEqual(byId.get('nav-h')?.to, 'Home');
  assert.strictEqual(byId.get('nav-n')?.to, 'End');
  assert.strictEqual(byId.get('nav-u')?.to, 'Ctrl+Left');
  assert.strictEqual(byId.get('nav-o')?.to, 'Ctrl+Right');
  assert.strictEqual(byId.get('edit-r')?.to, 'Ctrl+Backspace');
  assert.strictEqual(byId.get('mouse-tab-c')?.to, 'Ctrl+B');
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

test('keeps Windows startup scoped to the keyboard module without the island overlay', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

  assert.deepEqual(getEnabledModulesForPlatform('windows'), ['keyboard']);
  assert.strictEqual(appSource.includes('activePlatformModule'), true);
  assert.strictEqual(appSource.includes("setActiveModule(activePlatformModule)"), true);
  assert.strictEqual(libSource.includes('const fn should_start_island_runtime() -> bool'), true);
  assert.strictEqual(libSource.includes('cfg!(target_os = "macos")'), true);
  assert.strictEqual(libSource.includes('Skipping island runtime on this platform'), true);
});
