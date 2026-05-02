import { useState, useRef, useEffect, useCallback } from 'react';
import type { IslandSession } from '../../types';
import { MascotView } from './MascotView';
import { PermissionCard } from './PermissionCard';
import { QuestionCard } from './QuestionCard';
import { useIslandActions } from '../hooks/useIslandState';
import { t } from '../i18n';
import { formatProviderLabel, getPhaseLabel, getVisiblePromptMessages } from '../presentation';

interface ChatViewProps {
  session: IslandSession;
  onBack: () => void;
}

const AUTOSCROLL_THRESHOLD = 50;

/**
 * Detailed per-session drill-down view showing chat history,
 * tool calls, and session metadata.
 */
export function ChatView({ session, onBack }: ChatViewProps) {
  const { jumpToTerminal } = useIslandActions();
  const promptMessages = getVisiblePromptMessages(session);
  const providerLabel = formatProviderLabel(session.provider_id);
  const activityCount =
    promptMessages.length
    + (session.pending_permission ? 1 : 0)
    + (session.pending_question ? 1 : 0);

  const scrollRef = useRef<HTMLDivElement>(null);
  const copyResetRef = useRef<number | null>(null);
  const [isAutoscrollPaused, setIsAutoscrollPaused] = useState(false);
  const [newMessageCount, setNewMessageCount] = useState(0);
  const [copiedMessageKey, setCopiedMessageKey] = useState<string | null>(null);
  const prevActivityCountRef = useRef(activityCount);

  useEffect(() => {
    prevActivityCountRef.current = activityCount;
    setIsAutoscrollPaused(false);
    setNewMessageCount(0);
    setCopiedMessageKey(null);

    const frameId = window.requestAnimationFrame(() => {
      const el = scrollRef.current;
      if (!el) {
        return;
      }

      el.scrollTop = el.scrollHeight;
    });

    return () => window.cancelAnimationFrame(frameId);
  }, [session.id]);

  useEffect(() => {
    const prevCount = prevActivityCountRef.current;
    const currentCount = activityCount;

    if (currentCount > prevCount) {
      const added = currentCount - prevCount;
      if (isAutoscrollPaused) {
        setNewMessageCount((n) => n + added);
      } else {
        const el = scrollRef.current;
        if (el) {
          el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
        }
      }
    }

    prevActivityCountRef.current = currentCount;
  }, [activityCount, isAutoscrollPaused]);

  useEffect(() => () => {
    if (copyResetRef.current !== null) {
      window.clearTimeout(copyResetRef.current);
    }
  }, []);

  const handleScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.clientHeight - el.scrollTop;
    const atBottom = distanceFromBottom < AUTOSCROLL_THRESHOLD;

    if (!atBottom && !isAutoscrollPaused) {
      setIsAutoscrollPaused(true);
    } else if (atBottom && isAutoscrollPaused) {
      setIsAutoscrollPaused(false);
      setNewMessageCount(0);
    }
  }, [isAutoscrollPaused]);

  const scrollToBottom = useCallback(() => {
    const el = scrollRef.current;
    if (el) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });
    }
    setIsAutoscrollPaused(false);
    setNewMessageCount(0);
  }, []);

  const handleCopyPrompt = useCallback(async (messageKey: string, text: string) => {
    if (!navigator.clipboard?.writeText) {
      return;
    }

    try {
      await navigator.clipboard.writeText(text);
      setCopiedMessageKey(messageKey);
      if (copyResetRef.current !== null) {
        window.clearTimeout(copyResetRef.current);
      }
      copyResetRef.current = window.setTimeout(() => {
        setCopiedMessageKey(null);
        copyResetRef.current = null;
      }, 1600);
    } catch (error) {
      console.error('[Island] Failed to copy prompt:', error);
    }
  }, []);

  return (
    <div className="island-chatview">
      <div className="island-chatview__header">
        <button
          type="button"
          className="island-chatview__back"
          onClick={onBack}
          aria-label={t('chatview.back')}
          title={t('chatview.back')}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
            <path d="M10 3L5 8L10 13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
        <div className="island-chatview__header-info">
          <MascotView providerId={session.provider_id} phase={session.phase} size={20} />
          <span className="island-chatview__provider">{providerLabel}</span>
          <span className={`island-chatview__phase island-chatview__phase--${session.phase}`}>
            {getPhaseLabel(session.phase)}
          </span>
        </div>
        <button
          type="button"
          className="island-chatview__jump-btn"
          onClick={() => jumpToTerminal(session.id)}
          aria-label={t('session.jump')}
          title={t('session.jump')}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M4 10L10 4M10 4H5M10 4V9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
        </button>
      </div>

      <div
        ref={scrollRef}
        className="island-chatview__body"
        onScroll={handleScroll}
      >
        {promptMessages.length > 0 ? (
          <div className="island-chatview__body-item island-chatview__stack">
            <div className="island-chatview__section-label">{t('chatview.prompts')}</div>
            <div className="island-chatview__messages">
              {promptMessages.map((msg, i) => {
                const messageKey = `${msg.role}-${msg.timestamp}-${i}`;
                const copied = copiedMessageKey === messageKey;

                return (
                <div key={messageKey} className="island-chatview__msg island-chatview__msg--user">
                  <div className="island-chatview__msg-meta">
                    <span className="island-chatview__msg-role">{t('role.you')}</span>
                    <button
                      type="button"
                      className={`island-chatview__msg-copy${copied ? ' island-chatview__msg-copy--copied' : ''}`}
                      onClick={() => {
                        void handleCopyPrompt(messageKey, msg.text);
                      }}
                      aria-label={copied ? t('chatview.copied') : t('chatview.copy')}
                      title={copied ? t('chatview.copied') : t('chatview.copy')}
                    >
                      {copied ? t('chatview.copied') : t('chatview.copy')}
                    </button>
                  </div>
                  <span className="island-chatview__msg-text">{msg.text}</span>
                </div>
                );
              })}
            </div>
          </div>
        ) : null}

        {(session.pending_permission || session.pending_question) && (
          <div className="island-chatview__body-item island-chatview__stack">
            <div className="island-chatview__section-label">{t('chatview.decisions')}</div>
            {session.pending_permission && <PermissionCard permission={session.pending_permission} />}
            {session.pending_question && <QuestionCard question={session.pending_question} sessionId={session.id} />}
          </div>
        )}

        {promptMessages.length === 0 && !session.pending_permission && !session.pending_question && (
          <div className="island-chatview__body-item island-chatview__empty">
            {t('chatview.empty')}
          </div>
        )}
      </div>

      {/* New messages indicator */}
      {isAutoscrollPaused && newMessageCount > 0 && (
        <button
          type="button"
          className="island-chatview__new-msgs"
          onClick={scrollToBottom}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
            <path d="M2 4L5 7L8 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
          <span>
            {newMessageCount === 1
              ? t('chatview.new_message', { n: newMessageCount })
              : t('chatview.new_messages', { n: newMessageCount })}
          </span>
        </button>
      )}
    </div>
  );
}
