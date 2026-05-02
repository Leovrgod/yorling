import { useEffect, useState } from 'react';
import type { IslandSession } from '../../types';
import { getPrimaryIslandSession } from '../sessionQueue';
import { formatProviderLabel, getCollapsedTickerItems } from '../presentation';
import { MascotView } from './MascotView';
import { MorphText } from './MorphText';
import { t } from '../i18n';

interface CollapsedBarProps {
  sessions: IslandSession[];
  onClick: () => void;
}

/**
 * Collapsed state: a compact pill showing session status dots.
 */
export function CollapsedBar({ sessions, onClick }: CollapsedBarProps) {
  const primarySession = getPrimaryIslandSession(sessions);
  const tickerItems = getCollapsedTickerItems(sessions);
  const tickerKey = tickerItems.map((item) => item.id).join('|');
  const [messageIndex, setMessageIndex] = useState(0);

  useEffect(() => {
    setMessageIndex(0);
  }, [tickerKey]);

  useEffect(() => {
    if (tickerItems.length <= 1) {
      return;
    }

    const timer = window.setInterval(() => {
      setMessageIndex((current) => (current + 1) % tickerItems.length);
    }, 2400);

    return () => window.clearInterval(timer);
  }, [tickerItems.length, tickerKey]);

  const activeTickerItem = tickerItems[messageIndex] ?? null;
  const displaySession = activeTickerItem
    ? sessions.find((session) => session.id === activeTickerItem.sessionId) ?? primarySession
    : primarySession;
  const tone = displaySession ? getSessionTone(displaySession) : 'idle';
  const providerLabel = displaySession ? formatProviderLabel(displaySession.provider_id) : 'Y';
  const providerInitial = providerLabel.charAt(0).toUpperCase();
  const message = activeTickerItem?.text ?? t('collapsed.dynamic_island');
  const messageClassName = [
    'island-collapsed__message',
    activeTickerItem?.isHumanized ? 'island-collapsed__message--humanized' : '',
    activeTickerItem ? `island-collapsed__message--${activeTickerItem.accentToken}` : '',
  ].filter(Boolean).join(' ');
  const attentionCount = sessions.filter(
    (session) =>
      session.phase === 'waitingforapproval' || session.phase === 'waitingforanswer',
  ).length;

  return (
    <button
      type="button"
      className={`island-collapsed island-collapsed--${tone}`}
      onClick={onClick}
    >
      <span className={`island-collapsed__leading island-collapsed__leading--${tone}`}>
        <span className="island-collapsed__leading-core">
          {displaySession ? (
            <MascotView
              providerId={displaySession.provider_id}
              phase={displaySession.phase}
              size={24}
              motion={tone === 'attention'}
            />
          ) : (
            providerInitial
          )}
        </span>
      </span>
      <span className="island-collapsed__center">
        <MorphText text={message} className={messageClassName} />
      </span>
      <span className="island-collapsed__trailing">
        {attentionCount > 0 ? (
          <span className="island-collapsed__alert" aria-hidden="true">
            <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
              <path
                d="M6 2.25C4.48122 2.25 3.25 3.48122 3.25 5V6.3125C3.25 6.76503 3.0708 7.19903 2.75143 7.5184L2.25 8.01984V8.75H9.75V8.01984L9.24857 7.5184C8.9292 7.19903 8.75 6.76503 8.75 6.3125V5C8.75 3.48122 7.51878 2.25 6 2.25Z"
                stroke="currentColor"
                strokeWidth="1"
                strokeLinejoin="round"
              />
              <path d="M4.75 9.25C4.9375 9.83333 5.35417 10.125 6 10.125C6.64583 10.125 7.0625 9.83333 7.25 9.25" stroke="currentColor" strokeWidth="1" strokeLinecap="round"/>
            </svg>
          </span>
        ) : sessions.length > 0 ? (
          <span className="island-collapsed__count">{sessions.length}</span>
        ) : (
          <span className="island-collapsed__placeholder" aria-hidden="true" />
        )}
      </span>
    </button>
  );
}

function getSessionTone(session: IslandSession): 'idle' | 'busy' | 'attention' | 'compacting' {
  switch (session.phase) {
    case 'waitingforapproval':
    case 'waitingforanswer':
      return 'attention';
    case 'compacting':
      return 'compacting';
    case 'processing':
      return 'busy';
    default:
      return 'idle';
  }
}
