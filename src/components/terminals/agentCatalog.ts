export const TERMINAL_AGENTS = [
  { id: 'codex', label: 'Codex CLI', command: 'codex' },
  { id: 'claude-code', label: 'Claude Code', command: 'claude' },
  { id: 'gemini', label: 'Gemini CLI', command: 'gemini' },
  { id: 'cursor', label: 'Cursor Agent', command: 'cursor-agent' },
  { id: 'copilot', label: 'GitHub Copilot', command: 'copilot' },
  { id: 'opencode', label: 'OpenCode', command: 'opencode' },
  { id: 'qwen-code', label: 'Qwen Code', command: 'qwen' },
  { id: 'hermes', label: 'Hermes', command: 'hermes' },
  { id: 'openclaw', label: 'OpenClaw', command: 'openclaw' },
  { id: 'qoder', label: 'Qoder', command: 'qoder' },
  { id: 'qoderwork', label: 'QoderWork', command: 'qoderwork' },
  { id: 'codebuddy', label: 'CodeBuddy', command: 'codebuddy' },
  { id: 'workbuddy', label: 'WorkBuddy', command: 'workbuddy' },
] as const;

export type TerminalAgentId = (typeof TERMINAL_AGENTS)[number]['id'];

export const DEFAULT_TERMINAL_AGENT_ID: TerminalAgentId = 'codex';

const TERMINAL_AGENT_IDS = new Set<string>(TERMINAL_AGENTS.map((agent) => agent.id));

export function isTerminalAgentId(value: unknown): value is TerminalAgentId {
  return typeof value === 'string' && TERMINAL_AGENT_IDS.has(value);
}

export function getTerminalAgent(agentId: TerminalAgentId) {
  return TERMINAL_AGENTS.find((agent) => agent.id === agentId) ?? TERMINAL_AGENTS[0];
}
