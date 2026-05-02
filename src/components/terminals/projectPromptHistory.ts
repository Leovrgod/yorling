import { getVisiblePromptMessages } from '../../island/presentation.js';
import { normalizeProjectPath } from '../../stores/terminalStore.js';
import type { TerminalProject } from '../../stores/terminalStore.js';
import type { IslandSession } from '../../types';

export interface ProjectPromptHistoryItem {
  id: string;
  sessionId: string;
  providerId: string;
  text: string;
  timestamp: number;
  cwd: string | null;
}

export const PROJECT_PROMPT_HISTORY_LIMIT = 100;

export function normalizeProjectMatchPath(path: string | null | undefined): string {
  const normalized = normalizeProjectPath((path ?? '').replace(/\\/g, '/').replace(/\/{2,}/g, '/'));
  if (normalized.startsWith('/private/tmp/')) {
    return `/tmp/${normalized.slice('/private/tmp/'.length)}`;
  }
  if (normalized === '/private/tmp') {
    return '/tmp';
  }
  if (normalized.startsWith('/private/var/')) {
    return `/var/${normalized.slice('/private/var/'.length)}`;
  }
  if (normalized === '/private/var') {
    return '/var';
  }

  return normalized;
}

export function doesSessionExactlyMatchProject(
  projectPath: string,
  cwd: string | null | undefined,
): boolean {
  if (!cwd) {
    return false;
  }

  return normalizeProjectMatchPath(cwd) === normalizeProjectMatchPath(projectPath);
}

export function doesSessionBelongToProject(
  projectPath: string,
  cwd: string | null | undefined,
): boolean {
  if (!cwd) {
    return false;
  }

  const normalizedProject = normalizeProjectMatchPath(projectPath);
  const normalizedCwd = normalizeProjectMatchPath(cwd);
  return normalizedCwd === normalizedProject || normalizedCwd.startsWith(`${normalizedProject}/`);
}

export function collectProjectPromptHistory(
  sessions: IslandSession[],
  projectPath: string,
  limit = PROJECT_PROMPT_HISTORY_LIMIT,
): ProjectPromptHistoryItem[] {
  const unique = new Map<string, ProjectPromptHistoryItem>();

  for (const session of sessions) {
    const cwd = session.terminal_context?.cwd ?? null;
    if (!doesSessionBelongToProject(projectPath, cwd)) {
      continue;
    }

    const prompts = getVisiblePromptMessages(session);
    prompts.forEach((message, index) => {
      const text = message.text.trim();
      if (!text) {
        return;
      }

      const id = `${session.id}:${message.timestamp}:${index}:${text}`;
      unique.set(id, {
        id,
        sessionId: session.id,
        providerId: session.provider_id,
        text,
        timestamp: message.timestamp,
        cwd: normalizeProjectPath(cwd ?? '') || null,
      });
    });
  }

  return [...unique.values()]
    .sort((left, right) => (
      right.timestamp - left.timestamp
      || right.sessionId.localeCompare(left.sessionId)
      || right.id.localeCompare(left.id)
    ))
    .slice(0, limit);
}

export function mergeProjectPromptHistory(
  existing: ProjectPromptHistoryItem[],
  incoming: ProjectPromptHistoryItem[],
  limit = PROJECT_PROMPT_HISTORY_LIMIT,
): ProjectPromptHistoryItem[] {
  const merged = new Map<string, ProjectPromptHistoryItem>();
  [...existing, ...incoming].forEach((item) => {
    if (!item.text.trim()) {
      return;
    }
    merged.set(item.id, {
      ...item,
      text: item.text.trim(),
      cwd: item.cwd ? normalizeProjectPath(item.cwd) : null,
    });
  });

  return [...merged.values()]
    .sort((left, right) => (
      right.timestamp - left.timestamp
      || right.sessionId.localeCompare(left.sessionId)
      || right.id.localeCompare(left.id)
    ))
    .slice(0, limit);
}

export function syncProjectPromptHistories(
  projects: TerminalProject[],
  sessions: IslandSession[],
  existing: Record<string, ProjectPromptHistoryItem[]>,
): Record<string, ProjectPromptHistoryItem[]> {
  return projects.reduce<Record<string, ProjectPromptHistoryItem[]>>((next, project) => {
    const live = collectProjectPromptHistory(sessions, project.path);
    const merged = mergeProjectPromptHistory(existing[project.id] ?? [], live);
    next[project.id] = merged;
    return next;
  }, {});
}

function areProjectPromptHistoryItemsEqual(
  left: ProjectPromptHistoryItem[],
  right: ProjectPromptHistoryItem[],
): boolean {
  if (left.length !== right.length) {
    return false;
  }

  return left.every((item, index) => {
    const other = right[index];
    return item.id === other?.id
      && item.sessionId === other.sessionId
      && item.providerId === other.providerId
      && item.text === other.text
      && item.timestamp === other.timestamp
      && item.cwd === other.cwd;
  });
}

export function areProjectPromptHistoryMapsEqual(
  left: Record<string, ProjectPromptHistoryItem[]>,
  right: Record<string, ProjectPromptHistoryItem[]>,
): boolean {
  const leftKeys = Object.keys(left).sort();
  const rightKeys = Object.keys(right).sort();
  if (leftKeys.length !== rightKeys.length) {
    return false;
  }

  return leftKeys.every((key, index) => (
    key === rightKeys[index]
    && areProjectPromptHistoryItemsEqual(left[key] ?? [], right[key] ?? [])
  ));
}
