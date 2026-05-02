import {
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import { getSelectedTerminalProjectAgent, useTerminalStore } from '../../stores/terminalStore';
import type { ModuleName } from '../../types';
import { getTerminalAgent } from '../terminals/agentCatalog';
import { pickProjectDirectory } from '../terminals/projectPicker';

interface SidebarProps {
  activeModule: ModuleName;
  onModuleChange: (module: ModuleName) => void;
  windowControls: ReactNode;
}

const modules: { id: ModuleName; icon: string; hasBadge?: boolean }[] = [
  { id: 'keyboard', icon: '⌨' },
  { id: 'music', icon: '♪' },
  { id: 'superRightClick', icon: '⌁' },
  { id: 'clipboard', icon: '▥' },
  { id: 'island', icon: '◆' },
  { id: 'terminals', icon: '▣' },
];

const SIDEBAR_MIN_WIDTH = 120;
const SIDEBAR_MAX_WIDTH = 360;

function clampSidebarWidth(width: number): number {
  return Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, Math.round(width)));
}

export function SidebarDivider() {
  const language = useAppStore((state) => state.language);
  const collapsed = useAppStore((state) => state.sidebarCollapsed);
  const sidebarWidth = useAppStore((state) => state.sidebarWidth);
  const setSidebarCollapsed = useAppStore((state) => state.setSidebarCollapsed);
  const setSidebarWidth = useAppStore((state) => state.setSidebarWidth);
  const [isDragging, setIsDragging] = useState(false);
  const dragStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const pendingWidthRef = useRef<number | null>(null);
  const frameRef = useRef<number | null>(null);

  useEffect(() => {
    const handlePointerMove = (event: PointerEvent) => {
      const dragState = dragStateRef.current;

      if (!dragState) {
        return;
      }

      const nextWidth = clampSidebarWidth(dragState.startWidth + event.clientX - dragState.startX);
      pendingWidthRef.current = nextWidth;

      if (frameRef.current !== null) {
        return;
      }

      frameRef.current = window.requestAnimationFrame(() => {
        frameRef.current = null;

        if (pendingWidthRef.current !== null) {
          setSidebarWidth(pendingWidthRef.current, false);
        }
      });
    };

    const stopDragging = () => {
      if (!dragStateRef.current) {
        return;
      }

      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
        frameRef.current = null;
      }

      const finalWidth = pendingWidthRef.current ?? useAppStore.getState().sidebarWidth;
      pendingWidthRef.current = null;
      setSidebarWidth(finalWidth);
      dragStateRef.current = null;
      setIsDragging(false);
      document.body.classList.remove('sidebar-resizing');
    };

    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', stopDragging);
    window.addEventListener('pointercancel', stopDragging);

    return () => {
      window.removeEventListener('pointermove', handlePointerMove);
      window.removeEventListener('pointerup', stopDragging);
      window.removeEventListener('pointercancel', stopDragging);
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
      }
      document.body.classList.remove('sidebar-resizing');
    };
  }, [setSidebarWidth]);

  const toggleLabel = collapsed
    ? (language === 'zh' ? '展开侧栏' : 'Show sidebar')
    : (language === 'zh' ? '收起侧栏' : 'Hide sidebar');
  const resizeLabel = language === 'zh' ? '拖动调整侧栏宽度' : 'Drag to resize sidebar';

  const handleResizeStart = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }

    if ((event.target as HTMLElement).closest('.sidebar-divider-toggle')) {
      return;
    }

    const startWidth = collapsed ? SIDEBAR_MIN_WIDTH : clampSidebarWidth(sidebarWidth);

    if (collapsed) {
      setSidebarCollapsed(false);
      setSidebarWidth(startWidth);
    }

    dragStateRef.current = {
      startX: event.clientX,
      startWidth,
    };
    setIsDragging(true);
    document.body.classList.add('sidebar-resizing');
    event.preventDefault();
  };

  return (
    <div
      className={`sidebar-divider ${collapsed ? 'sidebar-divider-collapsed' : ''} ${isDragging ? 'dragging' : ''}`}
      onPointerDown={handleResizeStart}
      role="separator"
      aria-orientation="vertical"
      aria-label={resizeLabel}
    >
      <button
        type="button"
        className="sidebar-toggle-btn sidebar-divider-toggle"
        onPointerDown={(event) => event.stopPropagation()}
        onClick={() => setSidebarCollapsed(!collapsed)}
        aria-label={toggleLabel}
        title={toggleLabel}
      >
        <span className={`sidebar-toggle-icon ${collapsed ? 'collapsed' : ''}`} aria-hidden="true" />
      </button>
    </div>
  );
}

export function Sidebar({
  activeModule,
  onModuleChange,
  windowControls,
}: SidebarProps) {
  const language = useAppStore((state) => state.language);
  const collapsed = useAppStore((state) => state.sidebarCollapsed);
  const sidebarWidth = useAppStore((state) => state.sidebarWidth);
  const terminalProjects = useTerminalStore((state) => state.projects);
  const selectedTerminalProjectId = useTerminalStore((state) => state.selectedProjectId);
  const terminalExpanded = useTerminalStore((state) => state.multiTerminalExpanded);
  const setTerminalExpanded = useTerminalStore((state) => state.setMultiTerminalExpanded);
  const addTerminalProject = useTerminalStore((state) => state.addProject);
  const selectTerminalProject = useTerminalStore((state) => state.selectProject);
  const copy = getUiCopy(language);

  const handleAddTerminalProject = () => {
    void (async () => {
      try {
        const path = await pickProjectDirectory({
          title: copy.terminals.directoryPickerTitle,
        });
        if (!path) return;

        const projectId = addTerminalProject(path);
        if (!projectId) return;

        selectTerminalProject(projectId);
        setTerminalExpanded(true);
        onModuleChange('terminals');
      } catch (error) {
        console.error('Failed to open the native project picker.', error);
      }
    })();
  };

  return (
    <aside
      className={`sidebar ${collapsed ? 'sidebar-collapsed' : ''}`}
      aria-hidden={collapsed}
      style={{ width: collapsed ? 0 : clampSidebarWidth(sidebarWidth) }}
    >
      <div className="sidebar-window-strip" data-tauri-drag-region>
        <div className="sidebar-window-leading" data-no-window-drag>
          {windowControls}
        </div>
      </div>

      <nav className="sidebar-nav">
        {modules.map((mod) => {
          if (mod.id === 'terminals') {
            return (
              <div key={mod.id} className="sidebar-terminal-group">
                <button
                  type="button"
                  className={`sidebar-item sidebar-item-disclosure ${activeModule === mod.id ? 'active' : ''}`}
                  onClick={() => {
                    selectTerminalProject(null);
                    onModuleChange(mod.id);
                    setTerminalExpanded(activeModule === mod.id ? !terminalExpanded : true);
                  }}
                  aria-current={activeModule === mod.id ? 'page' : undefined}
                  aria-expanded={terminalExpanded}
                  tabIndex={collapsed ? -1 : 0}
                  title={terminalExpanded ? copy.terminals.collapse : copy.terminals.expand}
                >
                  <span className="item-icon">{mod.icon}</span>
                  <span>{copy.sidebar.modules[mod.id]}</span>
                  <span className={`sidebar-disclosure ${terminalExpanded ? 'expanded' : ''}`} aria-hidden="true" />
                </button>

                {terminalExpanded ? (
                  <div className="sidebar-terminal-projects">
                    {terminalProjects.map((project) => {
                      const agent = getTerminalAgent(
                        getSelectedTerminalProjectAgent(project)?.agentId ?? project.agents[0]?.agentId ?? 'codex',
                      );

                      return (
                        <button
                          key={project.id}
                          type="button"
                          className={`sidebar-terminal-project ${project.id === selectedTerminalProjectId ? 'active' : ''}`}
                          onClick={() => {
                            selectTerminalProject(project.id);
                            onModuleChange('terminals');
                          }}
                          tabIndex={collapsed ? -1 : 0}
                          title={project.path}
                          data-path={project.path}
                        >
                          <span className="sidebar-terminal-project-icon" aria-hidden="true" />
                          <span className="sidebar-terminal-project-copy">
                            <span className="sidebar-terminal-project-name">{project.name}</span>
                            <span className="sidebar-terminal-project-agent">{agent.label}</span>
                          </span>
                          <span className="sidebar-terminal-project-tooltip" role="tooltip">
                            {project.path}
                          </span>
                        </button>
                      );
                    })}
                    <button
                      type="button"
                      className="sidebar-terminal-add"
                      onClick={handleAddTerminalProject}
                      tabIndex={collapsed ? -1 : 0}
                    >
                      <span aria-hidden="true">+</span>
                      <span>{copy.terminals.addProject}</span>
                    </button>
                  </div>
                ) : null}
              </div>
            );
          }

          return (
            <button
              key={mod.id}
              type="button"
              className={`sidebar-item ${activeModule === mod.id ? 'active' : ''}`}
              onClick={() => onModuleChange(mod.id)}
              aria-current={activeModule === mod.id ? 'page' : undefined}
              tabIndex={collapsed ? -1 : 0}
            >
              <span className="item-icon">{mod.icon}</span>
              <span>{copy.sidebar.modules[mod.id]}</span>
              {mod.hasBadge ? <span className="item-badge">{copy.sidebar.soonBadge}</span> : null}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
