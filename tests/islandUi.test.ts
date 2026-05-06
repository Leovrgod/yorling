import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('styles the collapsed island with notch-like proportions and a sculpted shape', () => {
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');
  const notchShapeSource = readFileSync('src/island/NotchShape.tsx', 'utf8');
  const islandStateSource = readFileSync('src/island/hooks/useIslandState.ts', 'utf8');

  assert.ok(islandStyles.includes('266px'));
  assert.ok(islandStyles.includes('32px'));
  assert.ok(islandStyles.includes('padding-top: 0;'));
  assert.ok(notchShapeSource.includes('generateNotchPath'));
  assert.ok(notchShapeSource.includes("path('"));
  assert.ok(islandStateSource.includes("get_island_screen_info"));
});

test('keeps island host sizing and hover-open panels sticky only while hovered', () => {
  const islandAnimationSource = readFileSync('src/island/hooks/useIslandAnimation.ts', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');
  const islandContentSource = readFileSync('src/island/IslandContent.tsx', 'utf8');
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');
  const sessionCardSource = readFileSync('src/island/components/SessionCard.tsx', 'utf8');
  const islandPresentationSource = readFileSync('src/island/presentation.ts', 'utf8');

  assert.ok(islandAnimationSource.includes('const HOVER_OPEN_DELAY = 240'));
  assert.ok(
    islandAnimationSource.includes("if (viewMode !== 'expanded' || openReason !== 'hover') {") ||
      islandAnimationSource.includes("if (viewMode !== 'expanded' || openReason !== 'hover') return;"),
  );
  assert.ok(islandContentSource.includes('ResizeObserver'));
  assert.ok(islandContentSource.includes('scrollHeight'));
  assert.ok(islandContentSource.includes('measureNestedScrollablePanelHeight'));
  assert.ok(islandContentSource.includes('usesBoundedNativeWindowRef'));
  assert.ok(!islandContentSource.includes('getBoundingClientRect().height'));
  assert.ok(islandRustSource.includes('const ISLAND_WINDOW_HEIGHT: f64 = 750.0;'));
  assert.ok(islandRustSource.includes('const FALLBACK_CLOSED_WIDTH: f64 = 266.0;'));
  assert.ok(islandRustSource.includes('const FALLBACK_CLOSED_HEIGHT: f64 = 32.0;'));
  assert.ok(islandRustSource.includes('window.set_size'));
  assert.ok(islandRustSource.includes('notch.width.max(FALLBACK_CLOSED_WIDTH)'));
  assert.ok(islandStyles.includes('border-radius: 22px;'));
  assert.ok(sessionCardSource.includes('island-session__title-line'));
  assert.ok(sessionCardSource.includes('getSessionProjectLabel'));
  assert.ok(islandPresentationSource.includes('formatCwdSegments'));
  assert.ok(!sessionCardSource.includes('HIDDEN_PROJECT_LABELS'));
});

test('gives the island overlay transparent window styling and IPC capability access', () => {
  const islandWindowSource = readFileSync('src/island/IslandWindow.tsx', 'utf8');
  const resetSource = readFileSync('src/styles/nothing/reset.css', 'utf8');
  const capabilitySource = readFileSync('src-tauri/capabilities/default.json', 'utf8');

  assert.ok(islandWindowSource.includes("document.body.classList.add('island-overlay-window');"));
  assert.ok(resetSource.includes('body.island-overlay-window'));
  assert.ok(capabilitySource.includes('"island"'));
});

test('keeps island sessions in sync and adds hover/outside-click interaction recovery paths', () => {
  const islandStateSource = readFileSync('src/island/hooks/useIslandState.ts', 'utf8');
  const islandContentSource = readFileSync('src/island/IslandContent.tsx', 'utf8');
  const islandAnimationSource = readFileSync('src/island/hooks/useIslandAnimation.ts', 'utf8');
  const completionToastSource = readFileSync('src/island/components/CompletionToast.tsx', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');
  const islandWindowSource = readFileSync('src/island/IslandWindow.tsx', 'utf8');

  assert.ok(islandStateSource.includes("'island-session-sync'"));
  assert.ok(islandStateSource.includes('15000'));
  assert.ok(islandContentSource.includes('previousAttentionCount'));
  assert.ok(islandContentSource.includes('filterCollapsedIslandSessions'));
  assert.ok(islandContentSource.includes('CollapsedBar sessions={collapsedSessions}'));
  assert.ok(islandContentSource.includes('runExpandedViewTransition'));
  assert.ok(islandContentSource.includes('contentTransitionDirection'));
  assert.ok(islandContentSource.includes("expand('attention')"));
  assert.ok(islandContentSource.includes('shouldAutoCollapseExpandedIsland'));
  assert.ok(!islandContentSource.includes("invoke<boolean>('is_terminal_frontmost'"));
  assert.ok(islandContentSource.includes("set_island_interaction_bounds"));
  assert.ok(islandAnimationSource.includes('HOVER_OPEN_DELAY'));
  assert.ok(islandAnimationSource.includes("listen('island-outside-click'"));
  assert.ok(completionToastSource.includes('onOpenSession'));
  assert.ok(completionToastSource.includes('Review'));
  assert.ok(islandWindowSource.includes('filterVisibleIslandSessions'));
  assert.ok(islandRustSource.includes('accept_first_mouse(true)'));
  assert.ok(islandRustSource.includes('addGlobalMonitorForEventsMatchingMask_handler'));
  assert.ok(islandRustSource.includes('hitTest:'));
});

test('keeps attention requests visible even while fullscreen edge-hide mode is active', () => {
  const islandWindowSource = readFileSync('src/island/IslandWindow.tsx', 'utf8');
  const sessionQueueSource = readFileSync('src/island/sessionQueue.ts', 'utf8');

  assert.ok(islandWindowSource.includes('activeSessions.some(isAttentionSession)'));
  assert.ok(islandWindowSource.includes("hasAttentionSession || mode === 'visible'"));
  assert.ok(sessionQueueSource.includes('export function isAttentionSession'));
});

test('routes island sounds through the shared voice asset and event stream', () => {
  const islandStateSource = readFileSync('src/island/hooks/useIslandState.ts', 'utf8');
  const soundSource = readFileSync('src/island/sound.ts', 'utf8');
  const completionToastSource = readFileSync('src/island/components/CompletionToast.tsx', 'utf8');
  const storeSource = readFileSync('src/island/store/islandStore.ts', 'utf8');

  assert.ok(soundSource.includes("import voiceUrl from '../assets/sounds/voice.wav';"));
  assert.ok(soundSource.includes("case 'SessionStart':"));
  assert.ok(soundSource.includes("case 'PermissionRequest':"));
  assert.ok(soundSource.includes("case 'Notification':"));
  assert.ok(soundSource.includes('const MIN_SOUND_INTERVAL_MS = 320;'));
  assert.ok(islandStateSource.includes('preloadIslandSound(soundPrefs.style);'));
  assert.ok(islandStateSource.includes('playIslandEventSound(payload.event_type, soundPrefs.style, soundPrefs.volume);'));
  assert.ok(!completionToastSource.includes('AudioContext'));
  assert.ok(storeSource.includes("style: 'voice'"));
});

test('plays the shared voice sound when enabling the island from settings', () => {
  const islandSettingsSource = readFileSync('src/components/island/IslandPlaceholder.tsx', 'utf8');

  assert.ok(islandSettingsSource.includes("import { playIslandSound, preloadIslandSound } from '../../island/sound';"));
  assert.ok(islandSettingsSource.includes("const soundPrefs = useIslandStore((state) => state.soundPrefs);"));
  assert.ok(islandSettingsSource.includes('preloadIslandSound(soundPrefs.style);'));
  assert.ok(islandSettingsSource.includes("if (newValue && soundPrefs.enabled) {"));
  assert.ok(islandSettingsSource.includes('playIslandSound(soundPrefs.style, soundPrefs.volume);'));
});

test('ships the island as a dedicated frontend entry and bundles the bridge sidecar', () => {
  const islandHtml = readFileSync('island.html', 'utf8');
  const islandEntry = readFileSync('src/island-main.tsx', 'utf8');
  const appSource = readFileSync('src/App.tsx', 'utf8');
  const viteSource = readFileSync('vite.config.ts', 'utf8');
  const tauriConfig = readFileSync('src-tauri/tauri.conf.json', 'utf8');
  const buildSource = readFileSync('src-tauri/build.rs', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandHtml.includes('/src/island-main.tsx'));
  assert.ok(islandEntry.includes('IslandWindow'));
  assert.ok(!appSource.includes("label === 'island'"));
  assert.ok(viteSource.includes("island: resolve(__dirname, \"island.html\")"));
  assert.ok(tauriConfig.includes('"externalBin": ["binaries/yorling-bridge"]'));
  assert.ok(buildSource.includes('build_bridge_sidecar'));
  assert.ok(islandRustSource.includes('bridge_sidecar_name'));
  assert.ok(islandRustSource.includes('"island.html"'));
});

test('renders humanized activity summaries and plugin footer slots inside session cards', () => {
  const sessionCardSource = readFileSync('src/island/components/SessionCard.tsx', 'utf8');
  const presentationSource = readFileSync('src/island/presentation.ts', 'utf8');
  const storeSource = readFileSync('src/island/store/islandStore.ts', 'utf8');

  assert.ok(sessionCardSource.includes('getSessionActivity'));
  assert.ok(sessionCardSource.includes('island-session__activity'));
  assert.ok(sessionCardSource.includes("slot.slot === 'session_footer'"));
  assert.ok(sessionCardSource.includes('HIDDEN_BUILTIN_SESSION_FOOTER_SLOT_IDS'));
  assert.ok(sessionCardSource.includes('core.transcript-preview.status'));
  assert.ok(sessionCardSource.includes('core.tool-history.latest'));
  assert.ok(presentationSource.includes('HUMANIZED_ACTIVITY_LIBRARY'));
  assert.ok(presentationSource.includes('getLatestActiveToolCandidate'));
  assert.ok(storeSource.includes('plugins: IslandPluginInfo[]'));
});

test('surfaces provider diagnostics while only showing plugin details when useful', () => {
  const settingsSource = readFileSync('src/components/island/IslandPlaceholder.tsx', 'utf8');
  const islandStateSource = readFileSync('src/island/hooks/useIslandState.ts', 'utf8');
  const typeSource = readFileSync('src/types/index.ts', 'utf8');
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');

  assert.ok(settingsSource.includes("invoke<IslandPluginInfo[]>('get_island_plugins')"));
  assert.ok(settingsSource.includes('provider.health_report.issues'));
  assert.ok(settingsSource.includes('hookFeedback'));
  assert.ok(settingsSource.includes('Hook 已写入配置'));
  assert.ok(islandStyles.includes('.island-provider-card__feedback--success'));
  assert.ok(settingsSource.includes("plugins.filter((plugin) => plugin.source === 'manifest')"));
  assert.ok(settingsSource.includes('island-settings__hero-side'));
  assert.ok(!settingsSource.includes('copy.island.enableToggle'));
  assert.ok(!settingsSource.includes('copy.island.settingsSubtitle'));
  assert.ok(!settingsSource.includes('copy.island.subtitle'));
  assert.ok(!settingsSource.includes('Plugin Platform'));
  assert.ok(settingsSource.includes("return '~/.copilot/hooks/yorling-island.json';"));
  assert.ok(!settingsSource.includes("return '~/.copilot/config.json';"));
  assert.ok(islandStateSource.includes('fetchPlugins'));
  assert.ok(typeSource.includes('export interface IslandHookHealthReport'));
  assert.ok(islandStyles.includes('.island-settings__hero-side'));
});

test('routes terminal jump through Ghostty, Warp, and Terminal-specific scripts', () => {
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandRustSource.includes('ghostty_jump_script'));
  assert.ok(islandRustSource.includes('warp_jump_script'));
  assert.ok(islandRustSource.includes('terminal_app_jump_script'));
  assert.ok(islandRustSource.includes('normalize_terminal_name'));
});

test('keeps boot animation and collapsed interaction bound to the native island surface', () => {
  const islandAnimationSource = readFileSync('src/island/hooks/useIslandAnimation.ts', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandAnimationSource.includes("setOpenReason('boot')"));
  assert.ok(islandRustSource.includes('IslandPassThroughView'));
  assert.ok(islandRustSource.includes('set_island_interaction_bounds'));
  assert.ok(islandRustSource.includes('set_interaction_size'));
});

test('uses directional panel transitions and adaptive chat body sizing inside the expanded island', () => {
  const islandContentSource = readFileSync('src/island/IslandContent.tsx', 'utf8');
  const islandStyles = readFileSync('src/styles/nothing/island.css', 'utf8');
  const geometrySource = readFileSync('src/island/notchGeometry.ts', 'utf8');

  assert.ok(islandContentSource.includes("direction: 'forward'"));
  assert.ok(islandContentSource.includes("direction: 'back'"));
  assert.ok(islandContentSource.includes('transitioningFrom'));
  assert.ok(islandStyles.includes('.island-content-surface--forward-enter'));
  assert.ok(islandStyles.includes('.island-content-surface--back-exit'));
  assert.ok(islandStyles.includes('.island-chatview'));
  assert.ok(islandStyles.includes('max-height: none;'));
  assert.ok(geometrySource.includes('DEFAULT_CHATVIEW_HEIGHT'));
  assert.ok(geometrySource.includes('ISLAND_EXPANDED_WIDE_MAX_WIDTH = 680'));
});

test('uses native hover and passthrough sync so the collapsed island does not depend on DOM focus', () => {
  const islandWindowSource = readFileSync('src/island/IslandWindow.tsx', 'utf8');
  const islandAnimationSource = readFileSync('src/island/hooks/useIslandAnimation.ts', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandAnimationSource.includes("listen('island-hover-trigger-enter'"));
  assert.ok(islandAnimationSource.includes("listen('island-hover-trigger-leave'"));
  assert.ok(islandAnimationSource.includes("listen('island-collapsed-click'"));
  assert.ok(islandWindowSource.includes("set_island_mouse_passthrough"));
  assert.ok(!islandWindowSource.includes("session.phase !== 'ended'"));
  assert.ok(islandWindowSource.includes("!isVisible || viewMode === 'collapsed'"));
  assert.ok(islandRustSource.includes('NSEventMask::MouseMoved'));
  assert.ok(islandRustSource.includes('NSEventMask::LeftMouseDragged'));
  assert.ok(islandRustSource.includes('ISLAND_HOVER_LOCAL_MONITOR'));
  assert.ok(islandRustSource.includes('emit_island_hover_trigger'));
  assert.ok(islandRustSource.includes('Failed to install island local hover monitor'));
  assert.ok(islandRustSource.includes('ISLAND_COLLAPSED_CLICK_LOCAL_MONITOR'));
  assert.ok(islandRustSource.includes('emit_island_collapsed_click'));
  assert.ok(islandRustSource.includes('Failed to install island local collapsed-click monitor'));
  assert.ok(islandRustSource.includes('ISLAND_OUTSIDE_CLICK_LOCAL_MONITOR'));
  assert.ok(islandRustSource.includes('Failed to install island local outside-click monitor'));
  assert.ok(islandRustSource.includes('set_passthrough'));
  assert.ok(islandRustSource.includes('setIgnoresMouseEvents(passthrough)'));
  assert.ok(islandRustSource.includes('"island-hover-trigger-enter"'));
  assert.ok(islandRustSource.includes('"island-hover-trigger-leave"'));
  assert.ok(islandRustSource.includes('"island-collapsed-click"'));
});

test('replays expanded transparent-area clicks so the large island window does not swallow underlying app input', () => {
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');

  assert.ok(islandRustSource.includes('addLocalMonitorForEventsMatchingMask_handler'));
  assert.ok(islandRustSource.includes('CGEventCreateMouseEvent'));
  assert.ok(islandRustSource.includes('CGEventPost'));
  assert.ok(islandRustSource.includes('windowNumber()'));
  assert.ok(islandRustSource.includes('locationInWindow()'));
  assert.ok(islandRustSource.includes('convertPointToScreen'));
  assert.ok(islandRustSource.includes('return std::ptr::null_mut()'));
});

test('registers more built-in agent providers and bootstraps detected hooks on startup', () => {
  const providerTraitSource = readFileSync('crates/yorling-island-core/src/provider.rs', 'utf8');
  const providerRegistrySource = readFileSync('crates/yorling-island-core/src/providers/mod.rs', 'utf8');
  const islandRustSource = readFileSync('src-tauri/src/island.rs', 'utf8');
  const islandI18nSource = readFileSync('src/island/i18n.ts', 'utf8');
  const tauriLibSource = readFileSync('src-tauri/src/lib.rs', 'utf8');

  assert.ok(providerTraitSource.includes('fn detection_paths(&self)'));
  assert.ok(providerTraitSource.includes('fn detection_commands(&self)'));
  assert.ok(providerTraitSource.includes('fn is_available_on_system(&self)'));
  assert.ok(providerRegistrySource.includes('QWEN_CODE_SPEC'));
  assert.ok(providerRegistrySource.includes('QODER_SPEC'));
  assert.ok(providerRegistrySource.includes('QODERWORK_SPEC'));
  assert.ok(providerRegistrySource.includes('CODEBUDDY_SPEC'));
  assert.ok(providerRegistrySource.includes('WORKBUDDY_SPEC'));
  assert.ok(providerRegistrySource.includes('HermesProvider::new()'));
  assert.ok(providerRegistrySource.includes('OpenClawProvider::new()'));
  assert.ok(providerRegistrySource.includes('OpenCodeProvider::new()'));
  assert.ok(islandRustSource.includes('provider_hooks_bootstrapped'));
  assert.ok(islandRustSource.includes('install_detected_provider_hooks'));
  assert.ok(islandRustSource.includes("Skipping automatic hook install for"));
  assert.ok(tauriLibSource.includes('bootstrap_provider_hooks_if_needed'));
  assert.ok(islandI18nSource.includes("'provider.hermes'"));
  assert.ok(islandI18nSource.includes("'provider.openclaw'"));
  assert.ok(islandI18nSource.includes("'provider.opencode'"));
  assert.ok(islandI18nSource.includes("'provider.qwen-code'"));
  assert.ok(islandI18nSource.includes("'provider.qoder'"));
  assert.ok(islandI18nSource.includes("'provider.qoderwork'"));
  assert.ok(islandI18nSource.includes("'provider.codebuddy'"));
  assert.ok(islandI18nSource.includes("'provider.workbuddy'"));
});

test('restores transcript-backed sessions while keeping discovery lightweight', () => {
  const bridgeServerSource = readFileSync('crates/yorling-island-core/src/bridge_server.rs', 'utf8');
  const transcriptSource = readFileSync('crates/yorling-island-core/src/transcript.rs', 'utf8');

  assert.ok(bridgeServerSource.includes('latest_preview_activity_timestamp(&preview)'));
  assert.ok(bridgeServerSource.includes('discover_recent_openclaw_transcripts'));
  assert.ok(bridgeServerSource.includes('"openclaw" => infer_openclaw_transcript_path'));
  assert.ok(bridgeServerSource.includes('session_file_path'));
  assert.ok(bridgeServerSource.includes('TranscriptDiscoveryCache'));
  assert.ok(bridgeServerSource.includes('TRANSCRIPT_DISCOVERY_REFRESH_INTERVAL'));
  assert.ok(bridgeServerSource.includes('candidates.push((file, last_modified))'));
  assert.ok(bridgeServerSource.includes('let last_activity = transcript_activity_timestamp(&file)?;'));
  assert.ok(!bridgeServerSource.includes('transcript_activity_timestamp(&file).unwrap_or(last_modified)'));
  assert.ok(transcriptSource.includes('pub fn latest_preview_activity_timestamp'));
});
