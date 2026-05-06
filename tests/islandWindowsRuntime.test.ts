import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { getEnabledModulesForPlatform } from '../src/utils/platform.ts';

test('enables the island module and runtime on Windows', () => {
  const platformSource = readFileSync('src/utils/platform.ts', 'utf8');
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

  assert.deepEqual(getEnabledModulesForPlatform('windows'), ['keyboard', 'music', 'clipboard', 'island']);
  assert.ok(platformSource.includes("return ['keyboard', 'music', 'clipboard', 'island'];"));
  assert.ok(appSource.includes("case 'island':"));
  assert.ok(libSource.includes('cfg!(any(target_os = "macos", target_os = "windows"))'));
  assert.ok(libSource.includes('register_island_shortcuts(app.handle());'));
  assert.ok(libSource.includes('start_windows_island_cursor_monitor(app.handle());'));
});

test('uses Windows named-pipe transport and a bounded top island window', () => {
  const islandSource = readFileSync('src-tauri/src/island.rs', 'utf8');
  const bridgeSource = readFileSync('crates/yorling-island-bridge/src/main.rs', 'utf8');

  assert.ok(islandSource.includes('PathBuf::from(r"\\\\.\\pipe\\yorling-island")'));
  assert.ok(islandSource.includes('const TOP_BAR_ISLAND_MARGIN: f64 = 0.0;'));
  assert.ok(islandSource.includes('configure_windows_island'));
  assert.ok(islandSource.includes('resize_windows_island_window'));
  assert.ok(islandSource.includes('position_windows_island_window'));
  assert.ok(islandSource.includes('start_windows_island_cursor_monitor'));
  assert.ok(islandSource.includes('windows_cursor_inside_window'));
  assert.ok(islandSource.includes('GetCursorPos'));
  assert.ok(islandSource.includes('GetWindowRect'));
  assert.ok(islandSource.includes('window.set_ignore_cursor_events(false)'));
  assert.ok(islandSource.includes('screen_info_from_monitor'));
  assert.ok(bridgeSource.includes('ERROR_PIPE_BUSY'));
  assert.ok(bridgeSource.includes('PIPE_CONNECT_TIMEOUT'));
  assert.ok(bridgeSource.includes('started_at.elapsed() < PIPE_CONNECT_TIMEOUT'));
});

test('writes Windows-aware hook commands for provider configs', () => {
  const providerModSource = readFileSync('crates/yorling-island-core/src/providers/mod.rs', 'utf8');
  const copilotSource = readFileSync('crates/yorling-island-core/src/providers/copilot.rs', 'utf8');
  const codexSource = readFileSync('crates/yorling-island-core/src/providers/codex.rs', 'utf8');

  assert.ok(providerModSource.includes('pub(crate) fn transport_flag()'));
  assert.ok(providerModSource.includes('"--pipe"'));
  assert.ok(providerModSource.includes('#[cfg(windows)]'));
  assert.ok(copilotSource.includes('fn hook_command_field()'));
  assert.ok(copilotSource.includes('"windows"'));
  assert.ok(copilotSource.includes('"bash"'));
  assert.ok(codexSource.includes('#[cfg(windows)]'));
  assert.ok(codexSource.includes('command'));
});

test('materializes an embedded Windows bridge when the sidecar is missing', () => {
  const islandSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandSource.includes('EMBEDDED_WINDOWS_BRIDGE'));
  assert.ok(islandSource.includes('include_bytes!("../binaries/yorling-bridge-x86_64-pc-windows-msvc.exe")'));
  assert.ok(islandSource.includes('materialize_embedded_bridge()?'));
  assert.ok(islandSource.includes('island_support_dir().join("bin")'));
  assert.ok(islandSource.includes('std::fs::write(&bridge_path, EMBEDDED_WINDOWS_BRIDGE)'));
});
