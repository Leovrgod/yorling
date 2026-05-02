import { useEffect, useState, useRef, useCallback } from 'react';
import type { IslandNotification } from '../store/islandStore';

interface CompletionToastProps {
  notifications: IslandNotification[];
  onDismiss: (id: string) => void;
  onOpenSession: (sessionId: string) => void;
}

const TOAST_DURATION = 8000;

/**
 * Notification queue toast: shows one notification at a time with auto-dismiss,
 * hover pauses the timer, click to dismiss early.
 */
export function CompletionToast({
  notifications,
  onDismiss,
  onOpenSession,
}: CompletionToastProps) {
  const [currentIndex, setCurrentIndex] = useState(0);
  const [visible, setVisible] = useState(false);
  const [hovering, setHovering] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const playedIdsRef = useRef<Set<string>>(new Set());

  const current = notifications[currentIndex] ?? null;
  const remaining = notifications.length - currentIndex - 1;

  const dismissCurrent = useCallback(() => {
    if (!current) return;
    setVisible(false);
    setTimeout(() => {
      onDismiss(current.id);
      setCurrentIndex((i) => i + 1);
    }, 300);
  }, [current, onDismiss]);

  const openCurrentSession = useCallback(() => {
    if (!current) return;
    onOpenSession(current.sessionId);
    dismissCurrent();
  }, [current, dismissCurrent, onOpenSession]);

  // Start auto-dismiss timer (paused on hover)
  useEffect(() => {
    if (!current || !visible || hovering) {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
      return;
    }

    timerRef.current = setTimeout(dismissCurrent, TOAST_DURATION);

    return () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
      }
    };
  }, [current, visible, hovering, dismissCurrent]);

  // Animate in when a new notification appears
  useEffect(() => {
    if (!current) return;
    if (playedIdsRef.current.has(current.id)) return;
    playedIdsRef.current.add(current.id);

    requestAnimationFrame(() => setVisible(true));
  }, [current]);

  // Reset index when notifications array changes significantly
  useEffect(() => {
    if (currentIndex >= notifications.length) {
      setCurrentIndex(Math.max(0, notifications.length - 1));
    }
  }, [notifications.length, currentIndex]);

  if (!current) return null;

  return (
    <div
      className={'island-toast' + (visible ? ' island-toast--visible' : '')}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      role="alertdialog"
      aria-label="Completed session"
    >
      <div className="island-toast__content">
        <div className="island-toast__title">{current.title}</div>
        {current.body && (
          <div className="island-toast__body">{current.body}</div>
        )}
        <div className="island-toast__actions">
          <button
            type="button"
            className="island-toast__action island-toast__action--primary"
            onClick={openCurrentSession}
          >
            Review
          </button>
          <button
            type="button"
            className="island-toast__action island-toast__action--ghost"
            onClick={dismissCurrent}
          >
            Dismiss
          </button>
        </div>
      </div>
      {remaining > 0 && (
        <span className="island-toast__queue-badge">+{remaining}</span>
      )}
    </div>
  );
}
