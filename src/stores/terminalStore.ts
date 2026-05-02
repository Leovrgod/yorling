import { create } from 'zustand';
import {
  DEFAULT_TERMINAL_AGENT_ID,
  TERMINAL_AGENTS,
  isTerminalAgentId,
  type TerminalAgentId,
} from '../components/terminals/agentCatalog.js';

const PROJECTS_KEY = 'yorling.multiTerminal.projects';
const SELECTED_PROJECT_KEY = 'yorling.multiTerminal.selectedProjectId';
const EXPANDED_KEY = 'yorling.multiTerminal.expanded';
const SELECTED_TERMINAL_KEY = 'yorling.multiTerminal.selectedTerminalId';
const MAX_TERMINAL_OUTPUT_LENGTH = 500_000;

export interface TerminalProjectAgent {
  id: string;
  agentId: TerminalAgentId;
  createdAt: number;
  updatedAt: number;
  lastLaunchedAt: number | null;
}

export interface TerminalProject {
  id: string;
  name: string;
  path: string;
  agents: TerminalProjectAgent[];
  selectedAgentId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface EmbeddedTerminalSession {
  id: string;
  title: string;
  cwd: string;
  providerId: string;
  displayName: string;
  status: 'idle' | 'running' | 'exited' | 'error';
  output: string;
  exitCode: number | null;
}

interface PendingEmbeddedTerminalEvents {
  output: string;
  exited: boolean;
  exitCode: number | null;
}

interface TerminalStoreState {
  projects: TerminalProject[];
  selectedProjectId: string | null;
  selectedTerminalId: string;
  multiTerminalExpanded: boolean;
  embeddedSessions: EmbeddedTerminalSession[];
  activeEmbeddedTerminalId: string | null;
  pendingEmbeddedTerminalEvents: Record<string, PendingEmbeddedTerminalEvents>;
  addProject: (path: string, agentId?: TerminalAgentId) => string | null;
  addProjectAgent: (projectId: string, agentId?: TerminalAgentId) => string | null;
  removeProject: (projectId: string) => void;
  selectProject: (projectId: string | null) => void;
  selectProjectAgent: (projectId: string, projectAgentId: string) => void;
  updateProjectAgent: (projectId: string, projectAgentId: string, agentId: TerminalAgentId) => void;
  markProjectLaunched: (projectId: string, projectAgentId: string) => void;
  setSelectedTerminal: (terminalId: string) => void;
  setMultiTerminalExpanded: (expanded: boolean) => void;
  addEmbeddedTerminalSession: (session: EmbeddedTerminalSession) => void;
  appendEmbeddedTerminalOutput: (id: string, data: string) => void;
  markEmbeddedTerminalExited: (id: string, code: number | null) => void;
  markEmbeddedTerminalError: (id: string, message: string) => void;
  removeEmbeddedTerminalSession: (id: string) => void;
  setActiveEmbeddedTerminal: (id: string | null) => void;
}

function canUseStorage(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined';
}

export function normalizeProjectPath(path: string): string {
  const trimmed = path.trim();
  if (!trimmed) return '';

  if (trimmed === '/') return trimmed;
  return trimmed.replace(/\/+$/, '');
}

function deriveProjectName(path: string): string {
  const normalized = normalizeProjectPath(path);
  const parts = normalized.split(/[\\/]/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : normalized;
}

function createProjectId(path: string): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') {
    return `terminal-project-${globalThis.crypto.randomUUID()}`;
  }

  const suffix = Math.random().toString(36).slice(2, 8);
  return `terminal-project-${Date.now()}-${suffix}-${deriveProjectName(path)}`;
}

function createProjectAgentId(): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') {
    return `terminal-agent-${globalThis.crypto.randomUUID()}`;
  }

  return `terminal-agent-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function createProjectAgent(agentId: TerminalAgentId, now = Date.now()): TerminalProjectAgent {
  return {
    id: createProjectAgentId(),
    agentId,
    createdAt: now,
    updatedAt: now,
    lastLaunchedAt: null,
  };
}

function numberOr(value: unknown, fallback: number): number {
  const numeric = Number(value);
  return Number.isFinite(numeric) ? numeric : fallback;
}

function getSelectedAgentId(
  agents: TerminalProjectAgent[],
  selectedAgentId: unknown,
): string | null {
  if (
    typeof selectedAgentId === 'string'
    && agents.some((agent) => agent.id === selectedAgentId)
  ) {
    return selectedAgentId;
  }

  return agents[0]?.id ?? null;
}

function parseStoredProjectAgent(
  value: unknown,
  fallbackLastLaunchedAt: number | null,
  now: number,
): TerminalProjectAgent | null {
  if (!value || typeof value !== 'object') return null;

  const item = value as Partial<TerminalProjectAgent>;
  const agentId = isTerminalAgentId(item.agentId) ? item.agentId : DEFAULT_TERMINAL_AGENT_ID;

  return {
    id: String(item.id ?? createProjectAgentId()),
    agentId,
    createdAt: numberOr(item.createdAt, now),
    updatedAt: numberOr(item.updatedAt, now),
    lastLaunchedAt: typeof item.lastLaunchedAt === 'number'
      ? item.lastLaunchedAt
      : fallbackLastLaunchedAt,
  };
}

function parseStoredProjectAgents(
  item: Partial<TerminalProject> & { agentId?: unknown; lastLaunchedAt?: unknown },
  now: number,
): TerminalProjectAgent[] {
  const fallbackLastLaunchedAt = typeof item.lastLaunchedAt === 'number' ? item.lastLaunchedAt : null;
  const rawAgents = Array.isArray(item.agents) ? item.agents : [];
  const agents = rawAgents.flatMap((rawAgent) => {
    const parsed = parseStoredProjectAgent(rawAgent, fallbackLastLaunchedAt, now);
    return parsed ? [parsed] : [];
  });

  if (agents.length > 0) {
    return agents;
  }

  const legacyAgentId = isTerminalAgentId(item.agentId) ? item.agentId : DEFAULT_TERMINAL_AGENT_ID;
  return [{
    ...createProjectAgent(legacyAgentId, now),
    lastLaunchedAt: fallbackLastLaunchedAt,
  }];
}

function parseStoredProjects(value: string | null): TerminalProject[] {
  if (!value) return [];

  try {
    const parsed = JSON.parse(value);
    if (!Array.isArray(parsed)) return [];

    return parsed.flatMap((item): TerminalProject[] => {
      if (!item || typeof item !== 'object') return [];

      const project = item as Partial<TerminalProject> & { agentId?: unknown; lastLaunchedAt?: unknown };
      const path = normalizeProjectPath(String(project.path ?? ''));
      if (!path) return [];

      const now = Date.now();
      const agents = parseStoredProjectAgents(project, now);

      return [{
        id: String(project.id ?? createProjectId(path)),
        name: String(project.name ?? deriveProjectName(path)),
        path,
        agents,
        selectedAgentId: getSelectedAgentId(agents, project.selectedAgentId),
        createdAt: numberOr(project.createdAt, now),
        updatedAt: numberOr(project.updatedAt, now),
      }];
    });
  } catch {
    return [];
  }
}

function readProjects(): TerminalProject[] {
  if (!canUseStorage()) return [];
  return parseStoredProjects(window.localStorage.getItem(PROJECTS_KEY));
}

function persistProjects(projects: TerminalProject[]): void {
  if (canUseStorage()) {
    window.localStorage.setItem(PROJECTS_KEY, JSON.stringify(projects));
  }
}

function readSelectedProjectId(projects: TerminalProject[]): string | null {
  if (!canUseStorage()) return null;

  const storedId = window.localStorage.getItem(SELECTED_PROJECT_KEY);
  if (storedId && projects.some((project) => project.id === storedId)) {
    return storedId;
  }

  return null;
}

function persistSelectedProjectId(projectId: string | null): void {
  if (!canUseStorage()) return;

  if (projectId) {
    window.localStorage.setItem(SELECTED_PROJECT_KEY, projectId);
  } else {
    window.localStorage.removeItem(SELECTED_PROJECT_KEY);
  }
}

function readExpanded(): boolean {
  if (!canUseStorage()) return true;
  const value = window.localStorage.getItem(EXPANDED_KEY);
  return value === null ? true : value === 'true';
}

function persistExpanded(expanded: boolean): void {
  if (canUseStorage()) {
    window.localStorage.setItem(EXPANDED_KEY, String(expanded));
  }
}

function readSelectedTerminalId(): string {
  if (!canUseStorage()) return 'embedded';
  return window.localStorage.getItem(SELECTED_TERMINAL_KEY) || 'embedded';
}

function persistSelectedTerminalId(terminalId: string): void {
  if (canUseStorage()) {
    window.localStorage.setItem(SELECTED_TERMINAL_KEY, terminalId);
  }
}

function getSuggestedAgentId(project: TerminalProject): TerminalAgentId {
  return TERMINAL_AGENTS.find((agent) => (
    !project.agents.some((projectAgent) => projectAgent.agentId === agent.id)
  ))?.id ?? project.agents[project.agents.length - 1]?.agentId ?? DEFAULT_TERMINAL_AGENT_ID;
}

export function trimTerminalOutput(output: string): string {
  if (output.length <= MAX_TERMINAL_OUTPUT_LENGTH) {
    return output;
  }

  return output.slice(output.length - MAX_TERMINAL_OUTPUT_LENGTH);
}

function formatEmbeddedTerminalExit(code: number | null): string {
  return `\r\n[process exited${code === null ? '' : ` with code ${code}`}]\r\n`;
}

function mergePendingEmbeddedTerminalEvent(
  pending: PendingEmbeddedTerminalEvents | undefined,
  output: string,
  exited = pending?.exited ?? false,
  exitCode = pending?.exitCode ?? null,
): PendingEmbeddedTerminalEvents {
  return {
    output: trimTerminalOutput(`${pending?.output ?? ''}${output}`),
    exited,
    exitCode,
  };
}

function resolveActiveEmbeddedTerminalId(
  sessions: EmbeddedTerminalSession[],
  currentId: string | null,
): string | null {
  if (currentId && sessions.some((session) => session.id === currentId)) {
    return currentId;
  }

  return sessions.length > 0 ? sessions[sessions.length - 1].id : null;
}

const initialProjects = readProjects();

export const useTerminalStore = create<TerminalStoreState>((set, get) => ({
  projects: initialProjects,
  selectedProjectId: readSelectedProjectId(initialProjects),
  selectedTerminalId: readSelectedTerminalId(),
  multiTerminalExpanded: readExpanded(),
  embeddedSessions: [],
  activeEmbeddedTerminalId: null,
  pendingEmbeddedTerminalEvents: {},
  addProject: (rawPath, agentId = DEFAULT_TERMINAL_AGENT_ID) => {
    const path = normalizeProjectPath(rawPath);
    if (!path) return null;

    const existing = get().projects.find((project) => project.path === path);
    if (existing) {
      persistSelectedProjectId(existing.id);
      set({ selectedProjectId: existing.id, multiTerminalExpanded: true });
      persistExpanded(true);
      return existing.id;
    }

    const now = Date.now();
    const initialAgent = createProjectAgent(agentId, now);
    const project: TerminalProject = {
      id: createProjectId(path),
      name: deriveProjectName(path),
      path,
      agents: [initialAgent],
      selectedAgentId: initialAgent.id,
      createdAt: now,
      updatedAt: now,
    };

    const projects = [...get().projects, project];
    persistProjects(projects);
    persistSelectedProjectId(project.id);
    persistExpanded(true);
    set({
      projects,
      selectedProjectId: project.id,
      multiTerminalExpanded: true,
    });

    return project.id;
  },
  addProjectAgent: (projectId, agentId) => {
    const project = get().projects.find((item) => item.id === projectId);
    if (!project) return null;

    const now = Date.now();
    const projectAgent = createProjectAgent(agentId ?? getSuggestedAgentId(project), now);
    const projects = get().projects.map((item) => (
      item.id === projectId
        ? {
          ...item,
          agents: [...item.agents, projectAgent],
          selectedAgentId: projectAgent.id,
          updatedAt: now,
        }
        : item
    ));

    persistProjects(projects);
    set({ projects });

    return projectAgent.id;
  },
  removeProject: (projectId) => {
    const projects = get().projects.filter((project) => project.id !== projectId);
    const selectedProjectId = get().selectedProjectId === projectId ? null : get().selectedProjectId;

    persistProjects(projects);
    persistSelectedProjectId(selectedProjectId);
    set({ projects, selectedProjectId });
  },
  selectProject: (projectId) => {
    const nextProjectId = projectId && get().projects.some((project) => project.id === projectId)
      ? projectId
      : null;

    persistSelectedProjectId(nextProjectId);
    set({ selectedProjectId: nextProjectId });
  },
  selectProjectAgent: (projectId, projectAgentId) => {
    const projects = get().projects.map((project) => {
      if (
        project.id !== projectId
        || !project.agents.some((agent) => agent.id === projectAgentId)
      ) {
        return project;
      }

      return {
        ...project,
        selectedAgentId: projectAgentId,
        updatedAt: Date.now(),
      };
    });

    persistProjects(projects);
    set({ projects });
  },
  updateProjectAgent: (projectId, projectAgentId, agentId) => {
    const timestamp = Date.now();
    const projects = get().projects.map((project) => {
      if (project.id !== projectId) {
        return project;
      }

      let changed = false;
      const agents = project.agents.map((agent) => {
        if (agent.id !== projectAgentId || agent.agentId === agentId) {
          return agent;
        }

        changed = true;
        return {
          ...agent,
          agentId,
          updatedAt: timestamp,
        };
      });

      if (!changed) {
        return project;
      }

      return {
        ...project,
        agents,
        updatedAt: timestamp,
      };
    });

    persistProjects(projects);
    set({ projects });
  },
  markProjectLaunched: (projectId, projectAgentId) => {
    const timestamp = Date.now();
    const projects = get().projects.map((project) => {
      if (project.id !== projectId) {
        return project;
      }

      let changed = false;
      const agents = project.agents.map((agent) => {
        if (agent.id !== projectAgentId) {
          return agent;
        }

        changed = true;
        return {
          ...agent,
          lastLaunchedAt: timestamp,
          updatedAt: timestamp,
        };
      });

      if (!changed) {
        return project;
      }

      return {
        ...project,
        agents,
        updatedAt: timestamp,
      };
    });

    persistProjects(projects);
    set({ projects });
  },
  setSelectedTerminal: (terminalId) => {
    persistSelectedTerminalId(terminalId);
    set({ selectedTerminalId: terminalId });
  },
  setMultiTerminalExpanded: (expanded) => {
    persistExpanded(expanded);
    set({ multiTerminalExpanded: expanded });
  },
  addEmbeddedTerminalSession: (session) => set((state) => {
    const pending = state.pendingEmbeddedTerminalEvents[session.id];
    const nextPending = { ...state.pendingEmbeddedTerminalEvents };
    delete nextPending[session.id];

    const nextSession: EmbeddedTerminalSession = pending
      ? {
        ...session,
        status: pending.exited ? 'exited' : session.status,
        output: trimTerminalOutput(`${session.output}${pending.output}`),
        exitCode: pending.exited ? pending.exitCode : session.exitCode,
      }
      : session;
    const sessions = [
      ...state.embeddedSessions.filter((item) => item.id !== session.id),
      nextSession,
    ];

    return {
      embeddedSessions: sessions,
      activeEmbeddedTerminalId: nextSession.id,
      pendingEmbeddedTerminalEvents: nextPending,
    };
  }),
  appendEmbeddedTerminalOutput: (id, data) => set((state) => {
    let found = false;
    const sessions: EmbeddedTerminalSession[] = state.embeddedSessions.map((session): EmbeddedTerminalSession => {
      if (session.id !== id) {
        return session;
      }

      found = true;
      return {
        ...session,
        output: trimTerminalOutput(`${session.output}${data}`),
        status: session.status === 'idle' ? 'running' : session.status,
      };
    });

    if (found) {
      return { embeddedSessions: sessions };
    }

    return {
      pendingEmbeddedTerminalEvents: {
        ...state.pendingEmbeddedTerminalEvents,
        [id]: mergePendingEmbeddedTerminalEvent(
          state.pendingEmbeddedTerminalEvents[id],
          data,
        ),
      },
    };
  }),
  markEmbeddedTerminalExited: (id, code) => set((state) => {
    let found = false;
    const sessions: EmbeddedTerminalSession[] = state.embeddedSessions.map((session): EmbeddedTerminalSession => {
      if (session.id !== id) {
        return session;
      }

      found = true;
      if (session.status === 'exited') {
        return {
          ...session,
          exitCode: code,
        };
      }

      return {
        ...session,
        status: 'exited',
        exitCode: code,
        output: trimTerminalOutput(`${session.output}${formatEmbeddedTerminalExit(code)}`),
      };
    });

    if (found) {
      return { embeddedSessions: sessions };
    }

    return {
      pendingEmbeddedTerminalEvents: {
        ...state.pendingEmbeddedTerminalEvents,
        [id]: mergePendingEmbeddedTerminalEvent(
          state.pendingEmbeddedTerminalEvents[id],
          formatEmbeddedTerminalExit(code),
          true,
          code,
        ),
      },
    };
  }),
  markEmbeddedTerminalError: (id, message) => set((state) => ({
    embeddedSessions: state.embeddedSessions.map((session) => (
      session.id === id
        ? {
          ...session,
          status: 'error',
          output: trimTerminalOutput(`${session.output}\r\n${message}\r\n`),
        }
        : session
    )),
  })),
  removeEmbeddedTerminalSession: (id) => set((state) => {
    const sessions = state.embeddedSessions.filter((session) => session.id !== id);
    const pending = { ...state.pendingEmbeddedTerminalEvents };
    delete pending[id];

    return {
      embeddedSessions: sessions,
      activeEmbeddedTerminalId: resolveActiveEmbeddedTerminalId(
        sessions,
        state.activeEmbeddedTerminalId === id ? null : state.activeEmbeddedTerminalId,
      ),
      pendingEmbeddedTerminalEvents: pending,
    };
  }),
  setActiveEmbeddedTerminal: (id) => set((state) => ({
    activeEmbeddedTerminalId: resolveActiveEmbeddedTerminalId(state.embeddedSessions, id),
  })),
}));

export function getSelectedTerminalProject(
  projects: TerminalProject[],
  selectedProjectId: string | null,
) {
  if (!selectedProjectId) {
    return null;
  }

  return projects.find((project) => project.id === selectedProjectId) ?? null;
}

export function getSelectedTerminalProjectAgent(project: TerminalProject | null) {
  if (!project) {
    return null;
  }

  return project.agents.find((agent) => agent.id === project.selectedAgentId) ?? project.agents[0] ?? null;
}
