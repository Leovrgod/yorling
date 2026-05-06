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
  assert.strictEqual(byId.get('edit-w')?.to, 'Home→Select→Backspace');
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
  assert.strictEqual(mappingSource.includes('windowsElevationTitle'), false);
  assert.strictEqual(stylesSource.includes('--keyboard-unit'), true);
  assert.strictEqual(stylesSource.includes('overflow-x: auto;'), true);
  assert.strictEqual(stylesSource.includes('--keycap-bg-start'), true);
});

test('keeps Windows line-delete modifier held across the whole End key press', () => {
  const windowsKeyboardSource = readFileSync('src-tauri/src/windows_keyboard.rs', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');

  assert.strictEqual(copySource.includes('windowsElevationTitle'), false);
  assert.strictEqual(windowsKeyboardSource.includes('fn emit_synthetic_key_press'), true);
  assert.strictEqual(windowsKeyboardSource.includes('KEYEVENTF_EXTENDEDKEY'), true);
  assert.match(
    windowsKeyboardSource,
    /emit_synthetic_key_press\(key\.keycode, desired_flags, physical_flags\);[\s\S]*index \+= 2;/,
  );
  assert.match(
    windowsKeyboardSource,
    /push_keyboard_input\(&mut inputs, vk, true\);\s*push_keyboard_input\(&mut inputs, vk, false\);\s*push_modifier_inputs\(&mut inputs, modifiers_to_press, false\);/,
  );
});

test('keeps Windows startup scoped to keyboard, music, clipboard, and island modules', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

  assert.deepEqual(getEnabledModulesForPlatform('windows'), ['keyboard', 'music', 'clipboard', 'island']);
  assert.strictEqual(appSource.includes('activePlatformModule'), true);
  assert.strictEqual(appSource.includes("setActiveModule(activePlatformModule)"), true);
  assert.strictEqual(libSource.includes('const fn should_start_island_runtime() -> bool'), true);
  assert.strictEqual(libSource.includes('cfg!(any(target_os = "macos", target_os = "windows"))'), true);
  assert.strictEqual(libSource.includes('Skipping island runtime on this platform'), true);
});

test('bypasses native Windows Alt+Tab before keyboard mapping state can stick', () => {
  const windowsKeyboardSource = readFileSync('src-tauri/src/windows_keyboard.rs', 'utf8');

  assert.strictEqual(windowsKeyboardSource.includes('LLKHF_ALTDOWN'), true);
  assert.strictEqual(windowsKeyboardSource.includes('fn is_native_alt_tab_event'), true);
  assert.strictEqual(windowsKeyboardSource.includes('fn clear_native_alt_tab_state'), true);
  assert.match(
    windowsKeyboardSource,
    /if is_native_alt_tab_event\(&context, keyboard\.vkCode as u16, key_down, keyboard\.flags\)\s*\{\s*clear_native_alt_tab_state\(&context\);\s*return unsafe \{ CallNextHookEx/s,
  );
  assert.match(
    windowsKeyboardSource,
    /physical_keys\.remove\(&\(VirtualKeyCode::Option as u16\)\);[\s\S]*physical_keys\.remove\(&\(VirtualKeyCode::Tab as u16\)\);/,
  );
});

test('does not query async keyboard state from inside the Windows keyboard hook', () => {
  const windowsKeyboardSource = readFileSync('src-tauri/src/windows_keyboard.rs', 'utf8');
  const keyboardHookMatch = windowsKeyboardSource.match(
    /unsafe extern "system" fn low_level_keyboard_proc[\s\S]*?\n}\n\nfn is_native_alt_tab_event/,
  );

  assert.ok(keyboardHookMatch);
  assert.strictEqual(windowsKeyboardSource.includes('GetAsyncKeyState'), false);
  assert.strictEqual(
    keyboardHookMatch?.[0].includes('update_physical_key_state(&context, keycode, key_down)'),
    true,
  );
  assert.strictEqual(keyboardHookMatch?.[0].includes('reset_engine_after_physical_desync'), false);
});

test('clears synthetic restored modifiers before Windows mouse clicks', () => {
  const windowsKeyboardSource = readFileSync('src-tauri/src/windows_keyboard.rs', 'utf8');

  assert.strictEqual(windowsKeyboardSource.includes('synthetic_restored_modifiers'), true);
  assert.strictEqual(windowsKeyboardSource.includes('fn release_synthetic_restored_modifiers'), true);
  assert.match(
    windowsKeyboardSource,
    /WM_LBUTTONDOWN \| WM_RBUTTONDOWN \| WM_MBUTTONDOWN \| WM_XBUTTONDOWN[\s\S]*release_synthetic_restored_modifiers\(&context\);[\s\S]*cancel_transient_modes_for_mouse_down\(&context\);/s,
  );
  assert.match(
    windowsKeyboardSource,
    /record_synthetic_modifier_restores\(context, physical_flags & !desired_flags\);[\s\S]*emit_synthetic_key\(key\.keycode, key\.key_down, desired_flags, physical_flags\);/s,
  );
});
