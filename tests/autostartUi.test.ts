import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('wires launch-at-login through the top-right app shell control', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');
  const styleSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(appSource.includes("@tauri-apps/plugin-autostart"), true);
  assert.strictEqual(appSource.includes('function StartupToggle'), true);
  assert.strictEqual(appSource.includes('isAutostartEnabled()'), true);
  assert.strictEqual(appSource.includes('await enableAutostart();'), true);
  assert.strictEqual(appSource.includes('await disableAutostart();'), true);
  assert.strictEqual(appSource.includes('<StartupToggle language={language} />'), true);
  assert.strictEqual(copySource.includes("autoLaunchLabel: '开机自启'"), true);
  assert.strictEqual(copySource.includes("autoLaunchLabel: 'Launch at login'"), true);
  assert.strictEqual(styleSource.includes('.startup-toggle'), true);
});

test('registers the Tauri autostart plugin and permissions', () => {
  const cargoSource = readFileSync('src-tauri/Cargo.toml', 'utf8');
  const packageSource = readFileSync('package.json', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const capabilitySource = readFileSync('src-tauri/capabilities/default.json', 'utf8');

  assert.strictEqual(packageSource.includes('@tauri-apps/plugin-autostart'), true);
  assert.strictEqual(cargoSource.includes('tauri-plugin-autostart'), true);
  assert.strictEqual(libSource.includes('tauri_plugin_autostart::init'), true);
  assert.strictEqual(libSource.includes('tauri_plugin_autostart::MacosLauncher::LaunchAgent'), true);
  assert.strictEqual(capabilitySource.includes('autostart:allow-enable'), true);
  assert.strictEqual(capabilitySource.includes('autostart:allow-disable'), true);
  assert.strictEqual(capabilitySource.includes('autostart:allow-is-enabled'), true);
});
