import type {
  IslandChatMessage,
  IslandSession,
  IslandSessionPhase,
} from '../types';
import { t } from './i18n.ts';

const UNKNOWN_TOOL_LABELS = new Set(['', 'unknown', 'unknown tool']);
const HUMANIZED_ACTIVITY_MAX_LENGTH = 88;
const KNOWN_PROVIDER_IDS = [
  'claude-code',
  'codex',
  'gemini',
  'cursor',
  'copilot',
  'hermes',
  'openclaw',
  'opencode',
  'qwen-code',
  'qoder',
  'qoderwork',
  'codebuddy',
  'workbuddy',
] as const;

export interface IslandCollapsedTickerItem {
  id: string;
  sessionId: string;
  providerId: string;
  text: string;
  isHumanized: boolean;
  accentToken: IslandProviderAccent;
}

export type IslandProviderAccent =
  | 'claude'
  | 'codex'
  | 'copilot'
  | 'gemini'
  | 'cursor'
  | 'neutral';

export interface IslandSessionActivity {
  text: string;
  isHumanized: boolean;
  accentToken: IslandProviderAccent;
}

type ActivityCategory =
  | 'think'
  | 'inspect'
  | 'scan'
  | 'patch'
  | 'write'
  | 'search'
  | 'fetch'
  | 'delegate'
  | 'compact'
  | 'verify';

interface ToolActivityCandidate {
  tool: string | null | undefined;
  inputPreview: string | null;
  outputPreview: string | null;
  seed: string;
}

const HUMANIZED_ACTIVITY_LIBRARY: Record<IslandProviderAccent, Record<ActivityCategory, string[]>> = {
  claude: {
    think: ['Listening...', 'Untangling...', 'Sketching...'],
    inspect: ['Peeking around...', 'Following threads...', 'Reading the room...'],
    scan: ['Sweeping...', 'Sniffing out clues...', 'Hunting traces...'],
    patch: ['Mending...', 'Re-stitching...', 'Tuning seams...'],
    write: ['Drafting...', 'Composing...', 'Laying tracks...'],
    search: ['Gathering receipts...', 'Cross-checking...', 'Pulling context...'],
    fetch: ['Bringing context in...', 'Collecting notes...', 'Fetching...'],
    delegate: ['Calling in help...', 'Passing a note...', 'Splitting the trail...'],
    compact: ['Folding context...', 'Packing sparks...', 'Evaporating...'],
    verify: ['Double-checking...', 'Pressure-testing...', 'Checking the edges...'],
  },
  codex: {
    think: ['Reasoning...', 'Modeling...', 'Spooling up...'],
    inspect: ['Parsing...', 'Tracing...', 'Stepping through...'],
    scan: ['Indexing...', 'Sifting...', 'Scanning...'],
    patch: ['Patching...', 'Refactoring...', 'Rewiring...'],
    write: ['Synthesizing...', 'Scaffolding...', 'Drafting...'],
    search: ['Querying...', 'Cross-checking...', 'Gathering signals...'],
    fetch: ['Pulling context...', 'Fetching traces...', 'Collecting output...'],
    delegate: ['Sharding...', 'Forking threads...', 'Handing off...'],
    compact: ['Evaporating...', 'Condensing...', 'Compressing...'],
    verify: ['Verifying...', 'Dry-running...', 'Checking edges...'],
  },
  copilot: {
    think: ['Autocomplete humming...', 'Lining things up...', 'Plotting a suggestion...'],
    inspect: ['Hovering around...', 'Skimming...', 'Spotlighting clues...'],
    scan: ['Surfacing...', 'Indexing hints...', 'Sweeping snippets...'],
    patch: ['Filling gaps...', 'Polishing...', 'Stitching fixes...'],
    write: ['Ghost-writing...', 'Completing the shape...', 'Drafting snippets...'],
    search: ['Searching the graph...', 'Cross-linking...', 'Gathering hints...'],
    fetch: ['Syncing context...', 'Pulling snippets...', 'Fetching refs...'],
    delegate: ['Queuing helpers...', 'Branching out...', 'Handing off...'],
    compact: ['Blinking...', 'Evaporating...', 'Shrinking context...'],
    verify: ['Checking suggestions...', 'Running a quick pass...', 'Verifying fit...'],
  },
  gemini: {
    think: ['Sparking...', 'Refracting...', 'Orbiting...'],
    inspect: ['Reading starlight...', 'Prism-checking...', 'Tracing constellations...'],
    scan: ['Sweeping the sky...', 'Scanning horizons...', 'Sampling signals...'],
    patch: ['Reframing...', 'Re-weaving...', 'Aligning facets...'],
    write: ['Drafting light...', 'Shaping a beam...', 'Composing a glint...'],
    search: ['Gathering signals...', 'Cross-pollinating...', 'Following a flare...'],
    fetch: ['Pulling a thread...', 'Fetching a glimmer...', 'Collecting context...'],
    delegate: ['Splitting the prism...', 'Sending a shard...', 'Calling another star...'],
    compact: ['Collapsing starlight...', 'Evaporating...', 'Folding the sky...'],
    verify: ['Checking the spectrum...', 'Validating the arc...', 'Confirming the glow...'],
  },
  cursor: {
    think: ['Gliding...', 'Vectoring...', 'Calibrating...'],
    inspect: ['Tracing facets...', 'Reading edges...', 'Following contours...'],
    scan: ['Sweeping the surface...', 'Indexing edges...', 'Scanning the grain...'],
    patch: ['Carving...', 'Refining edges...', 'Polishing a cut...'],
    write: ['Laying geometry...', 'Drafting a path...', 'Shaping the surface...'],
    search: ['Tracking a line...', 'Gathering bearings...', 'Surveying the field...'],
    fetch: ['Pulling bearings...', 'Fetching a trace...', 'Collecting coordinates...'],
    delegate: ['Passing the cursor...', 'Branching the path...', 'Handing off the cut...'],
    compact: ['Faceting context...', 'Evaporating...', 'Shrinking the cut...'],
    verify: ['Checking alignment...', 'Verifying the cut...', 'Testing the fit...'],
  },
  neutral: {
    think: ['Thinking...', 'Working through it...', 'Settling in...'],
    inspect: ['Inspecting...', 'Reading...', 'Looking around...'],
    scan: ['Scanning...', 'Searching around...', 'Sifting...'],
    patch: ['Updating...', 'Reworking...', 'Patching...'],
    write: ['Writing...', 'Drafting...', 'Composing...'],
    search: ['Searching...', 'Gathering context...', 'Looking it up...'],
    fetch: ['Fetching...', 'Pulling context...', 'Collecting...'],
    delegate: ['Delegating...', 'Splitting work...', 'Calling backup...'],
    compact: ['Compacting...', 'Condensing...', 'Evaporating...'],
    verify: ['Verifying...', 'Checking...', 'Testing...'],
  },
};

export function getPhaseLabel(phase: IslandSessionPhase | string): string {
  const translated = t(`phase.${phase}`);
  return translated === `phase.${phase}` ? phase : translated;
}

export function formatProviderLabel(providerId: string): string {
  const translated = t(`provider.${providerId}`);
  if (translated !== `provider.${providerId}`) {
    return translated;
  }

  return providerId
    .split('-')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

export function formatCwdSegments(
  cwd: string | null | undefined,
  depth = 2,
): string | null {
  const normalized = normalizeCwd(cwd);
  if (!normalized) {
    return null;
  }

  const parts = normalized.split('/').filter(Boolean);
  if (parts.length === 0) {
    return normalized === '~' ? normalized : null;
  }

  return parts.slice(-Math.max(1, depth)).join('/');
}

export function getSessionProjectLabel(session: IslandSession): string | null {
  const projectLabel = formatCwdSegments(session.terminal_context?.cwd, 2);
  if (!projectLabel) {
    return null;
  }

  if (isGenericSessionHeading(projectLabel)) {
    return null;
  }

  const title = getSessionTitle(session).trim();
  if (title && title.toLowerCase() === projectLabel.toLowerCase()) {
    return null;
  }

  return projectLabel;
}

export function isGenericSessionHeading(label: string | null | undefined): boolean {
  const normalizedLabel = normalizeSessionHeading(label);
  if (!normalizedLabel) {
    return false;
  }

  return KNOWN_PROVIDER_IDS.some((providerId) => getProviderHeadingAliases(providerId).has(normalizedLabel));
}

export function formatElapsedDuration(timestamp: number, now = Date.now()): string {
  const delta = Math.max(0, now - timestamp);

  if (delta < 3_600_000) {
    return t('time.minute', { n: Math.max(1, Math.round(delta / 60_000)) });
  }

  if (delta < 86_400_000) {
    return t('time.hour', { n: Math.max(1, Math.round(delta / 3_600_000)) });
  }

  return t('time.day', { n: Math.max(1, Math.round(delta / 86_400_000)) });
}

export function getProviderAccentToken(providerId: string): IslandProviderAccent {
  switch (providerId) {
    case 'claude-code':
      return 'claude';
    case 'codex':
      return 'codex';
    case 'copilot':
      return 'copilot';
    case 'gemini':
      return 'gemini';
    case 'cursor':
      return 'cursor';
    default:
      return 'neutral';
  }
}

export function getSessionSummary(session: IslandSession): string {
  if (session.pending_permission) {
    return t('session.awaiting_approval', {
      tool: formatToolLabel(session.pending_permission.tool, session.pending_permission.input) ?? t('tool.unknown'),
    });
  }

  if (session.pending_question) {
    return session.pending_question.question;
  }

  const chatPreview = getMergedChatMessages(session);
  const toolHistory = session.transcript_preview?.tool_history ?? [];

  const latestAssistant = [...chatPreview]
    .reverse()
    .find((message) => message.role === 'assistant')?.text;
  if (latestAssistant) {
    return latestAssistant;
  }

  const latestUser = [...chatPreview]
    .reverse()
    .find((message) => message.role === 'user')?.text;
  if (latestUser) {
    return latestUser;
  }

  const visibleToolLabels = getVisibleToolLabels(session);
  if (visibleToolLabels.length > 0) {
    return visibleToolLabels[0];
  }

  const latestTool = toolHistory.length > 0 ? toolHistory[toolHistory.length - 1] : null;
  if (latestTool) {
    return latestTool.output_preview
      ?? latestTool.input_preview
      ?? formatToolLabel(latestTool.tool)
      ?? `${latestTool.state}`;
  }

  if (session.subagent_count > 0) {
    return t('session.subagents', { n: session.subagent_count });
  }

  return t('session.waiting');
}

export function getSessionActivity(session: IslandSession): IslandSessionActivity | null {
  const accentToken = getProviderAccentToken(session.provider_id);

  if (session.pending_permission) {
    return {
      text: t('collapsed.approval_needed'),
      isHumanized: false,
      accentToken,
    };
  }

  if (session.pending_question) {
    return {
      text: compactText(
        session.pending_question.question || t('collapsed.question_waiting'),
        HUMANIZED_ACTIVITY_MAX_LENGTH,
      ),
      isHumanized: false,
      accentToken,
    };
  }

  if (session.phase === 'compacting') {
    return buildHumanizedActivity(session, 'compact', `${session.phase}:${session.id}`);
  }

  const activeTool = getLatestActiveToolCandidate(session);
  if (activeTool) {
    const category = categorizeToolActivity(activeTool);
    if (category) {
      return buildHumanizedActivity(session, category, activeTool.seed);
    }

    const fallbackText = getToolFallbackText(activeTool);
    if (fallbackText) {
      return {
        text: fallbackText,
        isHumanized: false,
        accentToken,
      };
    }
  }

  if (session.phase === 'processing') {
    return buildHumanizedActivity(session, 'think', `${session.id}:${getSessionTitle(session)}`);
  }

  return null;
}

export function getSessionListTitle(count: number): string {
  if (count === 0) {
    return t('sessions.no_live');
  }

  return t('sessions.live_count', { n: count, suffix: count === 1 ? '' : 's' });
}

export function getVisiblePromptMessages(session: IslandSession) {
  return getMergedChatMessages(session).filter((message) => message.role === 'user');
}

export function getVisibleAgentMessages(session: IslandSession) {
  return getMergedChatMessages(session).filter((message) => message.role === 'assistant');
}

export function getMergedChatMessages(session: IslandSession): IslandChatMessage[] {
  const merged = [
    ...session.chat_messages,
    ...(session.transcript_preview?.chat_preview ?? []),
  ]
    .filter((message) => message.text.trim().length > 0)
    .sort((left, right) => {
      if (left.timestamp !== right.timestamp) {
        return left.timestamp - right.timestamp;
      }

      return roleRank(left.role) - roleRank(right.role);
    });

  const deduped: IslandChatMessage[] = [];
  for (const message of merged) {
    const last = deduped[deduped.length - 1];
    if (last && last.role === message.role && last.text === message.text) {
      continue;
    }
    deduped.push(message);
  }

  return deduped;
}

export function getSessionTitle(session: IslandSession): string {
  const explicitTitle = session.task_title?.trim();
  if (explicitTitle) {
    return explicitTitle;
  }

  const latestPrompt = getLastItem(getVisiblePromptMessages(session))?.text.trim();
  if (latestPrompt) {
    return compactText(latestPrompt, 90);
  }

  return formatProviderLabel(session.provider_id);
}

export function getLatestTranscriptActivityTimestamp(session: IslandSession): number | null {
  const timestamps = [
    ...getMergedChatMessages(session).map((message) => message.timestamp),
    ...(session.transcript_preview?.tool_history ?? []).flatMap((item) => [
      item.started_at ?? Number.NEGATIVE_INFINITY,
      item.finished_at ?? Number.NEGATIVE_INFINITY,
    ]),
  ].filter((timestamp) => Number.isFinite(timestamp));

  return timestamps.length > 0 ? Math.max(...timestamps) : null;
}

export function getCollapsedTickerItems(sessions: IslandSession[]): IslandCollapsedTickerItem[] {
  if (sessions.length === 0) {
    return [];
  }

  const orderedSessions = [...sessions].sort((left, right) => {
    const leftActivity = getLatestTranscriptActivityTimestamp(left) ?? left.started_at;
    const rightActivity = getLatestTranscriptActivityTimestamp(right) ?? right.started_at;
    return rightActivity - leftActivity;
  });

  const items = orderedSessions.flatMap((session) => buildCollapsedItemsForSession(session));
  const seen = new Set<string>();

  return items.filter((item) => {
    if (seen.has(item.text)) {
      return false;
    }
    seen.add(item.text);
    return true;
  });
}

export function getVisibleToolLabels(session: IslandSession): string[] {
  return session.tools_in_flight
    .map((tool) => formatToolLabel(tool))
    .filter((tool): tool is string => Boolean(tool));
}

export function formatToolLabel(tool: string | null | undefined, input?: unknown): string | null {
  const trimmed = tool?.trim() ?? '';
  const normalized = trimmed.toLowerCase();

  if (UNKNOWN_TOOL_LABELS.has(normalized)) {
    return inferToolLabelFromInput(input);
  }

  switch (normalized) {
    case 'bash':
    case 'shell':
    case 'shellid':
      return 'Bash';
    case 'read':
    case 'read_file':
    case 'view':
    case 'open':
      return 'Read';
    case 'edit':
    case 'file_edit':
    case 'multiedit':
    case 'multi_edit':
    case 'notebookedit':
    case 'notebook_edit':
    case 'str_replace_editor':
    case 'replace':
      return 'Edit';
    case 'write':
    case 'write_file':
      return 'Write';
    case 'glob':
    case 'list_dir':
    case 'list_directory':
      return 'Glob';
    case 'grep':
    case 'search_file_content':
      return 'Grep';
    case 'task':
    case 'spawn_agent':
      return 'Task';
    case 'webfetch':
    case 'web_fetch':
      return 'WebFetch';
    case 'websearch':
    case 'web_search':
      return 'WebSearch';
    case 'exec_command':
    case 'run_shell_command':
      return 'Bash';
    default:
      if (/^[a-z0-9_-]+$/.test(trimmed)) {
        return trimmed
          .split(/[-_]+/)
          .filter(Boolean)
          .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
          .join(' ');
      }
      return trimmed;
  }
}

function inferToolLabelFromInput(input: unknown): string | null {
  if (!input || typeof input !== 'object') {
    return null;
  }

  const record = input as Record<string, unknown>;

  if (typeof record.command === 'string') {
    return 'Bash';
  }

  if (typeof record.pattern === 'string') {
    return 'Grep';
  }

  const path = record.file_path ?? record.path;
  if (typeof path === 'string' && path.trim()) {
    if (typeof record.old_string === 'string' || typeof record.new_string === 'string') {
      return 'Edit';
    }

    if (typeof record.content === 'string') {
      return 'Write';
    }

    return 'Read';
  }

  return null;
}

function buildCollapsedItemsForSession(session: IslandSession): IslandCollapsedTickerItem[] {
  const items: IslandCollapsedTickerItem[] = [];
  const activity = getSessionActivity(session);

  if (activity) {
    items.push({
      id: `${session.id}:activity`,
      sessionId: session.id,
      providerId: session.provider_id,
      text: activity.text,
      isHumanized: activity.isHumanized,
      accentToken: activity.accentToken,
    });
  }

  const latestAssistant = getLastItem(getVisibleAgentMessages(session))?.text;
  if (items.length === 0 && latestAssistant) {
    items.push({
      id: `${session.id}:assistant`,
      sessionId: session.id,
      providerId: session.provider_id,
      text: compactText(latestAssistant, HUMANIZED_ACTIVITY_MAX_LENGTH),
      isHumanized: false,
      accentToken: getProviderAccentToken(session.provider_id),
    });
  }

  const latestPrompt = getLastItem(getVisiblePromptMessages(session))?.text;
  if (items.length === 0 && latestPrompt) {
    items.push({
      id: `${session.id}:prompt`,
      sessionId: session.id,
      providerId: session.provider_id,
      text: compactText(latestPrompt, HUMANIZED_ACTIVITY_MAX_LENGTH),
      isHumanized: false,
      accentToken: getProviderAccentToken(session.provider_id),
    });
  }

  if (items.length === 0) {
    items.push({
      id: `${session.id}:fallback`,
      sessionId: session.id,
      providerId: session.provider_id,
      text: getSessionTitle(session),
      isHumanized: false,
      accentToken: getProviderAccentToken(session.provider_id),
    });
  }

  return items;
}

function buildHumanizedActivity(
  session: IslandSession,
  category: ActivityCategory,
  seed: string,
): IslandSessionActivity {
  const accentToken = getProviderAccentToken(session.provider_id);
  const phrase = pickHumanizedPhrase(accentToken, category, `${session.provider_id}:${seed}`);

  return {
    text: `+ ${phrase}`,
    isHumanized: true,
    accentToken,
  };
}

function getProviderHeadingAliases(providerId: string): Set<string> {
  const spacedProviderId = providerId.replace(/-/g, ' ');
  return new Set([
    normalizeSessionHeading(providerId),
    normalizeSessionHeading(spacedProviderId),
    normalizeSessionHeading(formatProviderLabel(providerId)),
  ].filter((value): value is string => Boolean(value)));
}

function normalizeSessionHeading(label: string | null | undefined): string {
  return label
    ?.trim()
    .replace(/^[\s·•:：-]+|[\s·•:：-]+$/g, '')
    .replace(/\s+/g, ' ')
    .toLowerCase()
    ?? '';
}

function pickHumanizedPhrase(
  accentToken: IslandProviderAccent,
  category: ActivityCategory,
  seed: string,
): string {
  const providerPhrases = HUMANIZED_ACTIVITY_LIBRARY[accentToken] ?? HUMANIZED_ACTIVITY_LIBRARY.neutral;
  const phrases = providerPhrases[category] ?? HUMANIZED_ACTIVITY_LIBRARY.neutral[category];
  const index = stableIndex(seed, phrases.length);
  return phrases[index];
}

function stableIndex(seed: string, size: number): number {
  if (size <= 1) {
    return 0;
  }

  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) {
    hash = ((hash << 5) - hash + seed.charCodeAt(index)) | 0;
  }

  return Math.abs(hash) % size;
}

function getLatestActiveToolCandidate(session: IslandSession): ToolActivityCandidate | null {
  const toolHistory = session.transcript_preview?.tool_history ?? [];
  const activeHistoryItems = toolHistory.filter(
    (item) => item.state === 'started' || item.finished_at == null,
  );
  const latestHistoryItem = getLastItem(activeHistoryItems);

  if (latestHistoryItem) {
    return {
      tool: latestHistoryItem.tool,
      inputPreview: latestHistoryItem.input_preview,
      outputPreview: latestHistoryItem.output_preview,
      seed: `${latestHistoryItem.id}:${latestHistoryItem.tool}:${latestHistoryItem.input_preview ?? ''}`,
    };
  }

  const liveTool = getLastItem(session.tools_in_flight);
  if (liveTool) {
    return {
      tool: liveTool,
      inputPreview: null,
      outputPreview: null,
      seed: `in-flight:${session.id}:${liveTool}`,
    };
  }

  return null;
}

function categorizeToolActivity(candidate: ToolActivityCandidate): ActivityCategory | null {
  const normalizedTool = (formatToolLabel(candidate.tool) ?? candidate.tool ?? '')
    .trim()
    .toLowerCase();
  const command = extractCommandPreview(candidate.inputPreview);
  const commandCategory = command ? categorizeCommand(command) : null;

  if (normalizedTool === 'task') {
    return 'delegate';
  }

  if (normalizedTool === 'websearch') {
    return 'search';
  }

  if (normalizedTool === 'webfetch') {
    return 'fetch';
  }

  if (normalizedTool === 'read') {
    return 'inspect';
  }

  if (normalizedTool === 'grep' || normalizedTool === 'glob') {
    return 'scan';
  }

  if (normalizedTool === 'edit') {
    return 'patch';
  }

  if (normalizedTool === 'write') {
    return 'write';
  }

  if (normalizedTool === 'bash' && commandCategory) {
    return commandCategory;
  }

  return null;
}

function categorizeCommand(command: string): ActivityCategory | null {
  const trimmed = command.trim().replace(/^\$\s*/, '');
  if (!trimmed) {
    return null;
  }

  const tokens = trimmed.split(/\s+/).filter(Boolean);
  if (tokens.length === 0) {
    return null;
  }

  const executable = normalizeExecutableToken(tokens[0]);
  const subcommand = normalizeExecutableToken(tokens[1] ?? '');

  if (['rg', 'grep', 'find', 'fd', 'ls', 'tree', 'dir'].includes(executable)) {
    return 'scan';
  }

  if (['cat', 'sed', 'head', 'tail', 'less', 'more', 'bat'].includes(executable)) {
    return 'inspect';
  }

  if (['curl', 'wget', 'http', 'xh'].includes(executable)) {
    return 'fetch';
  }

  if (['pytest', 'vitest', 'jest', 'playwright', 'tsc', 'eslint', 'ruff', 'biome'].includes(executable)) {
    return 'verify';
  }

  if (['pnpm', 'npm', 'yarn', 'bun', 'cargo', 'go', 'make', 'deno', 'uv', 'swift', 'flutter', 'dart'].includes(executable)) {
    if (['test', 'check', 'lint', 'verify', 'build', 'analyze', 'fmt', 'format'].includes(subcommand)) {
      return 'verify';
    }
  }

  if (executable === 'git') {
    if (['status', 'diff', 'show', 'log', 'blame', 'grep'].includes(subcommand)) {
      return subcommand === 'grep' ? 'scan' : 'inspect';
    }

    if (['apply', 'commit', 'add', 'restore', 'checkout', 'switch', 'merge', 'rebase', 'cherry-pick'].includes(subcommand)) {
      return 'patch';
    }
  }

  if (trimmed.includes('apply_patch') || /\b(sed|perl)\b.*\b-i\b/.test(trimmed)) {
    return 'patch';
  }

  return null;
}

function normalizeExecutableToken(token: string): string {
  return token.replace(/^['"]+|['"]+$/g, '').toLowerCase();
}

function getToolFallbackText(candidate: ToolActivityCandidate): string | null {
  const commandPreview = extractCommandPreview(candidate.inputPreview);
  if (commandPreview) {
    return compactText(commandPreview, HUMANIZED_ACTIVITY_MAX_LENGTH);
  }

  const structuredPreview = extractStructuredPreview(candidate.inputPreview)
    ?? extractStructuredPreview(candidate.outputPreview);
  if (structuredPreview) {
    return compactText(structuredPreview, HUMANIZED_ACTIVITY_MAX_LENGTH);
  }

  return formatToolLabel(candidate.tool) ?? candidate.tool ?? null;
}

function extractCommandPreview(preview: string | null): string | null {
  const parsed = parsePreviewRecord(preview);
  if (parsed) {
    const command = getFirstString(parsed, ['command', 'cmd']);
    if (command) {
      return normalizePreviewText(command);
    }
  }

  if (!preview) {
    return null;
  }

  const match = preview.match(/"(?:command|cmd)"\s*:\s*"([^"]+)"/);
  if (match?.[1]) {
    return normalizePreviewText(match[1]);
  }

  return null;
}

function extractStructuredPreview(preview: string | null): string | null {
  const parsed = parsePreviewRecord(preview);
  if (parsed) {
    const meaningfulPreview = getFirstString(parsed, [
      'query',
      'q',
      'pattern',
      'path',
      'file_path',
      'url',
    ]);
    if (meaningfulPreview) {
      return normalizePreviewText(meaningfulPreview);
    }
  }

  return preview ? normalizePreviewText(preview) : null;
}

function parsePreviewRecord(preview: string | null): Record<string, unknown> | null {
  const trimmed = preview?.trim();
  if (!trimmed || (!trimmed.startsWith('{') && !trimmed.startsWith('['))) {
    return null;
  }

  try {
    const parsed = JSON.parse(trimmed);
    return parsed && typeof parsed === 'object' && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

function getFirstString(record: Record<string, unknown>, keys: string[]): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) {
      return value;
    }
  }

  return null;
}

function normalizePreviewText(text: string): string {
  return text
    .replace(/\s+/g, ' ')
    .replace(/\\"/g, '"')
    .trim();
}

function compactText(text: string, maxLength = 88) {
  const squashed = text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .join(' ');

  if (squashed.length <= maxLength) {
    return squashed;
  }

  return `${squashed.slice(0, maxLength - 1).trimEnd()}…`;
}

function roleRank(role: IslandChatMessage['role']) {
  switch (role) {
    case 'user':
      return 0;
    case 'assistant':
      return 1;
    default:
      return 2;
  }
}

function getLastItem<T>(items: T[]): T | undefined {
  return items.length > 0 ? items[items.length - 1] : undefined;
}

function normalizeCwd(cwd: string | null | undefined): string | null {
  const trimmed = cwd?.trim();
  if (!trimmed) {
    return null;
  }

  const slashNormalized = trimmed.replace(/\\/g, '/');
  const withoutTrailingSlash = slashNormalized.replace(/\/+$/, '');
  const normalized = withoutTrailingSlash || (slashNormalized.startsWith('/') ? '/' : slashNormalized);
  const compacted = normalized.replace(/\/{2,}/g, '/');

  return replaceHomeDirectoryPrefix(compacted);
}

function replaceHomeDirectoryPrefix(path: string): string {
  const homeDirectoryPatterns = [
    /^\/Users\/[^/]+(?=\/|$)/,
    /^\/home\/[^/]+(?=\/|$)/,
    /^[A-Za-z]:\/Users\/[^/]+(?=\/|$)/,
  ];

  for (const pattern of homeDirectoryPatterns) {
    if (pattern.test(path)) {
      return path.replace(pattern, '~');
    }
  }

  return path;
}
