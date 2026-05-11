import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('keeps the Tauri updater backend wired without showing an in-app update control', () => {
  const cargoSource = readFileSync('src-tauri/Cargo.toml', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const commandsSource = readFileSync('src-tauri/src/commands/mod.rs', 'utf8');
  const updaterSource = readFileSync('src-tauri/src/commands/updater.rs', 'utf8');
  const appSource = readFileSync('src/App.tsx', 'utf8');

  assert.strictEqual(cargoSource.includes('tauri-plugin-updater = "2.10.1"'), true);
  assert.strictEqual(libSource.includes('tauri_plugin_updater::Builder::new().build()'), true);
  assert.strictEqual(libSource.includes('.manage(updater_state)'), true);
  assert.strictEqual(commandsSource.includes('pub mod updater;'), true);
  assert.strictEqual(updaterSource.includes('option_env!("YORLING_UPDATER_ENDPOINT")'), true);
  assert.strictEqual(updaterSource.includes('option_env!("YORLING_UPDATER_PUBKEY")'), true);
  assert.strictEqual(updaterSource.includes('check_app_update'), true);
  assert.strictEqual(updaterSource.includes('install_app_update'), true);
  assert.strictEqual(appSource.includes('<UpdateControl language={language} />'), false);
  assert.strictEqual(appSource.includes("invoke<AppUpdateCheckResult>('check_app_update')"), false);
  assert.strictEqual(appSource.includes("invoke('install_app_update')"), false);
});

test('release script creates updater artifacts and a static manifest when configured', () => {
  const releaseScript = readFileSync('scripts/release-macos.sh', 'utf8');

  assert.strictEqual(releaseScript.includes('createUpdaterArtifacts'), true);
  assert.strictEqual(releaseScript.includes('$HOME/.tauri/yorling-updater.key'), true);
  assert.strictEqual(releaseScript.includes('TAURI_SIGNING_PRIVATE_KEY_PASSWORD'), true);
  assert.strictEqual(releaseScript.includes('Leovrgod/yorling'), true);
  assert.strictEqual(releaseScript.includes('YORLING_UPDATER_PUBKEY'), true);
  assert.strictEqual(releaseScript.includes('YORLING_UPDATER_ENDPOINT'), true);
  assert.strictEqual(releaseScript.includes('YORLING_UPDATER_ARTIFACT_URL'), true);
  assert.strictEqual(releaseScript.includes('$APP_NAME.app.tar.gz'), true);
  assert.strictEqual(releaseScript.includes('latest.json'), true);
});

test('release version stays in sync across package metadata', () => {
  const packageJson = JSON.parse(readFileSync('package.json', 'utf8'));
  const tauriConfig = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'));
  const cargoWorkspace = readFileSync('Cargo.toml', 'utf8');
  const cargoVersion = cargoWorkspace.match(/^version = "([^"]+)"/m)?.[1];

  assert.strictEqual(packageJson.version, '0.1.1');
  assert.strictEqual(tauriConfig.version, packageJson.version);
  assert.strictEqual(cargoVersion, packageJson.version);
});
