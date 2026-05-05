import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { THEME_OPTIONS, isThemeId } from '../src/theme/themeOptions.ts';

test('defines light and dark theme options with light as default', () => {
  assert.deepEqual(
    THEME_OPTIONS.map((theme) => theme.id),
    ['light', 'dark'],
  );
  assert.strictEqual(isThemeId('light'), true);
  assert.strictEqual(isThemeId('dark'), true);
  assert.strictEqual(isThemeId('graphite'), false);
});

test('keeps draggable regions inside the two-column shell without custom zoom hooks', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');

  assert.strictEqual(appSource.includes('className={`workspace-header'), true);
  assert.strictEqual(sidebarSource.includes('className="sidebar-window-strip" data-tauri-drag-region'), true);
  assert.strictEqual(appSource.includes('onDoubleClick={handleTitlebarDoubleClick}'), false);
  assert.strictEqual((appSource.match(/data-tauri-drag-region/g)?.length ?? 0) >= 1, true);
  assert.strictEqual((sidebarSource.match(/data-tauri-drag-region/g)?.length ?? 0) >= 1, true);
  assert.strictEqual(appSource.includes('startDragging'), false);
  assert.strictEqual(appSource.includes('handleTitlebarDoubleClick'), false);
  assert.strictEqual(appSource.includes('data-no-window-drag'), true);
  assert.strictEqual(appSource.includes('workspace-status-chip'), true);
  assert.strictEqual(sidebarSource.includes('sidebar-window-leading'), true);
  assert.strictEqual(sidebarSource.includes('Current module'), false);
});

test('uses a pure two-column shell instead of separate top and bottom chrome rows', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');

  assert.strictEqual(appSource.includes('className="workspace-pane"'), true);
  assert.strictEqual(appSource.includes('<SidebarDivider />'), true);
  assert.strictEqual(appSource.includes('className="titlebar"'), false);
  assert.strictEqual(appSource.includes('className="statusbar"'), false);
});

test('does not add a custom Command+B sidebar hook to the app shell', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');

  assert.strictEqual(appSource.includes("event.metaKey"), false);
  assert.strictEqual(appSource.includes("event.key.toLowerCase() !== 'b'"), false);
  assert.strictEqual(appSource.includes('setSidebarCollapsed(!sidebarCollapsed);'), false);
});

test('places the sidebar toggle on a resizable divider instead of in the titlebar', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');
  const componentsSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(appSource.includes('<SidebarDivider />'), true);
  assert.strictEqual(sidebarSource.includes('sidebar-divider-toggle'), true);
  assert.strictEqual(sidebarSource.includes('setSidebarWidth'), true);
  assert.strictEqual(sidebarSource.includes('requestAnimationFrame'), true);
  assert.strictEqual(sidebarSource.includes('role="separator"'), true);
  assert.strictEqual(componentsSource.includes('body.sidebar-resizing .sidebar'), true);
  assert.strictEqual(sidebarSource.includes('sidebar-divider-line'), false);
  assert.strictEqual(componentsSource.includes('.sidebar-divider::before'), true);
});

test('does not mark the music module as coming soon in the sidebar', () => {
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');

  assert.strictEqual(sidebarSource.includes("{ id: 'music', icon: '♪', hasBadge: true }"), false);
  assert.strictEqual(sidebarSource.includes("{ id: 'music', icon: '♪' }"), true);
  assert.strictEqual(sidebarSource.includes("{ id: 'island', icon: '◆', hasBadge: true }"), false);
  assert.strictEqual(sidebarSource.includes("{ id: 'island', icon: '◆' }"), true);
});

test('uses integrated mode switches and a single master toggle for other mappings', () => {
  const keyboardSource = readFileSync('src/components/keyboard/KeyboardMapping.tsx', 'utf8');

  assert.strictEqual(keyboardSource.includes('className="layer-switcher-pill-toggle"'), true);
  assert.strictEqual(keyboardSource.includes('other-mappings-master-toggle'), true);
  assert.strictEqual(keyboardSource.includes('setRulesDisabled(otherRuleIds, !nextActive);'), true);
  assert.strictEqual(keyboardSource.includes('rule-toggle'), false);
});

test('extracts explicit macOS fullscreen handling for the green window control', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');

  assert.strictEqual(
    appSource.includes("import { toggleWindowZoom } from './utils/windowControls';"),
    true,
  );
  assert.strictEqual(appSource.includes('toggleWindowZoom(getCurrentWindow())'), true);
  assert.strictEqual(appSource.includes('toggleMaximize'), false);
});

test('clips the transparent shell corners and matches native traffic-light geometry', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const resetSource = readFileSync('src/styles/nothing/reset.css', 'utf8');
  const componentsSource = readFileSync('src/styles/nothing/components.css', 'utf8');
  const variablesSource = readFileSync('src/styles/nothing/variables.css', 'utf8');

  assert.strictEqual(resetSource.includes('border-radius: var(--window-radius);'), true);
  assert.strictEqual(componentsSource.includes('clip-path: inset(0 round var(--window-radius));'), true);
  assert.strictEqual(variablesSource.includes('--sidebar-curve-radius: var(--window-radius);'), true);
  assert.strictEqual(componentsSource.includes('border-top-left-radius: var(--sidebar-curve-radius);'), true);
  assert.strictEqual(componentsSource.includes('border-bottom-left-radius: var(--sidebar-curve-radius);'), true);
  assert.strictEqual(componentsSource.includes('.window-control::before'), true);
  assert.strictEqual(componentsSource.includes('gap: 1px;'), true);
  assert.strictEqual(componentsSource.includes('inline-size: 22px;'), true);
  assert.strictEqual(componentsSource.includes('block-size: 22px;'), true);
  assert.strictEqual(componentsSource.includes('inline-size: 14px;'), true);
  assert.strictEqual(componentsSource.includes('block-size: 14px;'), true);
  assert.strictEqual(componentsSource.includes('.window-control-icon::before'), true);
  assert.strictEqual(appSource.includes('className="window-control-icon" aria-hidden="true" />'), true);
  assert.strictEqual(componentsSource.includes('margin-left: -6px;'), false);
});

test('uses Windows-style top-right rectangular controls on Windows', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const componentsSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(appSource.includes("platform === 'windows' ? null"), true);
  assert.strictEqual(appSource.includes('workspace-header-window-controls'), true);
  assert.strictEqual(appSource.includes("['minimize', 'zoom', 'close']"), true);
  assert.strictEqual(componentsSource.includes('.workspace-header-window-controls'), true);
  assert.strictEqual(componentsSource.includes('.window-controls-windows'), true);
  assert.strictEqual(componentsSource.includes('inline-size: 46px;'), true);
  assert.strictEqual(componentsSource.includes('block-size: 32px;'), true);
  assert.strictEqual(componentsSource.includes('border-radius: 0;'), true);
  assert.strictEqual(componentsSource.includes('background: #c42b1c;'), true);
});

test('keeps language and theme controls in the top-right action cluster instead of the sidebar footer', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');

  assert.strictEqual(appSource.includes('<LanguageSwitcher language={language} setLanguage={setLanguage} />'), true);
  assert.strictEqual(appSource.includes('<ThemeSwitcher theme={theme} setTheme={setTheme} language={language} />'), true);
  assert.strictEqual(appSource.includes('utilityControls={('), true);
  assert.strictEqual(sidebarSource.includes('utilityControls'), false);
  assert.strictEqual(sidebarSource.includes('footerMeta'), false);
  assert.strictEqual(sidebarSource.includes('sidebar-overview'), false);
});

test('surfaces screen recording permission guidance in the top-right controls', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const hookSource = readFileSync('src/hooks/useTauriCommand.ts', 'utf8');
  const typesSource = readFileSync('src/types/index.ts', 'utf8');
  const storeSource = readFileSync('src/stores/keyboardStore.ts', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/components.css', 'utf8');
  const commandSource = readFileSync('src-tauri/src/commands/keyboard.rs', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

  assert.strictEqual(appSource.includes('PermissionStatusToggle'), true);
  assert.strictEqual(appSource.includes('hasScreenRecording={hasScreenRecording}'), true);
  assert.strictEqual(appSource.includes('openScreenRecordingSettings'), true);
  assert.strictEqual(hookSource.includes("'open_screen_recording_settings'"), true);
  assert.strictEqual(typesSource.includes('has_screen_recording: boolean;'), true);
  assert.strictEqual(storeSource.includes('has_screen_recording: false'), true);
  assert.strictEqual(copySource.includes("screenRecording: '屏幕录制'"), true);
  assert.strictEqual(stylesSource.includes('.workspace-permission-toggle'), true);
  assert.strictEqual(commandSource.includes('ScreenCaptureAccess::default().preflight()'), true);
  assert.strictEqual(commandSource.includes('Privacy_ScreenCapture'), true);
  assert.strictEqual(libSource.includes('commands::keyboard::open_screen_recording_settings'), true);
});

test('limits the Windows shell to the keyboard module and hides macOS permission controls when not required', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');
  const platformSource = readFileSync('src/utils/platform.ts', 'utf8');
  const keyboardSource = readFileSync('src/components/keyboard/KeyboardMapping.tsx', 'utf8');
  const commandSource = readFileSync('src-tauri/src/commands/keyboard.rs', 'utf8');

  assert.strictEqual(platformSource.includes("if (platform === 'windows')"), true);
  assert.strictEqual(platformSource.includes("return ['keyboard'];"), true);
  assert.strictEqual(appSource.includes('getEnabledModulesForPlatform(platform)'), true);
  assert.strictEqual(appSource.includes('isModuleEnabledOnPlatform(activeModule, platform)'), true);
  assert.strictEqual(appSource.includes('useEmbeddedTerminalEvents(shouldEnableMacOnlyBackgroundModules)'), true);
  assert.strictEqual(appSource.includes('useSuperRightClickStartupRestore(shouldEnableMacOnlyBackgroundModules)'), true);
  assert.strictEqual(appSource.includes('requiresAccessibility ? ('), true);
  assert.strictEqual(appSource.includes('requiresScreenRecording ? ('), true);
  assert.strictEqual(sidebarSource.includes('visibleModules.map'), true);
  assert.strictEqual(keyboardSource.includes('getKeyboardMappingRules(platform)'), true);
  assert.strictEqual(keyboardSource.includes('windowsUnavailableTitle'), true);
  assert.strictEqual(commandSource.includes('WindowsKeyboardInterceptor'), true);
  assert.strictEqual(commandSource.includes('platform: "windows"'), true);
  assert.strictEqual(commandSource.includes('interception_supported: true'), true);
  assert.strictEqual(commandSource.includes('WindowsKeyboardInterceptor::is_running_elevated()'), true);
  assert.strictEqual(commandSource.includes('requires_accessibility: false'), true);
  assert.strictEqual(commandSource.includes('requires_screen_recording: false'), true);
});

test('declares real palette selectors for each app theme', () => {
  const variablesSource = readFileSync('src/styles/nothing/variables.css', 'utf8');

  assert.strictEqual(variablesSource.includes('[data-theme="dark"]'), true);
  assert.strictEqual(variablesSource.includes('--color-bg:'), true);
  assert.strictEqual(variablesSource.includes('--color-text:'), true);
});
