import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('wires Windows tray icon to keyboard mapping controls', () => {
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const keyboardCommandSource = readFileSync('src-tauri/src/commands/keyboard.rs', 'utf8');
  const traySource = readFileSync('src-tauri/src/windows_tray.rs', 'utf8');
  const hookSource = readFileSync('src/hooks/useTauriCommand.ts', 'utf8');

  assert.strictEqual(libSource.includes('mod windows_tray;'), true);
  assert.strictEqual(libSource.includes('windows_tray::setup(app.handle(), state.inner().clone())'), true);
  assert.strictEqual(traySource.includes('.show_menu_on_left_click(false)'), true);
  assert.strictEqual(traySource.includes('MouseButton::Left'), true);
  assert.strictEqual(traySource.includes('MouseButtonState::Down'), true);
  assert.strictEqual(traySource.includes('toggle_windows_keyboard_mapping'), true);
  assert.strictEqual(traySource.includes('tray.set_icon(Some(tray_image(active)?))'), true);
  assert.strictEqual(traySource.includes('dim_icon(icon)'), true);
  assert.strictEqual(traySource.includes('QUIT_MENU_ID'), true);
  assert.strictEqual(traySource.includes('app.exit(0)'), true);

  assert.strictEqual(keyboardCommandSource.includes('windows_keyboard_mapping_active'), true);
  assert.strictEqual(keyboardCommandSource.includes('crate::windows_tray::refresh(&app'), true);
  assert.strictEqual(hookSource.includes("listen<boolean>('keyboard-tray-status-changed'"), true);
});
