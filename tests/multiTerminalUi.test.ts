import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('registers the multi-terminal module across the app shell', () => {
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const typesSource = readFileSync('src/types/index.ts', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');
  const styleSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(typesSource.includes("'terminals'"), true);
  assert.strictEqual(copySource.includes("terminals: '多终端'"), true);
  assert.strictEqual(copySource.includes("terminals: 'Terminals'"), true);
  assert.strictEqual(appSource.includes("import('./components/terminals/MultiTerminal')"), true);
  assert.strictEqual(appSource.includes("import { useEmbeddedTerminalEvents }"), true);
  assert.strictEqual(appSource.includes('useEmbeddedTerminalEvents();'), true);
  assert.strictEqual(appSource.includes('terminalModuleMounted'), true);
  assert.strictEqual(appSource.includes('renderTransientModule'), true);
  assert.strictEqual(appSource.includes('app-module-persistent'), true);
  assert.strictEqual(appSource.includes('app-module-hidden'), true);
  assert.strictEqual(styleSource.includes('.app-module-stack'), true);
  assert.strictEqual(styleSource.includes('.app-module-hidden'), true);
  assert.strictEqual(appSource.includes("case 'terminals':"), true);
});

test('keeps terminal projects and multi-agent choice persisted per path', () => {
  const storeSource = readFileSync('src/stores/terminalStore.ts', 'utf8');
  const catalogSource = readFileSync('src/components/terminals/agentCatalog.ts', 'utf8');

  assert.strictEqual(storeSource.includes("yorling.multiTerminal.projects"), true);
  assert.strictEqual(storeSource.includes('addProjectAgent'), true);
  assert.strictEqual(storeSource.includes('selectedAgentId'), true);
  assert.strictEqual(storeSource.includes('selectedTerminalId'), true);
  assert.strictEqual(storeSource.includes('updateProjectAgent'), true);
  assert.strictEqual(storeSource.includes('lastLaunchedAt'), true);
  assert.strictEqual(storeSource.includes('embeddedSessions'), true);
  assert.strictEqual(storeSource.includes('activeEmbeddedTerminalId'), true);
  assert.strictEqual(storeSource.includes('pendingEmbeddedTerminalEvents'), true);
  assert.strictEqual(storeSource.includes('appendEmbeddedTerminalOutput'), true);
  assert.strictEqual(storeSource.includes('markEmbeddedTerminalExited'), true);
  assert.strictEqual(catalogSource.includes("id: 'codex'"), true);
  assert.strictEqual(catalogSource.includes("id: 'claude-code'"), true);
  assert.strictEqual(catalogSource.includes("DEFAULT_TERMINAL_AGENT_ID"), true);
});

test('renders an expandable multi-terminal project group in the sidebar', () => {
  const sidebarSource = readFileSync('src/components/common/Sidebar.tsx', 'utf8');
  const styleSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(sidebarSource.includes("{ id: 'terminals', icon: '▣' }"), true);
  assert.strictEqual(sidebarSource.includes('sidebar-item-disclosure'), true);
  assert.strictEqual(sidebarSource.includes('sidebar-terminal-projects'), true);
  assert.strictEqual(sidebarSource.includes('pickProjectDirectory'), true);
  assert.strictEqual(sidebarSource.includes('directoryPickerTitle'), true);
  assert.strictEqual(sidebarSource.includes('selectTerminalProject(null)'), true);
  assert.strictEqual(sidebarSource.includes('copy.terminals.addProject'), true);
  assert.strictEqual(styleSource.includes('.sidebar-terminal-project-tooltip'), true);
  assert.strictEqual(styleSource.includes('.sidebar-terminal-projects'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-page'), true);
});

test('renders project-scoped open agents and uses island session sync', () => {
  const multiTerminalSource = readFileSync('src/components/terminals/MultiTerminal.tsx', 'utf8');
  const copySource = readFileSync('src/i18n/copy.ts', 'utf8');
  const styleSource = readFileSync('src/styles/nothing/components.css', 'utf8');

  assert.strictEqual(multiTerminalSource.includes("invoke<IslandSession[]>('get_island_sessions')"), true);
  assert.strictEqual(multiTerminalSource.includes("'island-session-sync'"), true);
  assert.strictEqual(multiTerminalSource.includes('copy.openAgentsTitle'), true);
  assert.strictEqual(multiTerminalSource.includes('doesSessionExactlyMatchProject'), true);
  assert.strictEqual(multiTerminalSource.includes('startsWith(`${projectPath}/`)'), false);
  assert.strictEqual(multiTerminalSource.includes('multi-terminal-panel-projects'), false);
  assert.strictEqual(multiTerminalSource.includes('multi-terminal-agent-select'), true);
  assert.strictEqual(multiTerminalSource.includes('multi-terminal-agent-add-button'), true);
  assert.strictEqual(multiTerminalSource.includes("invoke<TerminalInventory>('get_terminal_inventory')"), true);
  assert.strictEqual(multiTerminalSource.includes("'launch_embedded_agent_terminal'"), true);
  assert.strictEqual(multiTerminalSource.includes("'send_embedded_terminal_input'"), true);
  assert.strictEqual(multiTerminalSource.includes("'resize_embedded_terminal'"), true);
  assert.strictEqual(multiTerminalSource.includes('EmbeddedTerminalPane'), true);
  assert.strictEqual(multiTerminalSource.includes('visibleEmbeddedSessions'), true);
  assert.strictEqual(multiTerminalSource.includes('useState<EmbeddedTerminalSession[]>([])'), false);
  assert.strictEqual(multiTerminalSource.includes("listen<EmbeddedTerminalDataEvent>('embedded-terminal-data'"), false);
  assert.strictEqual(multiTerminalSource.includes("listen<EmbeddedTerminalExitEvent>('embedded-terminal-exit'"), false);
  assert.strictEqual(multiTerminalSource.includes('@xterm/xterm'), true);
  assert.strictEqual(multiTerminalSource.includes('@xterm/addon-fit'), true);
  assert.strictEqual(multiTerminalSource.includes('multi-terminal-panel-terminal'), true);
  assert.strictEqual(multiTerminalSource.includes('ProjectPromptHistoryPanel'), true);
  assert.strictEqual(multiTerminalSource.includes('handleRequestRemoveProject'), true);
  assert.strictEqual(multiTerminalSource.includes('handleConfirmRemoveProject'), true);
  assert.strictEqual(multiTerminalSource.includes('pendingRemovalProjectId'), true);
  assert.strictEqual(multiTerminalSource.includes('promptHistories'), true);
  assert.strictEqual(copySource.includes('openAgentsTitle'), true);
  assert.strictEqual(copySource.includes('promptHistoryTitle'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-agent-launcher'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-session-list'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-xterm'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-terminal-tabs'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-project-delete'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-project-remove-confirm'), true);
  assert.strictEqual(styleSource.includes('scrollbar-gutter: stable'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-prompt-toggle'), true);
  assert.strictEqual(styleSource.includes('.multi-terminal-prompt-panel'), true);
});

test('uses the Tauri dialog plugin for native folder picking and keeps terminal launch wiring', () => {
  const packageSource = readFileSync('package.json', 'utf8');
  const libSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const commandsSource = readFileSync('src-tauri/src/commands/mod.rs', 'utf8');
  const terminalCommandSource = readFileSync('src-tauri/src/commands/terminal.rs', 'utf8');
  const cargoSource = readFileSync('src-tauri/Cargo.toml', 'utf8');
  const infoPlistSource = readFileSync('src-tauri/Info.plist', 'utf8');
  const capabilitySource = readFileSync('src-tauri/capabilities/default.json', 'utf8');

  assert.strictEqual(packageSource.includes('@tauri-apps/plugin-dialog'), true);
  assert.strictEqual(commandsSource.includes('pub mod terminal;'), true);
  assert.strictEqual(libSource.includes('tauri_plugin_dialog::init()'), true);
  assert.strictEqual(libSource.includes('commands::terminal::launch_agent_terminal'), true);
  assert.strictEqual(libSource.includes('commands::terminal::get_terminal_inventory'), true);
  assert.strictEqual(libSource.includes('commands::terminal::launch_embedded_agent_terminal'), true);
  assert.strictEqual(libSource.includes('commands::terminal::resize_embedded_terminal'), true);
  assert.strictEqual(cargoSource.includes('tauri-plugin-dialog = "2"'), true);
  assert.strictEqual(cargoSource.includes('libc = "0.2"'), true);
  assert.strictEqual(infoPlistSource.includes('CFBundleAllowMixedLocalizations'), true);
  assert.strictEqual(infoPlistSource.includes('CFBundleLocalizations'), true);
  assert.strictEqual(capabilitySource.includes('dialog:allow-open'), true);
  assert.strictEqual(terminalCommandSource.includes('pub fn launch_agent_terminal'), true);
  assert.strictEqual(terminalCommandSource.includes('pub fn get_terminal_inventory'), true);
  assert.strictEqual(terminalCommandSource.includes('pub fn launch_embedded_agent_terminal'), true);
  assert.strictEqual(terminalCommandSource.includes('pub fn resize_embedded_terminal'), true);
  assert.strictEqual(terminalCommandSource.includes('forkpty'), true);
  assert.strictEqual(terminalCommandSource.includes('provider_id: "codex"'), true);
  assert.strictEqual(terminalCommandSource.includes('provider_id: "qwen-code"'), true);
  assert.strictEqual(terminalCommandSource.includes('Ghostty'), true);
  assert.strictEqual(terminalCommandSource.includes('ghostty_open_arguments'), true);
  assert.strictEqual(terminalCommandSource.includes('com.mitchellh.ghostty'), true);
  assert.strictEqual(terminalCommandSource.includes('--working-directory={}'), true);
  assert.strictEqual(terminalCommandSource.includes('TerminalInventory'), true);
});
