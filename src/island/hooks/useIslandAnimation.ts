import { useState, useCallback, useRef, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import type { IslandViewMode } from '../../types';
import { useIslandStore } from '../store/islandStore';
import { shouldCollapseOnPointerLeave } from '../panelState';

export type IslandOpenReason =
  | 'click'
  | 'hover'
  | 'attention'
  | 'shortcut'
  | 'boot'
  | null;

// CodeIsland-matched spring presets (derived from SwiftUI spring(response, dampingFraction)):
// ω = 2π/response, stiffness = ω², damping = 2 × dampingFraction × ω
const OPEN_STIFFNESS = 224;   // response=0.42, df=0.82 — slight overshoot
const OPEN_DAMPING = 24.5;
const CLOSE_STIFFNESS = 273;  // response=0.38, df=1.0  — critically damped, no overshoot
const CLOSE_DAMPING = 33;
const POP_STIFFNESS = 439;   // response=0.3,  df=0.65 — fast bounce for attention/notifications
const POP_DAMPING = 27;
const BOOT_TARGET = 0.35;
const BOUNCE_TARGET = 0.08;
const SETTLE_DISTANCE = 0.0015;
const SETTLE_VELOCITY = 0.01;
const HOVER_OPEN_DELAY = 240;
const POINTER_LEAVE_COLLAPSE_DELAY = 140;

/**
 * Controls island expand/collapse animation and differentiates click, hover,
 * attention, and boot-driven opens so we can render the right surface.
 */
export function useIslandAnimation() {
  const viewMode = useIslandStore((s) => s.viewMode);
  const setViewMode = useIslandStore((s) => s.setViewMode);
  const [isAnimating, setIsAnimating] = useState(false);
  const [isBouncing, setIsBouncing] = useState(false);
  const [contentMode, setContentMode] = useState<IslandViewMode>(viewMode);
  const [openReason, setOpenReason] = useState<IslandOpenReason>(null);
  const [progress, setProgress] = useState(viewMode === 'expanded' ? 1 : 0);
  const rafRef = useRef<number | null>(null);
  const lastFrameRef = useRef<number | null>(null);
  const progressRef = useRef(progress);
  const velocityRef = useRef(0);
  const targetRef = useRef(progress);
  const springConfigRef = useRef({ stiffness: OPEN_STIFFNESS, damping: OPEN_DAMPING });
  const bootPlayedRef = useRef(false);
  const autoCollapseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const hoverOpenTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isBootAnimatingRef = useRef(false);
  const onSettleCallbackRef = useRef<(() => void) | null>(null);
  const clearAutoCollapseTimer = useCallback(() => {
    if (autoCollapseTimerRef.current !== null) {
      clearTimeout(autoCollapseTimerRef.current);
      autoCollapseTimerRef.current = null;
    }
  }, []);

  const clearHoverOpenTimer = useCallback(() => {
    if (hoverOpenTimerRef.current !== null) {
      clearTimeout(hoverOpenTimerRef.current);
      hoverOpenTimerRef.current = null;
    }
  }, []);

  const startSpring = useCallback(
    (target: number, config: { stiffness: number; damping: number }, onSettle?: () => void) => {
      targetRef.current = target;
      springConfigRef.current = config;
      onSettleCallbackRef.current = onSettle ?? null;
      setIsAnimating(true);

      if (rafRef.current !== null) {
        return;
      }

      const tick = (now: number) => {
        if (lastFrameRef.current === null) {
          lastFrameRef.current = now;
        }

        const dt = Math.min((now - lastFrameRef.current) / 1000, 1 / 24);
        lastFrameRef.current = now;

        const { stiffness, damping } = springConfigRef.current;
        const displacement = targetRef.current - progressRef.current;
        const acceleration =
          displacement * stiffness - velocityRef.current * damping;

        velocityRef.current += acceleration * dt;
        progressRef.current += velocityRef.current * dt;

        const settled =
          Math.abs(targetRef.current - progressRef.current) < SETTLE_DISTANCE &&
          Math.abs(velocityRef.current) < SETTLE_VELOCITY;

        if (settled) {
          progressRef.current = targetRef.current;
          velocityRef.current = 0;
          lastFrameRef.current = null;
          rafRef.current = null;
          setProgress(progressRef.current);
          setIsAnimating(false);

          if (targetRef.current === 0) {
            setContentMode('collapsed');
            setOpenReason(null);
          }

          const cb = onSettleCallbackRef.current;
          onSettleCallbackRef.current = null;
          if (cb) cb();

          return;
        }

        setProgress(progressRef.current);
        rafRef.current = window.requestAnimationFrame(tick);
      };

      rafRef.current = window.requestAnimationFrame(tick);
    },
    [],
  );

  const expand = useCallback(
    (reason: Exclude<IslandOpenReason, null> = 'click') => {
      clearAutoCollapseTimer();
      clearHoverOpenTimer();
      isBootAnimatingRef.current = false;
      setOpenReason(reason);

      if (targetRef.current === 1 && progressRef.current >= 1 - SETTLE_DISTANCE) {
        setViewMode('expanded');
        setContentMode('expanded');
        return;
      }

      setViewMode('expanded');
      setContentMode('expanded');
      const config = reason === 'attention'
        ? { stiffness: POP_STIFFNESS, damping: POP_DAMPING }
        : { stiffness: OPEN_STIFFNESS, damping: OPEN_DAMPING };
      startSpring(1, config);
    },
    [clearAutoCollapseTimer, clearHoverOpenTimer, setViewMode, startSpring],
  );

  const collapse = useCallback(() => {
    clearAutoCollapseTimer();
    clearHoverOpenTimer();
    isBootAnimatingRef.current = false;
    if (targetRef.current === 0 && progressRef.current <= SETTLE_DISTANCE) return;
    setViewMode('collapsed');
    startSpring(0, { stiffness: CLOSE_STIFFNESS, damping: CLOSE_DAMPING });
  }, [clearAutoCollapseTimer, clearHoverOpenTimer, setViewMode, startSpring]);

  const toggle = useCallback(
    (reason: Exclude<IslandOpenReason, null> = 'click') => {
      if (targetRef.current < 0.5) {
        expand(reason);
        return;
      }
      collapse();
    },
    [collapse, expand],
  );

  const bounce = useCallback(() => {
    if (targetRef.current > 0.1) return;
    if (isBouncing) return;
    setIsBouncing(true);
    startSpring(
      BOUNCE_TARGET,
      { stiffness: POP_STIFFNESS, damping: POP_DAMPING },
      () => {
        startSpring(0, { stiffness: CLOSE_STIFFNESS, damping: CLOSE_DAMPING }, () => {
          setIsBouncing(false);
        });
      },
    );
  }, [isBouncing, startSpring]);

  // Boot animation: brief expand-then-collapse on first mount
  useEffect(() => {
    if (bootPlayedRef.current) return;
    bootPlayedRef.current = true;

    const bootTimer = setTimeout(() => {
      isBootAnimatingRef.current = true;
      setOpenReason('boot');
      startSpring(
        BOOT_TARGET,
        { stiffness: POP_STIFFNESS, damping: POP_DAMPING },
        () => {
          if (!isBootAnimatingRef.current) return;
          setTimeout(() => {
            if (!isBootAnimatingRef.current) return;
            isBootAnimatingRef.current = false;
            startSpring(0, { stiffness: CLOSE_STIFFNESS, damping: CLOSE_DAMPING });
          }, 200);
        },
      );
    }, 500);

    return () => clearTimeout(bootTimer);
  }, [startSpring]);

  const handleMouseEnter = useCallback(() => {
    clearAutoCollapseTimer();

    if (viewMode !== 'collapsed' || isAnimating) {
      return;
    }

    clearHoverOpenTimer();
    hoverOpenTimerRef.current = setTimeout(() => {
      expand('hover');
    }, HOVER_OPEN_DELAY);
  }, [clearAutoCollapseTimer, clearHoverOpenTimer, expand, isAnimating, viewMode]);

  const handleMouseLeave = useCallback(() => {
    clearHoverOpenTimer();

    if (viewMode !== 'expanded' || !shouldCollapseOnPointerLeave(openReason)) {
      return;
    }

    clearAutoCollapseTimer();
    autoCollapseTimerRef.current = setTimeout(() => {
      autoCollapseTimerRef.current = null;
      collapse();
    }, POINTER_LEAVE_COLLAPSE_DELAY);
  }, [clearAutoCollapseTimer, clearHoverOpenTimer, collapse, openReason, viewMode]);

  useEffect(() => {
    const unlistenPromise = listen('island-outside-click', () => {
      if (viewMode === 'expanded') {
        collapse();
      }
    });

    const unlistenHoverEnterPromise = listen('island-hover-trigger-enter', () => {
      handleMouseEnter();
    });

    const unlistenHoverLeavePromise = listen('island-hover-trigger-leave', () => {
      handleMouseLeave();
    });

    const unlistenCollapsedClickPromise = listen('island-collapsed-click', () => {
      if (viewMode === 'collapsed') {
        expand('click');
      }
    });

    // Collapse after a successful jump-to-terminal
    const handleJumpCollapse = () => {
      if (viewMode === 'expanded') {
        collapse();
      }
    };
    window.addEventListener('yorling:island-collapse', handleJumpCollapse);

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
      unlistenHoverEnterPromise.then((unlisten) => unlisten());
      unlistenHoverLeavePromise.then((unlisten) => unlisten());
      unlistenCollapsedClickPromise.then((unlisten) => unlisten());
      window.removeEventListener('yorling:island-collapse', handleJumpCollapse);
    };
  }, [collapse, expand, handleMouseEnter, handleMouseLeave, viewMode]);

  useEffect(() => {
    return () => {
      if (rafRef.current !== null) {
        window.cancelAnimationFrame(rafRef.current);
      }
      clearAutoCollapseTimer();
      clearHoverOpenTimer();
    };
  }, [clearAutoCollapseTimer, clearHoverOpenTimer]);

  return {
    viewMode,
    contentMode,
    openReason,
    progress,
    isAnimating,
    isBouncing,
    expand,
    collapse,
    toggle,
    bounce,
    setOpenReason,
    handleMouseEnter,
    handleMouseLeave,
  };
}
