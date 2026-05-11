import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('wires Windows tray icon to left-click and menu keyboard mapping controls', () => {
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const keyboardCommandSource = readFileSync('src-tauri/src/commands/keyboard.rs', 'utf8');
  const traySource = readFileSync('src-tauri/src/windows_tray.rs', 'utf8');
  const hookSource = readFileSync('src/hooks/useTauriCommand.ts', 'utf8');

  assert.strictEqual(libSource.includes('mod windows_tray;'), true);
  assert.strictEqual(libSource.includes('mod windows_single_instance;'), true);
  assert.strictEqual(libSource.includes('windows_single_instance::claim_or_focus_existing'), true);
  assert.strictEqual(libSource.includes('windows_tray::setup(app.handle(), state.inner().clone())'), true);
  assert.strictEqual(traySource.includes('.show_menu_on_left_click(false)'), true);
  assert.strictEqual(traySource.includes('MouseButton::Left'), true);
  assert.strictEqual(traySource.includes('MouseButtonState::Down'), true);
  assert.match(
    traySource,
    /on_tray_icon_event[\s\S]*state_for_click\.toggle_windows_keyboard_mapping\(\)[\s\S]*update_tray\(tray, active\)[\s\S]*tray\.app_handle\(\)\.emit\(TRAY_EVENT, active\)/,
  );
  assert.strictEqual(traySource.includes('TOGGLE_MAPPING_MENU_ID'), true);
  assert.strictEqual(traySource.includes('切换键盘映射'), true);
  assert.strictEqual(traySource.includes('toggle_windows_keyboard_mapping'), true);
  assert.strictEqual(traySource.includes('tray.set_icon(Some(tray_image(active)?))'), true);
  assert.strictEqual(traySource.includes('dim_icon(icon)'), true);
  assert.strictEqual(traySource.includes('OPEN_WINDOW_MENU_ID'), true);
  assert.strictEqual(traySource.includes('打开 Yorling'), true);
  assert.strictEqual(traySource.includes('present_main_window(app, crate::MainWindowLifecycle::Ready)'), true);
  assert.strictEqual(traySource.includes('QUIT_MENU_ID'), true);
  assert.strictEqual(traySource.includes('app.exit(0)'), true);
  assert.strictEqual(libSource.includes('window.label() == MAIN_WINDOW_LABEL'), true);

  assert.strictEqual(keyboardCommandSource.includes('windows_keyboard_mapping_active'), true);
  assert.strictEqual(keyboardCommandSource.includes('crate::windows_tray::refresh(&app'), true);
  assert.strictEqual(hookSource.includes("listen<boolean>('keyboard-tray-status-changed'"), true);
});

test('cleans up Windows keyboard mapping from the shared app exit path', () => {
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const traySource = readFileSync('src-tauri/src/windows_tray.rs', 'utf8');

  assert.strictEqual(libSource.includes('fn cleanup_platform_runtimes<R: Runtime>'), true);
  assert.match(
    libSource,
    /#\[cfg\(target_os = "windows"\)\][\s\S]*state::<Arc<InterceptorState>>\(\)[\s\S]*shutdown_windows_keyboard_mapping\(\);/,
  );
  assert.match(
    libSource,
    /tauri::RunEvent::ExitRequested \{ \.\. \} \| tauri::RunEvent::Exit => \{\s*cleanup_platform_runtimes\(app_handle\);/,
  );
  assert.strictEqual(traySource.includes('state_for_quit.shutdown_windows_keyboard_mapping();'), true);
});
