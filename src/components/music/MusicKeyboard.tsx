import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import {
  EVENT_CODE_TO_KEY_ID,
  getMusicInputAutoReleaseMs,
  getMusicKeyBindings,
  getMidiToKeyIdMap,
  noteColorClass,
  noteLabel,
  resolveMusicKeyboardEvent,
  shouldEnableMusicNativeKeySuppression,
  shouldPauseNativeKeyboardInterceptor,
  type MusicKeyBinding,
  type MusicKeyboardControlAction,
  type MusicEventResolution,
} from '../../data/musicKeyMapping';
import { useKeyboardService } from '../../hooks/useTauriCommand';
import { useMusicAudio } from '../../hooks/useMusicAudio';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import { clampMusicPanelSplit, useMusicStore } from '../../stores/musicStore';
import { useTutorialStore, getCurrentSongTime, getSelectedSong } from '../../stores/tutorialStore';
import { getLatestActiveSuccessPulseIdsByMidi, useScoringStore } from '../../stores/scoringStore';
import { useKeyboardStore } from '../../stores/keyboardStore';
import { MiniToggle } from '../common/MiniToggle';
import { KEYBOARD_LAYOUT_ROWS, type KeyboardLayoutKey } from '../keyboard/keyboardUiModel';
import { InstrumentSelector } from './InstrumentSelector';
import { TutorialPanel } from './TutorialPanel';

// ── Single key component ────────────────────────────────────────────

interface ActiveMusicInput {
  keyId: string;
  midi: number | null;
  autoReleaseTimerId: number | null;
}

const MUSIC_TOP_PANEL_MIN_HEIGHT = 220;
const MUSIC_BOTTOM_PANEL_MIN_HEIGHT = 190;
const KEY_LABEL_BY_ID = new Map(
  KEYBOARD_LAYOUT_ROWS.flat().map((keyItem) => [keyItem.id, keyItem.label] as const),
);

interface MusicNativeKeyEventPayload {
  code: string;
  key_down: boolean;
}

function clampMusicPanelSplitForLayout(nextSplit: number, availableHeight: number): number {
  const normalizedSplit = clampMusicPanelSplit(nextSplit);

  if (!Number.isFinite(availableHeight) || availableHeight <= 0) {
    return normalizedSplit;
  }

  const minTopSplit = Math.min(MUSIC_TOP_PANEL_MIN_HEIGHT / availableHeight, 0.5);
  const maxTopSplit = Math.max(1 - MUSIC_BOTTOM_PANEL_MIN_HEIGHT / availableHeight, 0.5);

  if (minTopSplit > maxTopSplit) {
    return 0.5;
  }

  return Math.min(maxTopSplit, Math.max(minTopSplit, normalizedSplit));
}

const MusicKey = memo(function MusicKey({
  keyItem,
  binding,
  pressed,
  successPulseId,
  held,
  onPointerStart,
  onPointerEnd,
}: {
  keyItem: KeyboardLayoutKey;
  binding: MusicKeyBinding | undefined;
  pressed: boolean;
  successPulseId: number | null;
  held: boolean;
  onPointerStart: (keyId: string, pointerId: number) => void;
  onPointerEnd: (keyId: string, pointerId: number) => void;
}) {
  const noteInfo = binding?.kind === 'note' ? binding.noteInfo : undefined;
  const detailLabel = binding?.kind === 'note'
    ? noteLabel(binding.noteInfo)
    : binding?.kind === 'control'
      ? binding.display
      : null;
  const ariaLabel = detailLabel ? `${keyItem.label} ${detailLabel}` : keyItem.label;

  const classes = [
    'music-key',
    noteInfo ? 'has-note' : '',
    binding?.kind === 'control' ? 'has-action' : '',
    noteInfo ? noteColorClass(noteInfo.note) : '',
    pressed ? 'pressed' : '',
    successPulseId !== null ? 'hit-success' : '',
    held ? 'tutorial-held' : '',
  ].filter(Boolean).join(' ');

  const handlePointerDown = binding
    ? (event: ReactPointerEvent<HTMLDivElement>) => {
        event.preventDefault();
        event.stopPropagation();
        onPointerStart(keyItem.id, event.pointerId);
      }
    : undefined;

  const handlePointerRelease = binding
    ? (event: ReactPointerEvent<HTMLDivElement>) => {
        event.preventDefault();
        event.stopPropagation();
        onPointerEnd(keyItem.id, event.pointerId);
      }
    : undefined;

  const handleLostPointerCapture = binding
    ? (event: ReactPointerEvent<HTMLDivElement>) => {
        onPointerEnd(keyItem.id, event.pointerId);
      }
    : undefined;

  return (
    <div
      className={classes}
      style={{ '--key-flex': keyItem.width } as CSSProperties}
      aria-label={ariaLabel}
      onPointerDown={handlePointerDown}
      onPointerUp={handlePointerRelease}
      onPointerCancel={handlePointerRelease}
      onLostPointerCapture={handleLostPointerCapture}
    >
      {pressed && noteInfo ? (
        <span className="music-key-press-glow" aria-hidden="true" />
      ) : null}
      {successPulseId !== null ? (
        <span key={successPulseId} className="music-key-hit-flash" aria-hidden="true" />
      ) : null}
      <span className="music-key-label">{keyItem.label}</span>
      {binding?.kind === 'note' ? (
        <span className="music-key-note">{detailLabel}</span>
      ) : binding?.kind === 'control' ? (
        <span className="music-key-action">{binding.display}</span>
      ) : null}
    </div>
  );
});

MusicKey.displayName = 'MusicKey';

// ── Main keyboard component ─────────────────────────────────────────

export function MusicKeyboard() {
  const language = useAppStore((s) => s.language);
  const keyboardStatus = useKeyboardStore((s) => s.status);
  const copy = getUiCopy(language);
  const { setEnabled, setMusicNativeKeySuppression } = useKeyboardService();

  const selectedInstrument = useMusicStore((s) => s.selectedInstrument);
  const recentInstrumentIds = useMusicStore((s) => s.recentInstrumentIds);
  const panelSplit = useMusicStore((s) => s.panelSplit);
  const volume = useMusicStore((s) => s.volume);
  const keyboardLayout = useMusicStore((s) => s.keyboardLayout);
  const pianoLayoutOctave = useMusicStore((s) => s.pianoLayoutOctave);
  const pianoLowerLayoutOctave = useMusicStore((s) => s.pianoLowerLayoutOctave);
  const pianoNumberLayoutOctave = useMusicStore((s) => s.pianoNumberLayoutOctave);
  const setInstrument = useMusicStore((s) => s.setInstrument);
  const removeRecentInstrument = useMusicStore((s) => s.removeRecentInstrument);
  const setPanelSplit = useMusicStore((s) => s.setPanelSplit);
  const setVolume = useMusicStore((s) => s.setVolume);
  const setKeyboardLayout = useMusicStore((s) => s.setKeyboardLayout);
  const shiftPianoLayoutOctave = useMusicStore((s) => s.shiftPianoLayoutOctave);
  const shiftPianoLowerLayoutOctave = useMusicStore((s) => s.shiftPianoLowerLayoutOctave);
  const shiftPianoNumberLayoutOctave = useMusicStore((s) => s.shiftPianoNumberLayoutOctave);

  const { noteOn, noteOff, stopAll, unlock, loading, error, unlocked } = useMusicAudio(selectedInstrument, volume);

  const tutorialActive = useTutorialStore((s) => s.active);
  const setTutorialActive = useTutorialStore((s) => s.setActive);
  const [pressedKeyIds, setPressedKeyIds] = useState<Set<string>>(new Set());
  const successPulses = useScoringStore((s) => s.successPulses);
  const heldMidis = useScoringStore((s) => s.heldMidis);
  const [isDividerDragging, setIsDividerDragging] = useState(false);
  const [isWindowFrontmost, setIsWindowFrontmost] = useState(
    () => typeof document === 'undefined' || (!document.hidden && document.hasFocus()),
  );
  const activeInputsRef = useRef<Map<string, ActiveMusicInput>>(new Map());
  const dividerRef = useRef<HTMLDivElement | null>(null);
  const dividerDragStateRef = useRef<{ containerTop: number; availableHeight: number } | null>(null);
  const dividerFrameRef = useRef<number | null>(null);
  const layoutShellRef = useRef<HTMLDivElement | null>(null);
  const pendingPanelSplitRef = useRef<number | null>(null);
  const pointerInputsRef = useRef<Map<number, string>>(new Map());
  const pressedCountsRef = useRef<Map<string, number>>(new Map());
  const restoreNativeInterceptorRef = useRef(false);
  const musicNativeSuppressionRef = useRef(false);
  const pianoLayoutState = useMemo(() => ({
    mainOctave: pianoLayoutOctave,
    lowerOctave: pianoLowerLayoutOctave,
    numberOctave: pianoNumberLayoutOctave,
  }), [pianoLayoutOctave, pianoLowerLayoutOctave, pianoNumberLayoutOctave]);
  const keyBindings = useMemo(
    () => getMusicKeyBindings(keyboardLayout, pianoLayoutState),
    [keyboardLayout, pianoLayoutState],
  );
  const midiToKeyId = useMemo(
    () => getMidiToKeyIdMap(keyboardLayout, pianoLayoutState),
    [keyboardLayout, pianoLayoutState],
  );
  const heldKeyIds = useMemo(() => {
    const ids = new Set<string>();
    for (const midi of heldMidis) {
      const keyId = midiToKeyId.get(midi);
      if (keyId) ids.add(keyId);
    }
    return ids;
  }, [heldMidis, midiToKeyId]);
  const activeSuccessPulseIdsByMidi = useMemo(
    () => getLatestActiveSuccessPulseIdsByMidi(successPulses),
    [successPulses],
  );
  const keyLabelByMidi = useMemo(() => {
    const labels = new Map<number, string>();

    for (const [midi, keyId] of midiToKeyId.entries()) {
      labels.set(midi, KEY_LABEL_BY_ID.get(keyId) ?? keyId);
    }

    return labels;
  }, [midiToKeyId]);
  const handleTrustedAudioUnlock = useCallback(() => {
    void unlock();
  }, [unlock]);

  const updatePressedKeyId = useCallback((keyId: string, delta: 1 | -1) => {
    const pressedCounts = pressedCountsRef.current;
    const nextCount = Math.max(0, (pressedCounts.get(keyId) ?? 0) + delta);

    if (nextCount === 0) {
      pressedCounts.delete(keyId);
    } else {
      pressedCounts.set(keyId, nextCount);
    }

    setPressedKeyIds(new Set(pressedCounts.keys()));
  }, []);

  const releaseInput = useCallback((inputId: string) => {
    const activeInput = activeInputsRef.current.get(inputId);
    if (!activeInput) {
      return;
    }

    if (activeInput.autoReleaseTimerId !== null) {
      window.clearTimeout(activeInput.autoReleaseTimerId);
    }

    activeInputsRef.current.delete(inputId);
    updatePressedKeyId(activeInput.keyId, -1);
    if (activeInput.midi !== null) {
      noteOff(activeInput.midi);
      // Scoring: judge the release for hold notes
      const tutState = useTutorialStore.getState();
      const scorState = useScoringStore.getState();
      if (tutState.active && tutState.isPlaying && scorState.enabled) {
        const songTime = getCurrentSongTime(tutState);
        scorState.judgeRelease(activeInput.midi, songTime);
      }
    }
  }, [noteOff, updatePressedKeyId]);

  const startInput = useCallback((
    inputId: string,
    keyId: string,
    midi: number | null,
    autoReleaseMs: number | null = null,
  ) => {
    if (activeInputsRef.current.has(inputId)) {
      return;
    }

    const activeInput: ActiveMusicInput = {
      keyId,
      midi,
      autoReleaseTimerId: null,
    };

    activeInputsRef.current.set(inputId, activeInput);
    updatePressedKeyId(keyId, 1);
    if (midi !== null) {
      noteOn(midi);
      // Scoring: judge the hit if tutorial is playing
      const tutState = useTutorialStore.getState();
      const scorState = useScoringStore.getState();
      if (tutState.active && tutState.isPlaying && !tutState.isDemoPlaying && scorState.enabled) {
        tutState.markCurrentSongPracticed();
        const songTime = getCurrentSongTime(tutState);
        const song = getSelectedSong(tutState.songId);
        scorState.judgeHit(midi, songTime, song.notes);
      }
    }

    if (autoReleaseMs !== null) {
      activeInput.autoReleaseTimerId = window.setTimeout(() => {
        releaseInput(inputId);
      }, autoReleaseMs);
    }
  }, [noteOn, releaseInput, updatePressedKeyId]);

  const triggerControlAction = useCallback((action: MusicKeyboardControlAction) => {
    switch (action) {
      case 'mainOctaveDown':
        shiftPianoLayoutOctave(-1);
        return;
      case 'mainOctaveUp':
        shiftPianoLayoutOctave(1);
        return;
      case 'lowerOctaveDown':
        shiftPianoLowerLayoutOctave(-1);
        return;
      case 'lowerOctaveUp':
        shiftPianoLowerLayoutOctave(1);
        return;
      case 'numberOctaveDown':
        shiftPianoNumberLayoutOctave(-1);
        return;
      case 'numberOctaveUp':
        shiftPianoNumberLayoutOctave(1);
        return;
    }
  }, [shiftPianoLayoutOctave, shiftPianoLowerLayoutOctave, shiftPianoNumberLayoutOctave]);

  const startResolvedKeyboardInput = useCallback((
    code: string,
    resolution: MusicEventResolution,
  ) => {
    const keyId = EVENT_CODE_TO_KEY_ID[code];
    if (!keyId) {
      return;
    }

    if (resolution.controlAction !== null) {
      const inputId = `keyboard:${code}`;
      releaseInput(inputId);
      startInput(inputId, keyId, null, getMusicInputAutoReleaseMs(code));
      triggerControlAction(resolution.controlAction);
      return;
    }

    if (resolution.midi === null) {
      return;
    }

    startInput(
      `keyboard:${code}`,
      keyId,
      resolution.midi,
      getMusicInputAutoReleaseMs(code),
    );
  }, [releaseInput, startInput, triggerControlAction]);

  const releaseResolvedKeyboardInput = useCallback((
    code: string,
    resolution: MusicEventResolution,
  ) => {
    if (!resolution.shouldSwallow) {
      return;
    }

    releaseInput(`keyboard:${code}`);
  }, [releaseInput]);

  const releaseAll = useCallback(() => {
    for (const activeInput of activeInputsRef.current.values()) {
      if (activeInput.autoReleaseTimerId !== null) {
        window.clearTimeout(activeInput.autoReleaseTimerId);
      }
    }

    activeInputsRef.current.clear();
    pointerInputsRef.current.clear();
    pressedCountsRef.current.clear();
    setPressedKeyIds(new Set());
    stopAll();
  }, [stopAll]);

  const handlePointerStart = useCallback((keyId: string, pointerId: number) => {
    const binding = keyBindings[keyId];
    if (!binding) {
      return;
    }

    const inputId = `pointer:${pointerId}:${keyId}`;
    pointerInputsRef.current.set(pointerId, inputId);
    if (binding.kind === 'note') {
      startInput(inputId, keyId, binding.noteInfo.midi);
      return;
    }

    startInput(inputId, keyId, null);
    triggerControlAction(binding.action);
  }, [keyBindings, startInput, triggerControlAction]);

  const handlePointerEnd = useCallback((keyId: string, pointerId: number) => {
    const inputId = pointerInputsRef.current.get(pointerId) ?? `pointer:${pointerId}:${keyId}`;
    pointerInputsRef.current.delete(pointerId);
    releaseInput(inputId);
  }, [releaseInput]);

  const purePlayActive = isWindowFrontmost;
  const showMutedHint = volume === 0 && !loading && !error;
  const topPanelFlex = clampMusicPanelSplit(panelSplit);
  const bottomPanelFlex = Math.max(0.01, 1 - topPanelFlex);

  useEffect(() => {
    const handleDividerPointerMove = (event: PointerEvent) => {
      const dragState = dividerDragStateRef.current;

      if (!dragState) {
        return;
      }

      const nextSplit = clampMusicPanelSplitForLayout(
        (event.clientY - dragState.containerTop) / dragState.availableHeight,
        dragState.availableHeight,
      );
      pendingPanelSplitRef.current = nextSplit;

      if (dividerFrameRef.current !== null) {
        return;
      }

      dividerFrameRef.current = window.requestAnimationFrame(() => {
        dividerFrameRef.current = null;

        if (pendingPanelSplitRef.current !== null) {
          setPanelSplit(pendingPanelSplitRef.current, false);
        }
      });
    };

    const stopDividerDrag = () => {
      if (!dividerDragStateRef.current) {
        return;
      }

      if (dividerFrameRef.current !== null) {
        window.cancelAnimationFrame(dividerFrameRef.current);
        dividerFrameRef.current = null;
      }

      const finalSplit = pendingPanelSplitRef.current ?? useMusicStore.getState().panelSplit;
      pendingPanelSplitRef.current = null;
      setPanelSplit(finalSplit);
      dividerDragStateRef.current = null;
      setIsDividerDragging(false);
      document.body.classList.remove('music-layout-resizing');
    };

    window.addEventListener('pointermove', handleDividerPointerMove);
    window.addEventListener('pointerup', stopDividerDrag);
    window.addEventListener('pointercancel', stopDividerDrag);

    return () => {
      window.removeEventListener('pointermove', handleDividerPointerMove);
      window.removeEventListener('pointerup', stopDividerDrag);
      window.removeEventListener('pointercancel', stopDividerDrag);

      if (dividerFrameRef.current !== null) {
        window.cancelAnimationFrame(dividerFrameRef.current);
        dividerFrameRef.current = null;
      }

      dividerDragStateRef.current = null;
      pendingPanelSplitRef.current = null;
      document.body.classList.remove('music-layout-resizing');
    };
  }, [setPanelSplit]);

  useEffect(() => {
    if (purePlayActive) {
      if (!restoreNativeInterceptorRef.current
        && shouldPauseNativeKeyboardInterceptor(purePlayActive, {
          running: keyboardStatus.running,
          enabled: keyboardStatus.enabled,
        })
      ) {
        restoreNativeInterceptorRef.current = true;
        void setEnabled(false, { persistPreference: false });
      }
      return;
    }

    if (!restoreNativeInterceptorRef.current) {
      return;
    }

    restoreNativeInterceptorRef.current = false;

    if (keyboardStatus.running) {
      void setEnabled(true, { persistPreference: false });
    }
  }, [keyboardStatus.enabled, keyboardStatus.running, purePlayActive, setEnabled]);

  useEffect(() => {
    // Keep DOM keyboard events available until a trusted browser gesture has
    // unlocked Web Audio; otherwise physical keys may arrive only through the
    // Tauri event bridge, which does not satisfy WebKit's gesture requirement.
    const shouldSuppress = shouldEnableMusicNativeKeySuppression(
      keyboardLayout,
      purePlayActive,
      unlocked,
    );
    if (musicNativeSuppressionRef.current === shouldSuppress) {
      return;
    }

    if (shouldSuppress && !keyboardStatus.has_accessibility && !keyboardStatus.running) {
      return;
    }

    musicNativeSuppressionRef.current = shouldSuppress;
    void setMusicNativeKeySuppression(shouldSuppress);
  }, [
    keyboardLayout,
    keyboardStatus.has_accessibility,
    keyboardStatus.running,
    purePlayActive,
    setMusicNativeKeySuppression,
    unlocked,
  ]);

  useEffect(() => () => {
    if (!musicNativeSuppressionRef.current) {
      return;
    }

    musicNativeSuppressionRef.current = false;
    void setMusicNativeKeySuppression(false);
  }, [setMusicNativeKeySuppression]);

  useEffect(() => {
    const handleWindowPointerUp = (e: PointerEvent) => {
      const inputId = pointerInputsRef.current.get(e.pointerId);
      if (!inputId) return;

      pointerInputsRef.current.delete(e.pointerId);
      releaseInput(inputId);
    };

      const handleKeyDown = (e: KeyboardEvent) => {
        if (purePlayActive && !unlocked) {
          void unlock();
        }

        const resolution = resolveMusicKeyboardEvent(
          e.code,
          purePlayActive,
          keyboardLayout,
          pianoLayoutState,
        );
      if (!resolution.shouldSwallow) return;

        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        if (e.repeat) return;

        startResolvedKeyboardInput(e.code, resolution);
      };

    const handleKeyUp = (e: KeyboardEvent) => {
        const resolution = resolveMusicKeyboardEvent(
          e.code,
          purePlayActive,
          keyboardLayout,
          pianoLayoutState,
        );
      if (!resolution.shouldSwallow) return;

        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        releaseResolvedKeyboardInput(e.code, resolution);
      };

    const handleFocus = () => {
      setIsWindowFrontmost(!document.hidden && document.hasFocus());
    };

    const handleBlur = () => {
      setIsWindowFrontmost(false);
      releaseAll();
    };

    const handleVisibilityChange = () => {
      const nextIsFrontmost = !document.hidden && document.hasFocus();
      setIsWindowFrontmost(nextIsFrontmost);
      if (!nextIsFrontmost) releaseAll();
    };

    window.addEventListener('keydown', handleKeyDown, { capture: true });
    window.addEventListener('keyup', handleKeyUp, { capture: true });
    window.addEventListener('pointerup', handleWindowPointerUp, { capture: true });
    window.addEventListener('pointercancel', handleWindowPointerUp, { capture: true });
    window.addEventListener('focus', handleFocus);
    window.addEventListener('blur', handleBlur);
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      if (restoreNativeInterceptorRef.current) {
        restoreNativeInterceptorRef.current = false;

        if (keyboardStatus.running) {
          void setEnabled(true, { persistPreference: false });
        }
      }

      window.removeEventListener('keydown', handleKeyDown, { capture: true });
      window.removeEventListener('keyup', handleKeyUp, { capture: true });
      window.removeEventListener('pointerup', handleWindowPointerUp, { capture: true });
      window.removeEventListener('pointercancel', handleWindowPointerUp, { capture: true });
      window.removeEventListener('focus', handleFocus);
      window.removeEventListener('blur', handleBlur);
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      releaseAll();
    };
  }, [
    keyboardLayout,
    keyboardStatus.running,
    pianoLayoutState,
    purePlayActive,
    releaseAll,
    releaseResolvedKeyboardInput,
    setEnabled,
    startResolvedKeyboardInput,
    unlock,
    unlocked,
  ]);

  useEffect(() => {
    const unlistenPromise = listen<MusicNativeKeyEventPayload>('music-native-key', (event) => {
      const resolution = resolveMusicKeyboardEvent(
        event.payload.code,
        purePlayActive,
        keyboardLayout,
        pianoLayoutState,
      );
      if (!resolution.shouldSwallow) {
        return;
      }

      if (event.payload.key_down) {
        startResolvedKeyboardInput(event.payload.code, resolution);
        return;
      }

      releaseResolvedKeyboardInput(event.payload.code, resolution);
    });

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [
    keyboardLayout,
    pianoLayoutState,
    purePlayActive,
    releaseResolvedKeyboardInput,
    startResolvedKeyboardInput,
  ]);

  const handleDividerPointerDown = useCallback((event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }

    if (event.target instanceof HTMLElement && event.target.closest('[data-music-resize-exempt]')) {
      return;
    }

    const layoutShell = layoutShellRef.current;
    const divider = dividerRef.current;
    if (!layoutShell || !divider) {
      return;
    }

    const availableHeight = layoutShell.clientHeight - divider.getBoundingClientRect().height;
    if (availableHeight <= 0) {
      return;
    }

    const nextSplit = clampMusicPanelSplitForLayout(panelSplit, availableHeight);
    pendingPanelSplitRef.current = nextSplit;
    dividerDragStateRef.current = {
      containerTop: layoutShell.getBoundingClientRect().top,
      availableHeight,
    };
    setPanelSplit(nextSplit, false);
    setIsDividerDragging(true);
    document.body.classList.add('music-layout-resizing');
    event.preventDefault();
  }, [panelSplit, setPanelSplit]);

  return (
    <div className="page-enter music-page" onPointerDownCapture={handleTrustedAudioUnlock}>
      <div ref={layoutShellRef} className="music-layout-shell">
        <div className="music-layout-top" style={{ flex: `${topPanelFlex} 1 0px` }}>
          {tutorialActive ? (
            <TutorialPanel
              keyLabelByMidi={keyLabelByMidi}
              noteOn={noteOn}
              noteOff={noteOff}
              stopAllNotes={stopAll}
            />
          ) : (
            <>
              <InstrumentSelector
                selectedId={selectedInstrument}
                recentInstrumentIds={recentInstrumentIds}
                onSelect={setInstrument}
                onRemoveRecent={removeRecentInstrument}
                loading={loading}
                error={error}
              />

              {loading || error || showMutedHint ? (
                <div className="music-status-stack">
                  {loading ? (
                    <div className="music-status-banner loading">
                      {language === 'zh'
                        ? '正在加载所选乐器的采样…在加载完成前会继续使用内置音色。'
                        : 'Loading samples for the selected instrument. The built-in synth stays active until it is ready.'}
                    </div>
                  ) : null}

                  {error ? (
                    <div className="music-status-banner error">
                      {language === 'zh'
                        ? `所选乐器加载失败，当前已回退到内置音色。原因：${error}`
                        : `The selected instrument failed to load, so playback fell back to the built-in synth. Reason: ${error}`}
                    </div>
                  ) : null}

                  {showMutedHint ? (
                    <div className="music-status-banner warning">
                      {language === 'zh'
                        ? '当前音乐音量为 0。请把滑块调高后再试。'
                        : 'Music volume is currently 0. Raise the slider and try again.'}
                    </div>
                  ) : null}
                </div>
              ) : null}
            </>
          )}
        </div>

        <div
          ref={dividerRef}
          className={`music-pane-divider ${isDividerDragging ? 'dragging' : ''}`}
          onPointerDown={handleDividerPointerDown}
          role="separator"
          aria-orientation="horizontal"
          aria-label={language === 'zh'
            ? '拖动调整乐器区域和键盘区域的高度'
            : 'Drag to resize the instrument and keyboard areas'}
        >
          <label className="music-volume-control">
            <span className="music-volume-label">
              {language === 'zh' ? `音量 ${volume}` : `Volume ${volume}`}
            </span>
            <input
              className="music-volume-slider"
              data-music-resize-exempt
              type="range"
              min={0}
              max={127}
              step={1}
              value={volume}
              onChange={(event) => setVolume(Number(event.currentTarget.value))}
              aria-label={language === 'zh' ? '音乐音量' : 'Music volume'}
            />
          </label>

          <div className="music-pane-divider-grip" aria-hidden="true">
            <span />
            <span />
            <span />
          </div>
        </div>

        <div className="music-layout-bottom" style={{ flex: `${bottomPanelFlex} 1 0px` }}>
          <div className="music-keyboard-shell">
            <div className="music-layout-switcher" data-no-window-drag>
              <div className="music-layout-switcher-group" role="group" aria-label={copy.music.layoutLabel}>
                <span className="music-layout-switcher-label">{copy.music.layoutLabel}</span>
                <div className="music-layout-switcher-group-controls">
                  <button
                    type="button"
                    className={`music-layout-switcher-button ${keyboardLayout === 'classic' ? 'active' : ''}`}
                    onClick={() => setKeyboardLayout('classic')}
                  >
                    {copy.music.classicLayout}
                  </button>
                  <button
                    type="button"
                    className={`music-layout-switcher-button ${keyboardLayout === 'piano' ? 'active' : ''}`}
                    onClick={() => setKeyboardLayout('piano')}
                  >
                    {copy.music.pianoLayout}
                  </button>
                </div>
              </div>

              {keyboardLayout === 'piano' ? (
                <div className="music-octave-groups">
                  <div className="music-octave-group">
                    <span className="music-layout-switcher-label">{copy.music.octaveLabel}</span>
                    <div className="music-octave-group-controls">
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoLayoutOctave(-1)}>−</button>
                      <strong>{pianoLayoutOctave}</strong>
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoLayoutOctave(1)}>+</button>
                    </div>
                  </div>
                  <div className="music-octave-group">
                    <span className="music-layout-switcher-label">{language === 'zh' ? '下排' : 'Lower row'}</span>
                    <div className="music-octave-group-controls">
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoLowerLayoutOctave(-1)}>−</button>
                      <strong>{pianoLowerLayoutOctave}</strong>
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoLowerLayoutOctave(1)}>+</button>
                    </div>
                  </div>
                  <div className="music-octave-group">
                    <span className="music-layout-switcher-label">{language === 'zh' ? '数字排' : 'Number row'}</span>
                    <div className="music-octave-group-controls">
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoNumberLayoutOctave(-1)}>−</button>
                      <strong>{pianoNumberLayoutOctave}</strong>
                      <button type="button" className="music-layout-switcher-button" onClick={() => shiftPianoNumberLayoutOctave(1)}>+</button>
                    </div>
                  </div>
                  <div
                    className="music-layout-switcher-group music-layout-switcher-group--toggle"
                    role="group"
                    aria-label={copy.music.tutorialLabel}
                  >
                    <span className="music-layout-switcher-label">{copy.music.tutorialLabel}</span>
                    <div className="music-layout-switcher-group-controls">
                      <MiniToggle
                        active={tutorialActive}
                        onChange={setTutorialActive}
                        className="music-layout-switcher-toggle"
                        ariaLabel={language === 'zh' ? '切换音游模式' : 'Toggle rhythm game mode'}
                      />
                    </div>
                  </div>
                </div>
              ) : null}
            </div>
            <div className="music-keyboard-stage">
              <div className="music-keyboard-surface">
                {KEYBOARD_LAYOUT_ROWS.map((row, rowIndex) => (
                  <div key={rowIndex} className="music-keyboard-row">
                    {row.map((keyItem) => {
                      const binding = keyBindings[keyItem.id];
                      const successPulseId = binding?.kind === 'note'
                        ? activeSuccessPulseIdsByMidi.get(binding.noteInfo.midi) ?? null
                        : null;

                      return (
                        <MusicKey
                          key={keyItem.id}
                          keyItem={keyItem}
                          binding={binding}
                          pressed={pressedKeyIds.has(keyItem.id)}
                          successPulseId={successPulseId}
                          held={heldKeyIds.has(keyItem.id)}
                          onPointerStart={handlePointerStart}
                          onPointerEnd={handlePointerEnd}
                        />
                      );
                    })}
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
