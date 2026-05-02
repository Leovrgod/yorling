import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { getUiCopy } from '../../i18n/copy';
import { MascotView } from '../../island/components/MascotView';
import { useAppStore } from '../../stores/appStore';
import {
  getSelectedTerminalProject,
  getSelectedTerminalProjectAgent,
  normalizeProjectPath,
  useTerminalStore,
  type EmbeddedTerminalSession,
  type TerminalProject,
  type TerminalProjectAgent,
} from '../../stores/terminalStore';
import type { IslandChatMessage, IslandSession } from '../../types';
import {
  getTerminalAgent,
  isTerminalAgentId,
  TERMINAL_AGENTS,
  type TerminalAgentId,
} from './agentCatalog';
import {
  PROJECT_PROMPT_HISTORY_LIMIT,
  areProjectPromptHistoryMapsEqual,
  collectProjectPromptHistory,
  doesSessionExactlyMatchProject,
  mergeProjectPromptHistory,
  syncProjectPromptHistories,
  type ProjectPromptHistoryItem,
} from './projectPromptHistory';

type LaunchState =
  | { status: 'idle'; message: string | null }
  | { status: 'launching'; message: string | null }
  | { status: 'success'; message: string }
  | { status: 'error'; message: string };

interface MappedOpenSession {
  session: IslandSession;
  project: TerminalProject | null;
  cwd: string | null;
  summary: string | null;
}

interface DetectedTerminalAgent {
  id: string;
  label: string;
  command: string;
  installed: boolean;
  path: string | null;
}

interface DetectedTerminalApp {
  id: string;
  label: string;
  installed: boolean;
  embedded: boolean;
  bundle_id: string | null;
  app_path: string | null;
}

interface TerminalInventory {
  agents: DetectedTerminalAgent[];
  terminals: DetectedTerminalApp[];
}

interface EmbeddedTerminalLaunch {
  id: string;
  cwd: string;
  provider_id: string;
  display_name: string;
}

const INITIAL_TERMINAL_COLS = 120;
const INITIAL_TERMINAL_ROWS = 32;
const PROMPT_HISTORY_STORAGE_KEY = 'yorling.multiTerminal.promptHistory';
const PROMPT_COPY_RESET_MS = 1_600;

function formatPath(path: string): string {
  const parts = path.split('/').filter(Boolean);
  if (parts.length <= 3) {
    return path;
  }

  return `.../${parts.slice(-3).join('/')}`;
}

function formatProviderLabel(providerId: string): string {
  if (isTerminalAgentId(providerId)) {
    return getTerminalAgent(providerId).label;
  }

  return providerId
    .split('-')
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function canUseStorage(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined';
}

function parseStoredPromptHistories(value: string | null): Record<string, ProjectPromptHistoryItem[]> {
  if (!value) {
    return {};
  }

  try {
    const parsed = JSON.parse(value);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
      return {};
    }

    return Object.entries(parsed).reduce<Record<string, ProjectPromptHistoryItem[]>>((next, [projectId, rawItems]) => {
      if (!Array.isArray(rawItems)) {
        return next;
      }

      const items = rawItems.flatMap((rawItem) => {
        if (!rawItem || typeof rawItem !== 'object') {
          return [];
        }

        const item = rawItem as Partial<ProjectPromptHistoryItem>;
        const text = typeof item.text === 'string' ? item.text.trim() : '';
        if (!text) {
          return [];
        }

        return [{
          id: String(item.id ?? `${projectId}:${item.timestamp ?? 0}:${text}`),
          sessionId: String(item.sessionId ?? ''),
          providerId: String(item.providerId ?? ''),
          text,
          timestamp: Number.isFinite(item.timestamp) ? Number(item.timestamp) : 0,
          cwd: typeof item.cwd === 'string' ? normalizeProjectPath(item.cwd) || null : null,
        }];
      });

      next[projectId] = mergeProjectPromptHistory([], items);
      return next;
    }, {});
  } catch {
    return {};
  }
}

function readStoredPromptHistories(): Record<string, ProjectPromptHistoryItem[]> {
  if (!canUseStorage()) {
    return {};
  }

  return parseStoredPromptHistories(window.localStorage.getItem(PROMPT_HISTORY_STORAGE_KEY));
}

function persistStoredPromptHistories(histories: Record<string, ProjectPromptHistoryItem[]>): void {
  if (canUseStorage()) {
    window.localStorage.setItem(PROMPT_HISTORY_STORAGE_KEY, JSON.stringify(histories));
  }
}

function formatPromptTimestamp(timestamp: number, language: 'zh' | 'en'): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return '';
  }

  return new Intl.DateTimeFormat(language === 'zh' ? 'zh-CN' : 'en-US', {
    month: 'numeric',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  }).format(timestamp);
}

function fallbackTerminalInventory(): TerminalInventory {
  return {
    agents: TERMINAL_AGENTS.map((agent) => ({
      id: agent.id,
      label: agent.label,
      command: agent.command,
      installed: true,
      path: null,
    })),
    terminals: [
      {
        id: 'embedded',
        label: 'Default',
        installed: true,
        embedded: true,
        bundle_id: null,
        app_path: null,
      },
      {
        id: 'terminal',
        label: 'Terminal',
        installed: true,
        embedded: false,
        bundle_id: 'com.apple.Terminal',
        app_path: null,
      },
    ],
  };
}

function getDetectedAgentLabel(agent: { id: TerminalAgentId; label: string }, inventory: TerminalInventory | null) {
  return inventory?.agents.find((item) => item.id === agent.id)?.label ?? agent.label;
}

function getLatestMessageText(messages: IslandChatMessage[]): string | null {
  return [...messages]
    .reverse()
    .map((message) => message.text.trim())
    .find(Boolean) ?? null;
}

function getSessionSummary(session: IslandSession): string | null {
  if (session.pending_question?.question.trim()) {
    return session.pending_question.question.trim();
  }

  if (session.task_title?.trim()) {
    return session.task_title.trim();
  }

  const liveMessage = getLatestMessageText(session.chat_messages);
  if (liveMessage) {
    return liveMessage;
  }

  const transcriptPreview = session.transcript_preview?.chat_preview ?? [];
  return [...transcriptPreview]
    .reverse()
    .map((message) => message.text.trim())
    .find(Boolean) ?? null;
}

function getSessionSortTimestamp(session: IslandSession): number {
  const chatTimestamp = [...session.chat_messages]
    .reverse()
    .find((message) => Number.isFinite(message.timestamp))?.timestamp ?? 0;
  const transcriptTimestamp = session.transcript_preview?.synced_at ?? 0;
  const questionTimestamp = session.pending_question?.received_at ?? 0;
  const permissionTimestamp = session.pending_permission?.received_at ?? 0;

  return Math.max(
    session.started_at ?? 0,
    chatTimestamp,
    transcriptTimestamp,
    questionTimestamp,
    permissionTimestamp,
  );
}

function upsertSession(existing: IslandSession[], next: IslandSession): IslandSession[] {
  const index = existing.findIndex((session) => session.id === next.id);
  if (index === -1) {
    return [...existing, next];
  }

  const sessions = [...existing];
  sessions[index] = next;
  return sessions;
}

function ProjectPromptHistoryPanel({
  language,
  copy,
  project,
  items,
  copiedPromptId,
  onCopyPrompt,
  onClose,
}: {
  language: 'zh' | 'en';
  copy: ReturnType<typeof getUiCopy>['terminals'];
  project: TerminalProject;
  items: ProjectPromptHistoryItem[];
  copiedPromptId: string | null;
  onCopyPrompt: (item: ProjectPromptHistoryItem) => void;
  onClose: () => void;
}) {
  return (
    <aside className="multi-terminal-prompt-panel" aria-label={copy.promptHistoryTitle}>
      <div className="multi-terminal-prompt-panel-header">
        <div className="multi-terminal-prompt-panel-heading">
          <strong>{copy.promptHistoryTitle}</strong>
          <span>{copy.promptHistoryRecent(PROJECT_PROMPT_HISTORY_LIMIT)}</span>
        </div>
        <button
          type="button"
          className="multi-terminal-prompt-panel-close"
          onClick={onClose}
          aria-label={copy.promptHistoryClose}
          title={copy.promptHistoryClose}
        >
          ×
        </button>
      </div>
      <div className="multi-terminal-prompt-panel-project" title={project.path}>
        <span>{project.name}</span>
        <small>{project.path}</small>
      </div>
      <div className="multi-terminal-prompt-panel-body">
        {items.length > 0 ? items.map((item) => {
          const copied = copiedPromptId === item.id;
          const timeLabel = formatPromptTimestamp(item.timestamp, language);
          const promptCwd = item.cwd ?? '';
          const showCwd = Boolean(promptCwd) && !doesSessionExactlyMatchProject(project.path, promptCwd);

          return (
            <article key={item.id} className="multi-terminal-prompt-item">
              <div className="multi-terminal-prompt-item-header">
                <div className="multi-terminal-prompt-item-meta">
                  <strong>{formatProviderLabel(item.providerId)}</strong>
                  {timeLabel ? <span>{timeLabel}</span> : null}
                  {showCwd ? <code title={promptCwd}>{formatPath(promptCwd)}</code> : null}
                </div>
                <button
                  type="button"
                  className={`multi-terminal-prompt-item-copy${copied ? ' copied' : ''}`}
                  onClick={() => onCopyPrompt(item)}
                  aria-label={copied ? copy.promptHistoryCopied : copy.promptHistoryCopy}
                  title={copied ? copy.promptHistoryCopied : copy.promptHistoryCopy}
                >
                  {copied ? copy.promptHistoryCopied : copy.promptHistoryCopy}
                </button>
              </div>
              <pre className="multi-terminal-prompt-item-text">{item.text}</pre>
            </article>
          );
        }) : (
          <div className="multi-terminal-empty multi-terminal-prompt-panel-empty">
            <span>{copy.promptHistoryEmptyTitle}</span>
            <p>{copy.promptHistoryEmptyDescription}</p>
          </div>
        )}
      </div>
    </aside>
  );
}

function OpenAgentList({
  items,
  selectedProject,
  selectedProjectAgent,
  onJumpToSession,
}: {
  items: MappedOpenSession[];
  selectedProject: TerminalProject | null;
  selectedProjectAgent: TerminalProjectAgent | null;
  onJumpToSession: (sessionId: string) => void;
}) {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language).terminals;

  if (items.length === 0) {
    return (
      <div className="multi-terminal-empty multi-terminal-empty-session-list">
        <span>{selectedProject ? copy.noOpenAgentsInProject : copy.noOpenAgents}</span>
      </div>
    );
  }

  return (
    <div className="multi-terminal-session-list" aria-label={copy.openAgentsTitle}>
      {items.map(({ session, project, cwd, summary }) => {
        const isSelectedAgent = !!selectedProjectAgent && session.provider_id === selectedProjectAgent.agentId;

        return (
          <button
            key={session.id}
            type="button"
            className={`multi-terminal-session-row ${isSelectedAgent ? 'active' : ''}`}
            onClick={() => onJumpToSession(session.id)}
            title={cwd ?? summary ?? formatProviderLabel(session.provider_id)}
          >
            <div className="multi-terminal-session-icon">
              <MascotView providerId={session.provider_id} phase={session.phase} size={28} />
            </div>
            <div className="multi-terminal-session-copy">
              <div className="multi-terminal-session-title-row">
                <strong>{formatProviderLabel(session.provider_id)}</strong>
                <span className="multi-terminal-session-project-tag">
                  {project?.name ?? copy.allProjects}
                </span>
              </div>
              {cwd ? <code className="multi-terminal-session-path">{cwd}</code> : null}
              {summary ? <span className="multi-terminal-session-summary">{summary}</span> : null}
            </div>
          </button>
        );
      })}
    </div>
  );
}

function EmbeddedTerminalPane({ session }: { session: EmbeddedTerminalSession }) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const writtenOutputRef = useRef('');

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    container.textContent = '';

    const terminal = new Terminal({
      cols: INITIAL_TERMINAL_COLS,
      rows: INITIAL_TERMINAL_ROWS,
      cursorBlink: true,
      convertEol: false,
      fontFamily: 'SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
      fontSize: 12,
      lineHeight: 1.2,
      letterSpacing: 0,
      scrollback: 20_000,
      tabStopWidth: 4,
      theme: {
        background: '#101010',
        foreground: '#f3f3f3',
        cursor: '#f3f3f3',
        selectionBackground: '#f3f3f333',
        black: '#1b1b1b',
        red: '#ff6b6b',
        green: '#76d275',
        yellow: '#ffd166',
        blue: '#74a7ff',
        magenta: '#d6a4ff',
        cyan: '#64d8ff',
        white: '#f2f2f2',
        brightBlack: '#6f6f6f',
        brightRed: '#ff8585',
        brightGreen: '#8de88d',
        brightYellow: '#ffe08a',
        brightBlue: '#95bdff',
        brightMagenta: '#e5bdff',
        brightCyan: '#8be6ff',
        brightWhite: '#ffffff',
      },
    });
    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon((_event, uri) => {
      window.open(uri, '_blank', 'noopener,noreferrer');
    });
    const lastSize = { cols: 0, rows: 0 };
    let resizeFrame: number | null = null;

    terminal.loadAddon(fitAddon);
    terminal.loadAddon(webLinksAddon);
    terminal.open(container);
    terminal.write(session.output);
    terminalRef.current = terminal;
    writtenOutputRef.current = session.output;

    const sendResize = () => {
      resizeFrame = null;
      try {
        fitAddon.fit();
      } catch {
        return;
      }

      if (terminal.cols === lastSize.cols && terminal.rows === lastSize.rows) {
        return;
      }

      lastSize.cols = terminal.cols;
      lastSize.rows = terminal.rows;
      void invoke('resize_embedded_terminal', {
        id: session.id,
        cols: terminal.cols,
        rows: terminal.rows,
      });
    };

    const scheduleResize = () => {
      if (resizeFrame !== null) return;
      resizeFrame = window.requestAnimationFrame(sendResize);
    };

    scheduleResize();
    const focusFrame = window.requestAnimationFrame(() => terminal.focus());
    const resizeObserver = new ResizeObserver(scheduleResize);
    resizeObserver.observe(container);

    const dataDisposable = terminal.onData((data) => {
      void invoke('send_embedded_terminal_input', {
        id: session.id,
        data,
      });
    });

    return () => {
      terminalRef.current = null;
      window.cancelAnimationFrame(focusFrame);
      if (resizeFrame !== null) {
        window.cancelAnimationFrame(resizeFrame);
      }
      resizeObserver.disconnect();
      dataDisposable.dispose();
      terminal.dispose();
    };
  }, [session.id]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal || writtenOutputRef.current === session.output) return;

    if (session.output.startsWith(writtenOutputRef.current)) {
      terminal.write(session.output.slice(writtenOutputRef.current.length));
    } else {
      terminal.clear();
      terminal.write(session.output);
    }
    writtenOutputRef.current = session.output;
  }, [session.output]);

  return (
    <div
      ref={containerRef}
      className="multi-terminal-xterm"
      onMouseDown={() => {
        const textarea = containerRef.current?.querySelector('textarea');
        textarea?.focus();
      }}
    />
  );
}

export function MultiTerminal() {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language).terminals;
  const projects = useTerminalStore((state) => state.projects);
  const selectedProjectId = useTerminalStore((state) => state.selectedProjectId);
  const selectedTerminalId = useTerminalStore((state) => state.selectedTerminalId);
  const removeProject = useTerminalStore((state) => state.removeProject);
  const updateProjectAgent = useTerminalStore((state) => state.updateProjectAgent);
  const markProjectLaunched = useTerminalStore((state) => state.markProjectLaunched);
  const setSelectedTerminal = useTerminalStore((state) => state.setSelectedTerminal);
  const embeddedSessions = useTerminalStore((state) => state.embeddedSessions);
  const activeEmbeddedTerminalId = useTerminalStore((state) => state.activeEmbeddedTerminalId);
  const addEmbeddedTerminalSession = useTerminalStore((state) => state.addEmbeddedTerminalSession);
  const markEmbeddedTerminalError = useTerminalStore((state) => state.markEmbeddedTerminalError);
  const removeEmbeddedTerminalSession = useTerminalStore((state) => state.removeEmbeddedTerminalSession);
  const setActiveEmbeddedTerminal = useTerminalStore((state) => state.setActiveEmbeddedTerminal);
  const [launchState, setLaunchState] = useState<LaunchState>({ status: 'idle', message: null });
  const [sessions, setSessions] = useState<IslandSession[]>([]);
  const [inventory, setInventory] = useState<TerminalInventory | null>(null);
  const [promptHistories, setPromptHistories] = useState<Record<string, ProjectPromptHistoryItem[]>>(
    () => readStoredPromptHistories(),
  );
  const [isPromptHistoryOpen, setIsPromptHistoryOpen] = useState(false);
  const [copiedPromptId, setCopiedPromptId] = useState<string | null>(null);
  const [pendingRemovalProjectId, setPendingRemovalProjectId] = useState<string | null>(null);
  const promptCopyResetRef = useRef<number | null>(null);

  const selectedProject = useMemo(
    () => getSelectedTerminalProject(projects, selectedProjectId),
    [projects, selectedProjectId],
  );
  const selectedProjectAgent = useMemo(
    () => getSelectedTerminalProjectAgent(selectedProject),
    [selectedProject],
  );
  const effectiveInventory = inventory ?? fallbackTerminalInventory();
  const availableAgents = useMemo(() => {
    const installedIds = new Set(
      effectiveInventory.agents
        .filter((agent) => agent.installed)
        .map((agent) => agent.id),
    );
    const installedAgents = TERMINAL_AGENTS.filter((agent) => installedIds.has(agent.id));
    return installedAgents.length > 0 ? installedAgents : TERMINAL_AGENTS;
  }, [effectiveInventory]);
  const availableTerminals = useMemo(() => {
    const installedTerminals = effectiveInventory.terminals.filter((terminal) => (
      terminal.embedded || terminal.installed
    ));
    return installedTerminals.length > 0 ? installedTerminals : fallbackTerminalInventory().terminals;
  }, [effectiveInventory]);
  const selectedTerminal = availableTerminals.find((terminal) => terminal.id === selectedTerminalId)
    ?? availableTerminals[0]
    ?? fallbackTerminalInventory().terminals[0];
  const visibleEmbeddedSessions = useMemo(() => {
    if (!selectedProject) {
      return embeddedSessions;
    }

    return embeddedSessions.filter((session) => (
      doesSessionExactlyMatchProject(selectedProject.path, session.cwd)
    ));
  }, [embeddedSessions, selectedProject]);
  const activeEmbeddedTerminal = visibleEmbeddedSessions.find((session) => (
    session.id === activeEmbeddedTerminalId
  )) ?? visibleEmbeddedSessions[visibleEmbeddedSessions.length - 1] ?? null;
  const resolvedActiveEmbeddedTerminalId = activeEmbeddedTerminal?.id ?? null;
  const selectedProjectPromptHistory = useMemo(() => {
    if (!selectedProject) {
      return [];
    }

    return promptHistories[selectedProject.id]
      ?? collectProjectPromptHistory(sessions, selectedProject.path);
  }, [promptHistories, selectedProject, sessions]);

  useEffect(() => {
    let active = true;

    const fetchInventory = async () => {
      try {
        const nextInventory = await invoke<TerminalInventory>('get_terminal_inventory');
        if (active) {
          setInventory(nextInventory);
        }
      } catch {
        if (active) {
          setInventory(fallbackTerminalInventory());
        }
      }
    };

    void fetchInventory();
    const interval = window.setInterval(() => {
      void fetchInventory();
    }, 60_000);

    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (!selectedProject || !selectedProjectAgent || availableAgents.length === 0) return;
    if (availableAgents.some((agent) => agent.id === selectedProjectAgent.agentId)) return;

    updateProjectAgent(selectedProject.id, selectedProjectAgent.id, availableAgents[0].id);
  }, [availableAgents, selectedProject, selectedProjectAgent, updateProjectAgent]);

  useEffect(() => {
    if (availableTerminals.some((terminal) => terminal.id === selectedTerminalId)) return;

    setSelectedTerminal(availableTerminals[0]?.id ?? 'embedded');
  }, [availableTerminals, selectedTerminalId, setSelectedTerminal]);

  useEffect(() => () => {
    if (promptCopyResetRef.current !== null) {
      window.clearTimeout(promptCopyResetRef.current);
    }
  }, []);

  useEffect(() => {
    if (!resolvedActiveEmbeddedTerminalId || resolvedActiveEmbeddedTerminalId === activeEmbeddedTerminalId) {
      return;
    }

    setActiveEmbeddedTerminal(resolvedActiveEmbeddedTerminalId);
  }, [activeEmbeddedTerminalId, resolvedActiveEmbeddedTerminalId, setActiveEmbeddedTerminal]);

  useEffect(() => {
    let active = true;

    const fetchSessions = async () => {
      try {
        const nextSessions = await invoke<IslandSession[]>('get_island_sessions');
        if (active) {
          setSessions(nextSessions);
        }
      } catch {
        // The island bridge may not be ready yet.
      }
    };

    void fetchSessions();
    const interval = window.setInterval(() => {
      void fetchSessions();
    }, 15_000);

    const unlistenSessionSync = listen<IslandSession>('island-session-sync', (event) => {
      setSessions((existing) => upsertSession(existing, event.payload));
    });

    return () => {
      active = false;
      window.clearInterval(interval);
      unlistenSessionSync.then((cleanup) => cleanup());
    };
  }, []);

  useEffect(() => {
    setPromptHistories((current) => {
      const next = syncProjectPromptHistories(projects, sessions, current);
      if (areProjectPromptHistoryMapsEqual(current, next)) {
        return current;
      }

      persistStoredPromptHistories(next);
      return next;
    });
  }, [projects, sessions]);

  useEffect(() => {
    if (!selectedProject) {
      setIsPromptHistoryOpen(false);
      setCopiedPromptId(null);
      setPendingRemovalProjectId(null);
      return;
    }

    if (pendingRemovalProjectId && pendingRemovalProjectId !== selectedProject.id) {
      setPendingRemovalProjectId(null);
    }
  }, [pendingRemovalProjectId, selectedProject]);

  const mappedOpenSessions = useMemo(() => (
    sessions
      .filter((session) => session.phase !== 'ended')
      .map((session): MappedOpenSession => {
        const cwd = normalizeProjectPath(session.terminal_context?.cwd ?? '');
        const project = projects.find((item) => doesSessionExactlyMatchProject(item.path, cwd)) ?? null;

        return {
          session,
          project,
          cwd: cwd || null,
          summary: getSessionSummary(session),
        };
      })
      .sort((left, right) => getSessionSortTimestamp(right.session) - getSessionSortTimestamp(left.session))
  ), [projects, sessions]);

  const visibleOpenSessions = useMemo(() => {
    if (!selectedProject) {
      return mappedOpenSessions;
    }

    return mappedOpenSessions.filter(({ cwd, project }) => (
      project?.id === selectedProject.id || doesSessionExactlyMatchProject(selectedProject.path, cwd)
    ));
  }, [mappedOpenSessions, selectedProject]);

  const handleAgentChange = (value: string) => {
    if (!selectedProject || !selectedProjectAgent || !isTerminalAgentId(value)) return;
    updateProjectAgent(selectedProject.id, selectedProjectAgent.id, value);
    setLaunchState({ status: 'idle', message: null });
  };

  const handleTerminalChange = (value: string) => {
    setSelectedTerminal(value);
    setLaunchState({ status: 'idle', message: null });
  };

  const handleLaunch = async () => {
    if (!selectedProject || !selectedProjectAgent) return;

    setLaunchState({ status: 'launching', message: null });

    try {
      if (selectedTerminal.id === 'embedded') {
        const launched = await invoke<EmbeddedTerminalLaunch>('launch_embedded_agent_terminal', {
          cwd: selectedProject.path,
          providerId: selectedProjectAgent.agentId,
          cols: INITIAL_TERMINAL_COLS,
          rows: INITIAL_TERMINAL_ROWS,
        });
        const nextSession: EmbeddedTerminalSession = {
          id: launched.id,
          title: `${launched.display_name} · ${selectedProject.name}`,
          cwd: launched.cwd,
          providerId: launched.provider_id,
          displayName: launched.display_name,
          status: 'running',
          output: '',
          exitCode: null,
        };
        addEmbeddedTerminalSession(nextSession);
      } else {
        await invoke('launch_agent_terminal', {
          cwd: selectedProject.path,
          providerId: selectedProjectAgent.agentId,
          terminalId: selectedTerminal.id,
        });
      }
      markProjectLaunched(selectedProject.id, selectedProjectAgent.id);
      setLaunchState({ status: 'success', message: copy.launchSuccess });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLaunchState({ status: 'error', message: `${copy.launchErrorPrefix}: ${message}` });
    }
  };

  const handleStopEmbeddedTerminal = async () => {
    if (!activeEmbeddedTerminal) return;

    try {
      await invoke('stop_embedded_terminal', { id: activeEmbeddedTerminal.id });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      markEmbeddedTerminalError(
        activeEmbeddedTerminal.id,
        `${copy.actionErrorPrefix}: ${message}`,
      );
    }
  };

  const handleCloseEmbeddedTerminal = async (id: string) => {
    const terminal = embeddedSessions.find((session) => session.id === id);
    if (terminal?.status === 'running') {
      try {
        await invoke('stop_embedded_terminal', { id });
      } catch {
        // Closing the tab should still remove the local view if the backend already exited.
      }
    }

    removeEmbeddedTerminalSession(id);
  };

  const handleJumpToSession = (sessionId: string) => {
    void (async () => {
      try {
        await invoke('jump_to_terminal', { sessionId });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setLaunchState({ status: 'error', message: `${copy.actionErrorPrefix}: ${message}` });
      }
    })();
  };

  const handleCopyPrompt = async (item: ProjectPromptHistoryItem) => {
    if (!navigator.clipboard?.writeText) {
      return;
    }

    try {
      await navigator.clipboard.writeText(item.text);
      setCopiedPromptId(item.id);
      if (promptCopyResetRef.current !== null) {
        window.clearTimeout(promptCopyResetRef.current);
      }
      promptCopyResetRef.current = window.setTimeout(() => {
        setCopiedPromptId(null);
        promptCopyResetRef.current = null;
      }, PROMPT_COPY_RESET_MS);
    } catch (error) {
      console.error('[MultiTerminal] Failed to copy prompt history item.', error);
    }
  };

  const removeSelectedProject = (project: TerminalProject) => {
    const projectId = project.id;
    removeProject(projectId);
    setPromptHistories((current) => {
      if (!(projectId in current)) {
        return current;
      }

      const next = { ...current };
      delete next[projectId];
      persistStoredPromptHistories(next);
      return next;
    });
    setPendingRemovalProjectId(null);
    setIsPromptHistoryOpen(false);
    setLaunchState({ status: 'success', message: copy.removeProjectSuccess });
  };

  const handleRequestRemoveProject = () => {
    if (!selectedProject) {
      return;
    }

    setPendingRemovalProjectId(selectedProject.id);
    setLaunchState({ status: 'idle', message: null });
  };

  const handleConfirmRemoveProject = () => {
    if (!selectedProject || pendingRemovalProjectId !== selectedProject.id) {
      return;
    }

    try {
      removeSelectedProject(selectedProject);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLaunchState({ status: 'error', message: `${copy.actionErrorPrefix}: ${message}` });
    }
  };

  return (
    <section className="multi-terminal-page page-enter">
      <div className="multi-terminal-heading">
        <div>
          <h1>{copy.title}</h1>
          <p>{copy.subtitle}</p>
        </div>
      </div>

      <div className="multi-terminal-grid">
        <section className="multi-terminal-panel multi-terminal-panel-agents">
          {selectedProject && selectedProjectAgent ? (
            <div className="multi-terminal-panel-heading multi-terminal-panel-heading-agents">
              <div className="multi-terminal-agent-launcher">
                <select
                  className="multi-terminal-agent-select"
                  aria-label={copy.agentLabel}
                  value={selectedProjectAgent.agentId}
                  onChange={(event) => handleAgentChange(event.target.value)}
                >
                  {availableAgents.map((agent) => (
                    <option key={agent.id} value={agent.id}>
                      {getDetectedAgentLabel(agent, inventory)}
                    </option>
                  ))}
                </select>
                <select
                  className="multi-terminal-agent-select"
                  aria-label={copy.terminalLabel}
                  value={selectedTerminal.id}
                  onChange={(event) => handleTerminalChange(event.target.value)}
                >
                  {availableTerminals.map((terminal) => (
                    <option key={terminal.id} value={terminal.id}>
                      {terminal.id === 'embedded' ? copy.embeddedTerminal : terminal.label}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="multi-terminal-agent-add-button"
                  onClick={() => void handleLaunch()}
                  disabled={launchState.status === 'launching' || availableAgents.length === 0}
                  title={copy.launch}
                  aria-label={copy.launch}
                >
                  +
                </button>
                <button
                  type="button"
                  className="multi-terminal-project-delete multi-terminal-project-delete-icon"
                  onClick={handleRequestRemoveProject}
                  title={copy.removeProject}
                  aria-label={copy.removeProject}
                >
                  ×
                </button>
              </div>
              {pendingRemovalProjectId === selectedProject.id ? (
                <div className="multi-terminal-project-remove-confirm" role="alert">
                  <span>{copy.removeProjectConfirmMessage(selectedProject.name)}</span>
                  <div className="multi-terminal-project-remove-confirm-actions">
                    <button
                      type="button"
                      className="multi-terminal-project-remove-cancel"
                      onClick={() => setPendingRemovalProjectId(null)}
                    >
                      {copy.removeProjectConfirmCancel}
                    </button>
                    <button
                      type="button"
                      className="multi-terminal-project-remove-confirm-button"
                      onClick={handleConfirmRemoveProject}
                    >
                      {copy.removeProjectConfirmConfirm}
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}
          <OpenAgentList
            items={visibleOpenSessions}
            selectedProject={selectedProject}
            selectedProjectAgent={selectedProjectAgent}
            onJumpToSession={handleJumpToSession}
          />
          {launchState.message ? (
            <div className={`multi-terminal-status multi-terminal-panel-status ${launchState.status}`}>
              {launchState.message}
            </div>
          ) : null}
        </section>

        <section className="multi-terminal-panel multi-terminal-panel-terminal">
          <div className="multi-terminal-terminal-heading">
            <span className="multi-terminal-heading-copy">
              <span>{copy.terminalPanelTitle}</span>
              <small title={activeEmbeddedTerminal?.cwd ?? undefined}>
                {activeEmbeddedTerminal?.cwd ?? (selectedProject ? selectedProject.path : copy.noProject)}
              </small>
            </span>
            <div className="multi-terminal-terminal-actions">
              {selectedProject ? (
                <button
                  type="button"
                  className={`multi-terminal-prompt-toggle${isPromptHistoryOpen ? ' active' : ''}`}
                  onClick={() => setIsPromptHistoryOpen((open) => !open)}
                  aria-label={isPromptHistoryOpen ? copy.promptHistoryClose : copy.promptHistoryOpen}
                  title={isPromptHistoryOpen ? copy.promptHistoryClose : copy.promptHistoryOpen}
                >
                  <span>{copy.promptHistoryToggle}</span>
                  <strong>{selectedProjectPromptHistory.length}</strong>
                </button>
              ) : null}
              {activeEmbeddedTerminal?.status === 'running' ? (
                <button
                  type="button"
                  className="multi-terminal-terminal-stop"
                  onClick={() => void handleStopEmbeddedTerminal()}
                >
                  {copy.terminalStop}
                </button>
              ) : null}
              <span className={`multi-terminal-terminal-state ${activeEmbeddedTerminal?.status ?? 'idle'}`}>
                {activeEmbeddedTerminal?.status === 'running' ? copy.runningTerminal : copy.readyTerminal}
              </span>
            </div>
          </div>
          {selectedProject && isPromptHistoryOpen ? (
            <ProjectPromptHistoryPanel
              language={language}
              copy={copy}
              project={selectedProject}
              items={selectedProjectPromptHistory}
              copiedPromptId={copiedPromptId}
              onCopyPrompt={(item) => {
                void handleCopyPrompt(item);
              }}
              onClose={() => setIsPromptHistoryOpen(false)}
            />
          ) : null}
          {visibleEmbeddedSessions.length > 0 ? (
            <div className="multi-terminal-terminal-tabs" role="tablist" aria-label={copy.terminalPanelTitle}>
              {visibleEmbeddedSessions.map((session) => (
                <div
                  key={session.id}
                  role="tab"
                  tabIndex={0}
                  aria-selected={session.id === activeEmbeddedTerminal?.id}
                  className={`multi-terminal-terminal-tab ${
                    session.id === activeEmbeddedTerminal?.id ? 'active' : ''
                  } ${session.status}`}
                  onClick={() => setActiveEmbeddedTerminal(session.id)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter' || event.key === ' ') {
                      event.preventDefault();
                      setActiveEmbeddedTerminal(session.id);
                    }
                  }}
                  title={session.cwd}
                >
                  <span>{session.title}</span>
                  <small>{session.status === 'running' ? copy.runningTerminal : copy.readyTerminal}</small>
                  <button
                    type="button"
                    className="multi-terminal-terminal-tab-close"
                    aria-label={copy.terminalStop}
                    onClick={(event) => {
                      event.stopPropagation();
                      void handleCloseEmbeddedTerminal(session.id);
                    }}
                  >
                    ×
                  </button>
                </div>
              ))}
            </div>
          ) : null}
          <div className="multi-terminal-terminal-body">
            {activeEmbeddedTerminal ? (
              <EmbeddedTerminalPane
                key={activeEmbeddedTerminal.id}
                session={activeEmbeddedTerminal}
              />
            ) : (
              <div className="multi-terminal-empty multi-terminal-terminal-empty">
                <span>{selectedProject ? copy.terminalPanelIdle : copy.noProject}</span>
                <p>{selectedProject ? copy.terminalPanelIdleDescription : copy.noProjectSelectedDescription}</p>
              </div>
            )}
          </div>
        </section>
      </div>
    </section>
  );
}
