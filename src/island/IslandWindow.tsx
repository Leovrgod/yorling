import { useEffect, useState, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { LANGUAGE_STORAGE_KEY } from '../i18n/copy';
import { getIslandLang, setIslandLang, syncIslandLangFromStorage } from './i18n';
import { useIslandStore } from './store/islandStore';
import { useIslandState } from './hooks/useIslandState';
import { IslandContent } from './IslandContent';
import { filterVisibleIslandSessions, isAttentionSession } from './sessionQueue';

type VisibilityMode = 'visible' | 'hidden' | 'edge';

const EDGE_HOVER_DELAY = 300;
const EDGE_LEAVE_DELAY = 500;
const EDGE_ZONE_HEIGHT = 8;

/**
 * Root component for the island overlay window.
 * This renders in a separate Tauri window (label="island").
 */
export function IslandWindow() {
  useIslandState();
  const [language, setLanguage] = useState(() => getIslandLang());
  const [mode, setMode] = useState<VisibilityMode>('visible');
  const [edgeRevealed, setEdgeRevealed] = useState(false);
  const hoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const leaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', 'dark');
    document.body.classList.add('island-overlay-window');

    const syncLanguage = () => {
      const nextLanguage = syncIslandLangFromStorage(window.localStorage);
      setIslandLang(nextLanguage);
      setLanguage(nextLanguage);
      document.documentElement.lang = nextLanguage;
    };

    syncLanguage();

    const handleStorage = (event: StorageEvent) => {
      if (event.key && event.key !== LANGUAGE_STORAGE_KEY) {
        return;
      }

      syncLanguage();
    };

    window.addEventListener('storage', handleStorage);

    // Listen for fullscreen visibility events from Rust
    // Payload: "visible" | "hidden" | "edge" (or legacy boolean)
    const unlisten = listen<string | boolean>('island-visibility', (event) => {
      const payload = event.payload;
      if (typeof payload === 'boolean') {
        setMode(payload ? 'visible' : 'hidden');
      } else {
        setMode(payload as VisibilityMode);
      }
      setEdgeRevealed(false);
    });

    return () => {
      document.body.classList.remove('island-overlay-window');
      window.removeEventListener('storage', handleStorage);
      unlisten.then((fn) => fn());
    };
  }, []);

  // Edge reveal: track mouse at top edge when in edge mode
  const handleEdgeMouseMove = useCallback(
    (e: MouseEvent) => {
      if (mode !== 'edge') return;

      const inEdgeZone = e.clientY <= EDGE_ZONE_HEIGHT;

      if (inEdgeZone && !edgeRevealed) {
        if (hoverTimerRef.current === null) {
          hoverTimerRef.current = setTimeout(() => {
            setEdgeRevealed(true);
            hoverTimerRef.current = null;
          }, EDGE_HOVER_DELAY);
        }
        // Clear leave timer if re-entering
        if (leaveTimerRef.current !== null) {
          clearTimeout(leaveTimerRef.current);
          leaveTimerRef.current = null;
        }
      } else if (!inEdgeZone) {
        // Cancel hover timer
        if (hoverTimerRef.current !== null) {
          clearTimeout(hoverTimerRef.current);
          hoverTimerRef.current = null;
        }
        // Start leave timer
        if (edgeRevealed && leaveTimerRef.current === null) {
          leaveTimerRef.current = setTimeout(() => {
            setEdgeRevealed(false);
            leaveTimerRef.current = null;
          }, EDGE_LEAVE_DELAY);
        }
      }
    },
    [mode, edgeRevealed],
  );

  useEffect(() => {
    if (mode === 'edge') {
      window.addEventListener('mousemove', handleEdgeMouseMove);
      return () => {
        window.removeEventListener('mousemove', handleEdgeMouseMove);
        if (hoverTimerRef.current !== null) clearTimeout(hoverTimerRef.current);
        if (leaveTimerRef.current !== null) clearTimeout(leaveTimerRef.current);
      };
    }
  }, [mode, handleEdgeMouseMove]);

  const sessions = useIslandStore((s) => s.sessions);
  const viewMode = useIslandStore((s) => s.viewMode);
  const activeSessions = filterVisibleIslandSessions(sessions);
  const hasAttentionSession = activeSessions.some(isAttentionSession);

  const isVisible = hasAttentionSession || mode === 'visible' || (mode === 'edge' && edgeRevealed);

  useEffect(() => {
    const passthrough = !isVisible || viewMode === 'collapsed';

    void invoke('set_island_mouse_passthrough', { passthrough }).catch(() => {
      // Native passthrough is best-effort and macOS specific.
    });
  }, [isVisible, viewMode]);

  return (
    <div
      className="island-window"
      data-lang={language}
      style={{
        opacity: isVisible ? 1 : 0,
        pointerEvents: isVisible ? 'auto' : 'none',
        transition: 'opacity 0.3s ease',
      }}
    >
      <IslandContent sessions={activeSessions} />
    </div>
  );
}
