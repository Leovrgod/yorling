import { useEffect, useRef } from 'react';
import type { IslandSession } from '../../types';
import type { IslandOpenReason } from '../hooks/useIslandAnimation';
import { t } from '../i18n';
import { getSessionListTitle } from '../presentation';
import { getLatestAttentionSessionId, sortIslandSessionsForList } from '../sessionQueue';
import { useIslandStore } from '../store/islandStore';
import { SessionCard } from './SessionCard';

interface SessionListProps {
  sessions: IslandSession[];
  onCollapse: () => void;
  onSelectSession?: (sessionId: string) => void;
  onShowRules?: () => void;
  openReason?: IslandOpenReason;
}

/**
 * Expanded view: list of all active sessions.
 */
export function SessionList({
  sessions,
  onCollapse,
  onSelectSession,
  onShowRules,
  openReason = null,
}: SessionListProps) {
  const orderedSessions = sortIslandSessionsForList(sessions);
  const latestAttentionSessionId = getLatestAttentionSessionId(orderedSessions);
  const plugins = useIslandStore((state) => state.plugins);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (openReason !== 'attention') {
      return;
    }

    const body = bodyRef.current;
    if (!body) {
      return;
    }

    const scrollToLatest = () => {
      body.scrollTop = body.scrollHeight;
    };

    scrollToLatest();
    const frameId = window.requestAnimationFrame(scrollToLatest);
    return () => window.cancelAnimationFrame(frameId);
  }, [latestAttentionSessionId, openReason, orderedSessions.length]);

  return (
    <div className="island-session-list">
      <div className="island-session-list__header">
        <div className="island-session-list__titles">
          <span className="island-session-list__title">{getSessionListTitle(sessions.length)}</span>
        </div>
        <div className="island-session-list__actions">
          {onShowRules && (
            <button
              type="button"
              className="island-session-list__rules-btn"
              onClick={onShowRules}
              aria-label={t('sessions.rules')}
              title={t('sessions.rules')}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
                <circle cx="7" cy="7" r="2.5" stroke="currentColor" strokeWidth="1.2"/>
                <path d="M7 1V3M7 11V13M1 7H3M11 7H13M2.8 2.8L4.2 4.2M9.8 9.8L11.2 11.2M11.2 2.8L9.8 4.2M4.2 9.8L2.8 11.2" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round"/>
              </svg>
            </button>
          )}
          <button
            type="button"
            className="island-session-list__close"
            onClick={onCollapse}
            aria-label={t('sessions.collapse')}
            title={t('sessions.collapse')}
          >
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path d="M3 3L11 11M11 3L3 11" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
            </svg>
          </button>
        </div>
      </div>
      <div className="island-session-list__body" ref={bodyRef}>
        {sessions.length === 0 ? (
          <div className="island-session-list__empty">{t('sessions.no_active')}</div>
        ) : (
          orderedSessions.map((session) => (
            <SessionCard
              key={session.id}
              session={session}
              plugins={plugins}
              onSelect={onSelectSession ? () => onSelectSession(session.id) : undefined}
            />
          ))
        )}
      </div>
    </div>
  );
}
