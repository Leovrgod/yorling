export interface EngineStatus {
  running: boolean;
  enabled: boolean;
  event_count: number;
  has_accessibility: boolean;
  has_screen_recording: boolean;
}

export interface AltTabWindowLite {
  id: number;
  owner_pid: number;
  app_name: string;
  title: string;
  width: number;
  height: number;
}

export type AltTabWindow = AltTabWindowLite;

export interface AltTabOverlayTileLayout {
  row: number;
  width: number;
  height: number;
}

export interface AltTabOverlayLayout {
  rows: number;
  panel_width: number;
  panel_height: number;
  tile_gap: number;
  tile_height: number;
  tiles: AltTabOverlayTileLayout[];
}

export interface AltTabOverlayState {
  visible: boolean;
  session_id: number;
  selected_index: number | null;
  hovered_index: number | null;
  windows: AltTabWindowLite[];
  layout: AltTabOverlayLayout;
}

export interface AltTabOverlaySelection {
  visible: boolean;
  session_id: number;
  selected_index: number | null;
  hovered_index: number | null;
}

export interface AltTabOverlayThumbnailUpdate {
  session_id: number;
  window_id: number;
  thumbnail_data_url?: string | null;
}

export interface AltTabAppIconUpdate {
  session_id: number;
  owner_pid: number;
  app_icon_data_url?: string | null;
}

export type BracketOverlayPair = 'round' | 'square' | 'curly';

export interface BracketOverlayState {
  visible: boolean;
  pending_pair: BracketOverlayPair | null;
  anchor_x: number;
  anchor_y: number;
}

export interface MappingRule {
  id: string;
  type: 'simple' | 'tap_hold';
  category: 'navigation' | 'editing' | 'system' | 'number' | 'symbol' | 'shortcut' | 'mouse';
  from: string;
  fromDisplay: string;
  to: string;
  toDisplay: string;
  modifier?: string;
  modifierDisplay?: string;
  active: boolean;
}

export type ModuleName = 'keyboard' | 'music' | 'superRightClick' | 'clipboard' | 'island' | 'terminals';
export type AppThemeId = 'light' | 'dark';
export type AppLanguageId = 'zh' | 'en';

export interface AppState {
  activeModule: ModuleName;
  theme: AppThemeId;
  language: AppLanguageId;
}

// ─── Island types ───

export type IslandSessionPhase =
  | 'idle'
  | 'processing'
  | 'waitingforapproval'
  | 'waitingforanswer'
  | 'compacting'
  | 'ended';

export interface IslandPendingPermission {
  request_id: string;
  tool: string;
  input: Record<string, unknown>;
  received_at?: number;
}

export interface IslandPendingQuestion {
  request_id: string;
  question: string;
  options: string[];
  received_at?: number;
  can_answer?: boolean;
}

export type IslandTranscriptSyncStatus = 'unavailable' | 'synced' | 'degraded';
export type IslandToolHistoryState = 'started' | 'succeeded' | 'failed';

export interface IslandToolHistoryItem {
  id: string;
  tool: string;
  state: IslandToolHistoryState;
  input_preview: string | null;
  output_preview: string | null;
  started_at: number | null;
  finished_at: number | null;
}

export interface IslandTranscriptPreview {
  transcript_path: string;
  provider_session_id: string | null;
  status: IslandTranscriptSyncStatus;
  synced_at: number | null;
  turn_count: number;
  latest_task_started_at: number | null;
  latest_task_finished_at: number | null;
  latest_pending_question: IslandPendingQuestion | null;
  last_error: string | null;
  chat_preview: {
    role: 'user' | 'assistant' | 'system';
    text: string;
    timestamp: number;
  }[];
  tool_history: IslandToolHistoryItem[];
}

export interface IslandChatMessage {
  role: 'user' | 'assistant' | 'system';
  text: string;
  timestamp: number;
}

export interface IslandTerminalContext {
  pid?: number;
  tty?: string;
  cwd?: string;
  terminal_app?: string;
  terminal_bundle_id?: string;
  terminal_session_id?: string;
  pane_title?: string;
  warp_pane_uuid?: string;
}

export interface IslandSession {
  id: string;
  provider_id: string;
  phase: IslandSessionPhase;
  task_title: string | null;
  provider_session_id: string | null;
  chat_messages: IslandChatMessage[];
  tools_in_flight: string[];
  pending_permission: IslandPendingPermission | null;
  pending_question: IslandPendingQuestion | null;
  subagent_count: number;
  started_at: number;
  ended_at: number | null;
  terminal_context: IslandTerminalContext | null;
  transcript_preview: IslandTranscriptPreview | null;
  richer_snapshot: boolean;
}

export type IslandHookHealthSeverity = 'error' | 'warning' | 'info';

export interface IslandHookHealthIssue {
  code: string;
  message: string;
  severity: IslandHookHealthSeverity;
  repairable: boolean;
}

export interface IslandHookHealthReport {
  healthy: boolean;
  issues: IslandHookHealthIssue[];
  config_paths: string[];
  bridge_path?: string | null;
  checked_at: number;
}

export interface IslandProviderInfo {
  id: string;
  display_name: string;
  hook_status: 'installed' | 'not_installed' | 'outdated' | { broken: { reason: string } };
  supports_blocking: boolean;
  origin: 'built_in' | 'external';
  plugin_id: string | null;
  manages_hooks: boolean;
  config_paths: string[];
  health_report: IslandHookHealthReport;
  slot_count: number;
}

export interface IslandPluginSlot {
  id: string;
  slot: string;
  title: string | null;
  template: string;
  provider_ids: string[];
  priority: number;
}

export interface IslandPluginInfo {
  id: string;
  name: string;
  version: string;
  description: string | null;
  source: 'built_in' | 'manifest';
  provider_id: string | null;
  manifest_path: string | null;
  slots: IslandPluginSlot[];
}

export type IslandViewMode = 'collapsed' | 'expanded';
export type IslandPlacementMode = 'notch' | 'top_bar';
export type IslandApprovalPolicy = 'AllowAlways' | 'DenyAlways';

export interface IslandApprovalRule {
  provider_id: string;
  tool_name: string;
  input_signature: string | null;
  policy: IslandApprovalPolicy;
  created_at: number;
}

export interface IslandScreenRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface IslandScreenInfo {
  screen_name: string;
  has_notch: boolean;
  screen_width: number;
  screen_height: number;
  notch_rect: IslandScreenRect | null;
  scale_factor: number;
  is_builtin: boolean;
  placement_mode: IslandPlacementMode;
  top_inset: number;
  closed_width: number;
  closed_height: number;
}

export interface IslandEventPayload {
  session_id: string;
  provider_id: string;
  timestamp: number;
  event_type: { type: string; data?: unknown };
  terminal_context: IslandTerminalContext | null;
}

export interface IslandScreenListItem {
  screen_name: string;
  has_notch: boolean;
  is_builtin: boolean;
  screen_width: number;
  screen_height: number;
}
