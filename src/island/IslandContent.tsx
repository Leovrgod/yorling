import { useCallback, useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { IslandSession } from '../types';
import { useIslandAnimation } from './hooks/useIslandAnimation';
import { useIslandStore } from './store/islandStore';
import { useIslandActions } from './hooks/useIslandState';
import { NotchShape } from './NotchShape';
import { CollapsedBar } from './components/CollapsedBar';
import { SessionList } from './components/SessionList';
import { ChatView } from './components/ChatView';
import { ApprovalRulesPanel } from './components/ApprovalRulesPanel';
import { CompletionToast } from './components/CompletionToast';
import { getIslandSurfaceMetrics } from './notchGeometry';
import { filterCollapsedIslandSessions, isAttentionSession } from './sessionQueue';
import {
  promoteIslandOpenReasonForInternalInteraction,
  resolveExpandedPanelHeight,
  shouldAutoCollapseExpandedIsland,
  shouldUseDomMouseLeave,
} from './panelState';

interface IslandContentProps {
  sessions: IslandSession[];
}

type ExpandedView = 'sessions' | 'chatview' | 'rules';
type ContentPhase = 'idle' | 'appearing' | 'transitioning';
type ContentTransitionDirection = 'forward' | 'back';
type ExpandedViewSnapshot = {
  view: ExpandedView;
  sessionId: string | null;
  key: string;
};

/** Hermite smoothstep for natural crossfade easing */
function smoothstep(edge0: number, edge1: number, x: number): number {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

export function IslandContent({ sessions }: IslandContentProps) {
  const {
    viewMode,
    contentMode,
    progress,
    isAnimating,
    isBouncing,
    openReason,
    toggle,
    collapse,
    expand,
    setOpenReason,
    handleMouseEnter,
    handleMouseLeave,
  } = useIslandAnimation();
  const notifications = useIslandStore((s) => s.notifications);
  const screenInfo = useIslandStore((s) => s.screenInfo);
  const selectedSessionId = useIslandStore((s) => s.selectedSessionId);
  const setSelectedSessionId = useIslandStore((s) => s.setSelectedSessionId);
  const removeNotification = useIslandStore((s) => s.removeNotification);
  const { approvePermission } = useIslandActions();
  const previousAttentionCount = useRef(0);
  const expandedContentRef = useRef<HTMLDivElement | null>(null);
  const activeExpandedSurfaceRef = useRef<HTMLDivElement | null>(null);
  const notchRef = useRef<HTMLDivElement | null>(null);
  const expandedLeaveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastPointerPositionRef = useRef({ x: 0, y: 0 });
  const previousNotchRectRef = useRef<DOMRect | null>(null);
  const transitionInteractionGuardUntilRef = useRef(0);
  const contentTransitionTimersRef = useRef<number[]>([]);
  const previousContentModeRef = useRef(contentMode);
  const [expandedView, setExpandedView] = useState<ExpandedView>('sessions');
  const [contentPhase, setContentPhase] = useState<ContentPhase>('idle');
  const [contentTransitionDirection, setContentTransitionDirection] =
    useState<ContentTransitionDirection>('forward');
  const [transitioningFrom, setTransitioningFrom] = useState<ExpandedViewSnapshot | null>(null);
  const [measuredExpandedHeight, setMeasuredExpandedHeight] = useState<number | null>(null);
  const [smoothedExpandedHeight, setSmoothedExpandedHeight] = useState<number | null>(null);
  const smoothedHeightRef = useRef<number | null>(null);
  const heightAnimationRef = useRef<number | null>(null);
  const interactionMetrics = getIslandSurfaceMetrics({
    progress,
    sessionCount: sessions.length,
    screenInfo,
    openReason,
    panelMode: expandedView,
    measuredExpandedHeight: smoothedExpandedHeight ?? measuredExpandedHeight ?? estimateExpandedHeight({
      sessions,
      openReason,
      expandedView,
      selectedSessionId,
    }),
  });

  const attentionCount = sessions.filter(
    isAttentionSession,
  ).length;
  const hasAttentionRequest = attentionCount > 0;
  const collapsedSessions = filterCollapsedIslandSessions(sessions);

  const clearContentTransitionTimers = useCallback(() => {
    for (const timerId of contentTransitionTimersRef.current) {
      window.clearTimeout(timerId);
    }

    contentTransitionTimersRef.current = [];
  }, []);

  const clearExpandedLeaveTimer = useCallback(() => {
    if (expandedLeaveTimerRef.current !== null) {
      clearTimeout(expandedLeaveTimerRef.current);
      expandedLeaveTimerRef.current = null;
    }
  }, []);

  const runExpandedViewTransition = useCallback((options: {
    nextView: ExpandedView;
    nextSessionId: string | null;
    direction: ContentTransitionDirection;
  }) => {
    const currentSnapshot = createExpandedViewSnapshot(expandedView, selectedSessionId);
    const nextSnapshot = createExpandedViewSnapshot(options.nextView, options.nextSessionId);
    if (currentSnapshot.key === nextSnapshot.key) {
      return;
    }

    clearContentTransitionTimers();
    previousNotchRectRef.current = notchRef.current?.getBoundingClientRect() ?? null;
    transitionInteractionGuardUntilRef.current = Date.now() + 420;
    setContentTransitionDirection(options.direction);
    setTransitioningFrom(currentSnapshot);
    setSelectedSessionId(options.nextSessionId);
    setExpandedView(options.nextView);
    setContentPhase('transitioning');

    const settleTimer = window.setTimeout(() => {
      setTransitioningFrom(null);
      setContentPhase('idle');
    }, 320);

    contentTransitionTimersRef.current.push(settleTimer);
  }, [clearContentTransitionTimers, expandedView, selectedSessionId, setSelectedSessionId]);

  useEffect(() => {
    if (attentionCount > previousAttentionCount.current) {
      expand('attention');
    }

    previousAttentionCount.current = attentionCount;
  }, [attentionCount, expand]);

  useEffect(() => {
    const width = Math.round(interactionMetrics.width);
    const height = Math.round(interactionMetrics.height);
    void invoke('set_island_interaction_bounds', { width, height });
  }, [interactionMetrics.height, interactionMetrics.width]);

  useEffect(() => {
    if (contentMode === 'collapsed') {
      clearContentTransitionTimers();
      setExpandedView('sessions');
      setSelectedSessionId(null);
      setContentPhase('idle');
      setContentTransitionDirection('forward');
      setTransitioningFrom(null);
      setMeasuredExpandedHeight(null);
      setSmoothedExpandedHeight(null);
      smoothedHeightRef.current = null;
      previousNotchRectRef.current = null;
      transitionInteractionGuardUntilRef.current = 0;
      if (heightAnimationRef.current !== null) {
        cancelAnimationFrame(heightAnimationRef.current);
        heightAnimationRef.current = null;
      }
    }
  }, [clearContentTransitionTimers, contentMode, setSelectedSessionId]);

  useEffect(() => {
    if (contentMode !== 'expanded') {
      return;
    }

    const node = expandedContentRef.current;
    const activeSurfaceNode = activeExpandedSurfaceRef.current;
    if (!node || !activeSurfaceNode) {
      return;
    }

    let frameId: number | null = null;
    const observedChild =
      activeSurfaceNode.firstElementChild instanceof HTMLElement
        ? activeSurfaceNode.firstElementChild
        : null;

    const measure = () => {
      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }

      frameId = window.requestAnimationFrame(() => {
        setMeasuredExpandedHeight((previous) => {
          const nextHeight = resolveExpandedPanelHeight({
            activeSurfaceScrollHeight: activeSurfaceNode.scrollHeight,
            activeSurfaceOffsetHeight: activeSurfaceNode.offsetHeight,
            activeInnerScrollHeight: observedChild?.scrollHeight ?? 0,
            previousMeasuredHeight: previous,
          });

          if (nextHeight === null || !Number.isFinite(nextHeight) || nextHeight <= 0) {
            return previous;
          }

          return previous === nextHeight ? previous : nextHeight;
        });

        frameId = null;
      });
    };

    measure();

    const resizeObserver =
      typeof ResizeObserver === 'undefined'
        ? null
        : new ResizeObserver(() => {
            measure();
          });

    resizeObserver?.observe(activeSurfaceNode);
    if (observedChild) {
      resizeObserver?.observe(observedChild);
    }

    window.addEventListener('resize', measure);

    return () => {
      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }

      resizeObserver?.disconnect();
      window.removeEventListener('resize', measure);
    };
  }, [contentMode, expandedView, openReason, selectedSessionId, sessions]);

  // Smooth height transitions when switching between panels
  useEffect(() => {
    if (measuredExpandedHeight === null) {
      return;
    }

    const from = smoothedHeightRef.current;

    if (from === null || from === measuredExpandedHeight) {
      smoothedHeightRef.current = measuredExpandedHeight;
      setSmoothedExpandedHeight(measuredExpandedHeight);
      return;
    }

    const to = measuredExpandedHeight;
    const duration = 280;
    const startTime = performance.now();

    if (heightAnimationRef.current !== null) {
      cancelAnimationFrame(heightAnimationRef.current);
    }

    const animate = (now: number) => {
      const elapsed = now - startTime;
      const t = Math.min(elapsed / duration, 1);
      const eased = 1 - Math.pow(1 - t, 3);
      const current = Math.round(from + (to - from) * eased);

      smoothedHeightRef.current = current;
      setSmoothedExpandedHeight(current);

      if (t < 1) {
        heightAnimationRef.current = requestAnimationFrame(animate);
      } else {
        heightAnimationRef.current = null;
      }
    };

    heightAnimationRef.current = requestAnimationFrame(animate);

    return () => {
      if (heightAnimationRef.current !== null) {
        cancelAnimationFrame(heightAnimationRef.current);
        heightAnimationRef.current = null;
      }
    };
  }, [measuredExpandedHeight]);

  useEffect(() => {
    const previousContentMode = previousContentModeRef.current;
    previousContentModeRef.current = contentMode;

    if (contentMode === 'expanded' && previousContentMode !== 'expanded') {
      clearContentTransitionTimers();
      setTransitioningFrom(null);
      setContentPhase('appearing');

      const settleTimer = window.setTimeout(() => {
        setContentPhase('idle');
      }, 180);

      contentTransitionTimersRef.current.push(settleTimer);
    }
  }, [clearContentTransitionTimers, contentMode]);

  useEffect(() => {
    if (viewMode !== 'expanded') {
      return;
    }

    const handleWindowPointerMove = (event: MouseEvent) => {
      lastPointerPositionRef.current = {
        x: event.clientX,
        y: event.clientY,
      };

      const node = notchRef.current;
      if (!node) {
        return;
      }

      const rect = node.getBoundingClientRect();
      const guardActive = Date.now() < transitionInteractionGuardUntilRef.current;
      const pointerInsideBounds =
        isPointInsideRect(event.clientX, event.clientY, rect, contentPhase === 'transitioning' ? 18 : 8)
        || (guardActive
          && previousNotchRectRef.current !== null
          && isPointInsideRect(event.clientX, event.clientY, previousNotchRectRef.current, 18));

      if (pointerInsideBounds) {
        clearExpandedLeaveTimer();
      } else if (!shouldAutoCollapseExpandedIsland(hasAttentionRequest)) {
        clearExpandedLeaveTimer();
      } else if (expandedLeaveTimerRef.current === null) {
        if (openReason === 'hover') {
          collapse();
        } else {
          expandedLeaveTimerRef.current = setTimeout(() => {
            expandedLeaveTimerRef.current = null;
            collapse();
          }, 300);
        }
      }
    };

    const handleWindowMouseLeave = () => {
      // On macOS NonactivatingPanel windows, clicking inside the island
      // can trigger spurious mouseleave events due to focus routing.
      // Validate against last known mouse position before collapsing.
      const node = notchRef.current;
      if (node) {
        const rect = node.getBoundingClientRect();
        const guardActive = Date.now() < transitionInteractionGuardUntilRef.current;
        const stillInside =
          isPointInsideRect(lastPointerPositionRef.current.x, lastPointerPositionRef.current.y, rect, contentPhase === 'transitioning' ? 18 : 8)
          || (guardActive
            && previousNotchRectRef.current !== null
            && isPointInsideRect(
              lastPointerPositionRef.current.x,
              lastPointerPositionRef.current.y,
              previousNotchRectRef.current,
              18,
            ));
        if (stillInside) {
          return;
        }
      }

      clearExpandedLeaveTimer();
      if (!shouldAutoCollapseExpandedIsland(hasAttentionRequest)) {
        return;
      }

      if (openReason === 'hover') {
        collapse();
      } else {
        expandedLeaveTimerRef.current = setTimeout(() => {
          expandedLeaveTimerRef.current = null;
          collapse();
        }, 300);
      }
    };

    window.addEventListener('mousemove', handleWindowPointerMove, { passive: true });
    window.addEventListener('mouseleave', handleWindowMouseLeave);

    return () => {
      clearExpandedLeaveTimer();
      window.removeEventListener('mousemove', handleWindowPointerMove);
      window.removeEventListener('mouseleave', handleWindowMouseLeave);
    };
  }, [clearExpandedLeaveTimer, collapse, contentPhase, hasAttentionRequest, openReason, viewMode]);

  useEffect(() => {
    const handleToggle = () => toggle('shortcut');
    const handleApprove = () => {
      const session = sessions.find((s) => s.pending_permission);
      if (session?.pending_permission) {
        void approvePermission(session.pending_permission.request_id, 'allow');
      }
    };
    const handleDeny = () => {
      const session = sessions.find((s) => s.pending_permission);
      if (session?.pending_permission) {
        void approvePermission(session.pending_permission.request_id, 'deny');
      }
    };

    window.addEventListener('yorling:island-toggle', handleToggle);
    window.addEventListener('yorling:island-approve', handleApprove);
    window.addEventListener('yorling:island-deny', handleDeny);

    return () => {
      window.removeEventListener('yorling:island-toggle', handleToggle);
      window.removeEventListener('yorling:island-approve', handleApprove);
      window.removeEventListener('yorling:island-deny', handleDeny);
    };
  }, [approvePermission, sessions, toggle]);

  useEffect(() => {
    return () => {
      clearContentTransitionTimers();
    };
  }, [clearContentTransitionTimers]);

  const handleExpandedMouseDownCapture = useCallback(
    (event: ReactMouseEvent<HTMLDivElement>) => {
      lastPointerPositionRef.current = {
        x: event.clientX,
        y: event.clientY,
      };
      clearExpandedLeaveTimer();

      const nextOpenReason = promoteIslandOpenReasonForInternalInteraction(viewMode, openReason);
      if (nextOpenReason !== openReason) {
        setOpenReason(nextOpenReason);
      }
    },
    [clearExpandedLeaveTimer, openReason, setOpenReason, viewMode],
  );

  const activeSnapshot = createExpandedViewSnapshot(expandedView, selectedSessionId);

  // ─── Crossfade layer calculations ───
  // Both collapsed and expanded layers render simultaneously during transitions.
  // Opacity is driven by the spring progress for a smooth, natural morph.
  const expandedVisible = contentMode === 'expanded';
  const isMorphingToCollapsed = viewMode === 'collapsed' && progress > 0.02;
  const expandedOpacity = expandedVisible ? smoothstep(0.12, 0.4, progress) : 0;
  const collapsedOpacity = expandedVisible ? 1 - smoothstep(0.1, 0.38, progress) : 1;
  const expandedScale = expandedVisible && progress < 0.5
    ? 0.96 + 0.04 * smoothstep(0.08, 0.45, progress)
    : 1;
  const showExpandedLayer = expandedVisible;
  const showCollapsedLayer = isMorphingToCollapsed || collapsedOpacity > 0.005 || !expandedVisible;
  const shouldAttachDomMouseLeave = shouldUseDomMouseLeave(viewMode, openReason);

  const handleSelectSession = (sessionId: string) => {
    // Upgrade openReason to 'click' so that the window-level pointer tracking
    // uses a short delay before collapsing, preventing accidental closures
    // while the user interacts with the ChatView.
    expand('click');
    runExpandedViewTransition({
      direction: 'forward',
      nextView: 'chatview',
      nextSessionId: sessionId,
    });
  };

  const handleBackToList = () => {
    expand('click');
    runExpandedViewTransition({
      direction: 'back',
      nextView: 'sessions',
      nextSessionId: null,
    });
  };

  const handleShowRules = () => {
    expand('click');
    runExpandedViewTransition({
      direction: 'forward',
      nextView: 'rules',
      nextSessionId: null,
    });
  };

  const hasExpandedActivity = sessions.length > 0;
  const hasCollapsedActivity = collapsedSessions.length > 0;
  const activityLevel =
    attentionCount > 0
      ? 'attention'
      : contentMode === 'expanded'
        ? hasExpandedActivity ? 'busy' : 'idle'
        : hasCollapsedActivity ? 'busy' : 'idle';
  const hasSurfaceActivity = contentMode === 'expanded' ? hasExpandedActivity : hasCollapsedActivity;

  const renderExpandedContent = (snapshot: ExpandedViewSnapshot) => {
    const snapshotSession = snapshot.sessionId
      ? sessions.find((session) => session.id === snapshot.sessionId) ?? null
      : null;

    if (snapshot.view === 'rules') {
      return <ApprovalRulesPanel onBack={handleBackToList} />;
    }
    if (snapshot.view === 'chatview' && snapshotSession) {
      return <ChatView session={snapshotSession} onBack={handleBackToList} />;
    }
    return (
      <SessionList
        sessions={sessions}
        onCollapse={collapse}
        onSelectSession={handleSelectSession}
        onShowRules={handleShowRules}
        openReason={openReason}
      />
    );
  };

  const activeSurfaceClass = [
    'island-content-surface',
    'island-content-surface--current',
    contentPhase === 'idle'
      ? ''
      : contentTransitionDirection === 'forward'
        ? 'island-content-surface--forward-enter'
        : 'island-content-surface--back-enter',
  ].filter(Boolean).join(' ');
  const ghostSurfaceClass = transitioningFrom
    ? [
        'island-content-surface',
        'island-content-surface--ghost',
        contentTransitionDirection === 'forward'
          ? 'island-content-surface--forward-exit'
          : 'island-content-surface--back-exit',
      ].join(' ')
    : '';

  return (
    <NotchShape
      ref={notchRef}
      surfaceMetrics={interactionMetrics}
      isAnimating={isAnimating}
      isBouncing={isBouncing}
      activityLevel={activityLevel}
      hasActivity={hasSurfaceActivity}
      placementMode={screenInfo?.placement_mode ?? 'top_bar'}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={shouldAttachDomMouseLeave ? handleMouseLeave : undefined}
      onMouseDownCapture={handleExpandedMouseDownCapture}
    >
      {/* ─── Crossfade content layers ─── */}
      <div className="island-content-layers">
        {showExpandedLayer && (
          <div
            className={`island-content-layer island-content-layer--expanded${expandedOpacity < 0.01 ? ' island-content-layer--hidden' : ''}`}
            style={{
              opacity: expandedOpacity,
              transform: expandedScale < 0.999 ? `scale(${expandedScale})` : undefined,
            }}
          >
            <div
              className="island-content-transition"
              ref={expandedContentRef}
            >
              <div
                className={activeSurfaceClass}
                key={activeSnapshot.key}
                ref={activeExpandedSurfaceRef}
              >
                {renderExpandedContent(activeSnapshot)}
              </div>
              {transitioningFrom ? (
                <div className={ghostSurfaceClass} aria-hidden="true">
                  {renderExpandedContent(transitioningFrom)}
                </div>
              ) : null}
            </div>
          </div>
        )}
        {showCollapsedLayer && (
          <div
            className={`island-content-layer island-content-layer--collapsed${collapsedOpacity < 0.01 ? ' island-content-layer--hidden' : ''}`}
            style={{ opacity: collapsedOpacity }}
          >
            <CollapsedBar sessions={collapsedSessions} onClick={() => toggle('click')} />
          </div>
        )}
      </div>

      {notifications.length > 0 && contentMode === 'collapsed' && (
        <CompletionToast
          notifications={notifications}
          onDismiss={removeNotification}
          onOpenSession={handleSelectSession}
        />
      )}
    </NotchShape>
  );
}

function createExpandedViewSnapshot(view: ExpandedView, sessionId: string | null): ExpandedViewSnapshot {
  return {
    view,
    sessionId,
    key: `${view}:${sessionId ?? 'none'}`,
  };
}

function isPointInsideRect(x: number, y: number, rect: DOMRect | { left: number; right: number; top: number; bottom: number }, margin: number) {
  return x >= rect.left - margin
    && x <= rect.right + margin
    && y >= rect.top - margin
    && y <= rect.bottom + margin;
}

function estimateExpandedHeight({
  sessions,
  expandedView,
  selectedSessionId,
}: {
  sessions: IslandSession[];
  openReason: string | null;
  expandedView: ExpandedView;
  selectedSessionId: string | null;
}) {
  if (expandedView === 'chatview' && selectedSessionId) {
    return null;
  }

  if (expandedView === 'rules') {
    return 260;
  }

  if (sessions.length === 0) {
    return 200;
  }

  let height = 86;

  for (const session of sessions) {
    let cardHeight = 81;

    if (session.tools_in_flight.length > 0) {
      cardHeight += 38;
    }

    if (session.pending_permission) {
      cardHeight += 160;
    }

    if (session.pending_question) {
      cardHeight += 130;
    }

    height += cardHeight;
  }

  // Add gaps between session cards (10px each)
  if (sessions.length > 1) {
    height += (sessions.length - 1) * 10;
  }

  return height;
}
