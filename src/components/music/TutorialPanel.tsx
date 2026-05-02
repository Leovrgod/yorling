import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  TUTORIAL_SONGS,
  getTutorialSongCategory,
  getTutorialSongDifficulty,
  groupTutorialSongsByDifficulty,
} from '../../data/tutorialSongs';
import type { TutorialSong, TutorialSongDifficultyId } from '../../data/tutorialSongTypes';
import {
  useTutorialStore,
  getCurrentSongTime,
  getSelectedSong,
} from '../../stores/tutorialStore';
import { useScoringStore, getAccuracy, isSongComplete } from '../../stores/scoringStore';
import { useAppStore } from '../../stores/appStore';
import { getUiCopy } from '../../i18n/copy';
import { WaterfallCanvas } from './WaterfallCanvas';
import type { AppLanguageId } from '../../types';

const SPEED_OPTIONS = [0.5, 0.75, 1, 1.25] as const;

function speedLabel(speed: number): string {
  return speed === 1 ? '1×' : `${speed}×`;
}

function computeRank(accuracy: number): string {
  if (accuracy >= 98) return 'S';
  if (accuracy >= 90) return 'A';
  if (accuracy >= 80) return 'B';
  if (accuracy >= 60) return 'C';
  return 'D';
}

interface TutorialPanelProps {
  keyLabelByMidi: ReadonlyMap<number, string>;
  noteOn: (midi: number) => void;
  noteOff: (midi: number) => void;
  stopAllNotes: () => void;
}

type BrowserMode = 'library' | 'recent';

interface SongTransitionOptions {
  source?: 'manual' | 'random';
  autoStartMode?: 'play' | 'demo';
  keepRandomChain?: boolean;
}

function songLibraryMetaText(song: TutorialSong, language: AppLanguageId): string {
  return language === 'zh'
    ? `${Math.round(song.bpm)} BPM · ${song.notes.length} 音符`
    : `${Math.round(song.bpm)} BPM · ${song.notes.length} notes`;
}

function songCategoryBadgeText(song: TutorialSong, language: AppLanguageId): string | null {
  if (song.categoryId === 'featured-favorites') {
    return null;
  }

  return getTutorialSongCategory(song.categoryId).label[language];
}

function matchesSongQuery(song: TutorialSong, normalizedQuery: string): boolean {
  if (!normalizedQuery) {
    return true;
  }

  const category = getTutorialSongCategory(song.categoryId);
  const difficulty = getTutorialSongDifficulty(song.difficultyId);
  const haystack = [
    song.title.zh,
    song.title.en,
    category.label.zh,
    category.label.en,
    difficulty.label.zh,
    difficulty.label.en,
    song.source,
  ].join(' ').toLowerCase();

  return haystack.includes(normalizedQuery);
}

const TutorialPanelComponent = ({ keyLabelByMidi, noteOn, noteOff, stopAllNotes }: TutorialPanelProps) => {
  const language = useAppStore((state) => state.language) as AppLanguageId;
  const theme = useAppStore((state) => state.theme);
  const copy = getUiCopy(language);
  const songId = useTutorialStore((state) => state.songId);
  const recentSongIds = useTutorialStore((state) => state.recentSongIds);
  const isPlaying = useTutorialStore((state) => state.isPlaying);
  const isDemoPlaying = useTutorialStore((state) => state.isDemoPlaying);
  const speed = useTutorialStore((state) => state.speed);
  const selectSong = useTutorialStore((state) => state.selectSong);
  const removeRecentSong = useTutorialStore((state) => state.removeRecentSong);
  const play = useTutorialStore((state) => state.play);
  const pause = useTutorialStore((state) => state.pause);
  const stop = useTutorialStore((state) => state.stop);
  const setSpeed = useTutorialStore((state) => state.setSpeed);
  const setExternalCurrentTime = useTutorialStore((state) => state.setExternalCurrentTime);
  const startDemo = useTutorialStore((state) => state.startDemo);
  const stopDemo = useTutorialStore((state) => state.stopDemo);

  const perfect = useScoringStore((state) => state.perfect);
  const good = useScoringStore((state) => state.good);
  const miss = useScoringStore((state) => state.miss);
  const combo = useScoringStore((state) => state.combo);
  const maxCombo = useScoringStore((state) => state.maxCombo);
  const judgedCount = useScoringStore((state) => state.judgedIndices.size);
  const activeHoldCount = useScoringStore((state) => state.activeHolds.length);
  const scoringEnabled = useScoringStore((state) => state.enabled);
  const enableScoring = useScoringStore((state) => state.enable);
  const disableScoring = useScoringStore((state) => state.disable);
  const resetScoring = useScoringStore((state) => state.reset);

  const [browserMode, setBrowserMode] = useState<BrowserMode | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [activeDifficultyJumpId, setActiveDifficultyJumpId] = useState<TutorialSongDifficultyId | null>(null);
  const [activeRecentDifficultyJumpId, setActiveRecentDifficultyJumpId] = useState<TutorialSongDifficultyId | null>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const audioFrameRef = useRef<number | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const libraryBrowserBodyRef = useRef<HTMLDivElement>(null);
  const recentBrowserBodyRef = useRef<HTMLDivElement>(null);
  const difficultySectionRefs = useRef<Partial<Record<TutorialSongDifficultyId, HTMLElement | null>>>({});
  const recentDifficultySectionRefs = useRef<Partial<Record<TutorialSongDifficultyId, HTMLElement | null>>>({});
  const demoScheduledTimersRef = useRef<ReturnType<typeof setTimeout>[]>([]);
  const demoActiveNotesRef = useRef<Set<number>>(new Set());

  const allSongs = TUTORIAL_SONGS;

  const song = useMemo(() => {
    return getSelectedSong(songId);
  }, [songId]);
  const recentSongs = useMemo(() => {
    const songsById = new Map(allSongs.map((entry) => [entry.id, entry] as const));
    return recentSongIds
      .map((id) => songsById.get(id))
      .filter((entry): entry is TutorialSong => entry !== undefined);
  }, [allSongs, recentSongIds]);
  const normalizedSearchQuery = searchQuery.trim().toLowerCase();
  const visibleCatalogSongs = useMemo(
    () => allSongs.filter((entry) => matchesSongQuery(entry, normalizedSearchQuery)),
    [allSongs, normalizedSearchQuery],
  );
  const groupedCatalogSongs = useMemo(
    () => groupTutorialSongsByDifficulty(visibleCatalogSongs),
    [visibleCatalogSongs],
  );
  const groupedRecentSongs = useMemo(
    () => groupTutorialSongsByDifficulty(recentSongs),
    [recentSongs],
  );
  const recentSongIdSet = useMemo(() => new Set(recentSongs.map((entry) => entry.id)), [recentSongs]);

  const accuracy = getAccuracy({ perfect, good, miss });
  const songComplete = isSongComplete(judgedCount + activeHoldCount, song.notes.length);

  // Auto-finish: when all notes are judged, wait a brief period then show results
  const [resultsVisible, setResultsVisible] = useState(false);
  const autoFinishTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [showRandomChainControl, setShowRandomChainControl] = useState(false);
  const [continuousRandomEnabled, setContinuousRandomEnabled] = useState(false);
  const pendingAutoStartModeRef = useRef<'play' | 'demo' | null>(null);

  const dismissResults = useCallback(() => {
    setResultsVisible(false);
  }, []);

  const clearDemoTimers = useCallback(() => {
    for (const timer of demoScheduledTimersRef.current) {
      clearTimeout(timer);
    }
    demoScheduledTimersRef.current = [];
    for (const midi of demoActiveNotesRef.current) {
      noteOff(midi);
    }
    demoActiveNotesRef.current.clear();
  }, [noteOff]);

  // Cleanup demo timers on unmount
  useEffect(() => {
    return () => {
      clearDemoTimers();
    };
  }, [clearDemoTimers]);

  useEffect(() => {
    // Clear timer on song change or reset
    return () => {
      if (autoFinishTimerRef.current !== null) {
        clearTimeout(autoFinishTimerRef.current);
        autoFinishTimerRef.current = null;
      }
    };
  }, [songId]);

  useEffect(() => {
    if (isPlaying) {
      setBrowserMode(null);
      return;
    }

    if (browserMode !== 'library') {
      return;
    }

    const frameId = requestAnimationFrame(() => {
      searchInputRef.current?.focus();
    });
    return () => cancelAnimationFrame(frameId);
  }, [browserMode, isPlaying]);

  useEffect(() => {
    if (groupedCatalogSongs.length === 0) {
      setActiveDifficultyJumpId(null);
      return;
    }

    const nextActiveDifficultyId = groupedCatalogSongs.some(
      ({ difficulty }) => difficulty.id === song.difficultyId,
    )
      ? song.difficultyId
      : groupedCatalogSongs[0]?.difficulty.id ?? null;

    setActiveDifficultyJumpId(nextActiveDifficultyId);
  }, [groupedCatalogSongs, song.difficultyId]);

  useEffect(() => {
    if (groupedRecentSongs.length === 0) {
      setActiveRecentDifficultyJumpId(null);
      return;
    }

    const nextActiveDifficultyId = groupedRecentSongs.some(
      ({ difficulty }) => difficulty.id === song.difficultyId,
    )
      ? song.difficultyId
      : groupedRecentSongs[0]?.difficulty.id ?? null;

    setActiveRecentDifficultyJumpId(nextActiveDifficultyId);
  }, [groupedRecentSongs, song.difficultyId]);

  useEffect(() => {
    if (browserMode !== 'library' || isPlaying) {
      return;
    }

    const root = libraryBrowserBodyRef.current;
    if (!root) {
      return;
    }

    const updateActiveDifficultyFromScroll = () => {
      const rootTop = root.getBoundingClientRect().top;
      const visibleSections = groupedCatalogSongs.flatMap(({ difficulty }) => {
        const section = difficultySectionRefs.current[difficulty.id];
        if (!section) {
          return [];
        }

        const distance = section.getBoundingClientRect().top - rootTop;
        return distance <= 24 ? [{ difficultyId: difficulty.id, distance: Math.abs(distance) }] : [];
      });

      const nextDifficultyId = visibleSections.sort((left, right) => left.distance - right.distance)[0]?.difficultyId
        ?? groupedCatalogSongs[0]?.difficulty.id
        ?? null;

      setActiveDifficultyJumpId(nextDifficultyId);
    };

    const libraryDifficultyObserver = new IntersectionObserver(() => {
      updateActiveDifficultyFromScroll();
    }, {
      root,
      threshold: [0, 0.25, 0.5, 0.75, 1],
    });

    root.addEventListener('scroll', updateActiveDifficultyFromScroll, { passive: true });

    for (const { difficulty } of groupedCatalogSongs) {
      const section = difficultySectionRefs.current[difficulty.id];
      if (section) {
        libraryDifficultyObserver.observe(section);
      }
    }

    requestAnimationFrame(updateActiveDifficultyFromScroll);

    return () => {
      root.removeEventListener('scroll', updateActiveDifficultyFromScroll);
      libraryDifficultyObserver.disconnect();
    };
  }, [browserMode, groupedCatalogSongs, isPlaying]);

  useEffect(() => {
    if (browserMode !== 'recent' || isPlaying) {
      return;
    }

    const root = recentBrowserBodyRef.current;
    if (!root) {
      return;
    }

    const updateActiveRecentDifficultyFromScroll = () => {
      const rootTop = root.getBoundingClientRect().top;
      const visibleSections = groupedRecentSongs.flatMap(({ difficulty }) => {
        const section = recentDifficultySectionRefs.current[difficulty.id];
        if (!section) {
          return [];
        }

        const distance = section.getBoundingClientRect().top - rootTop;
        return distance <= 24 ? [{ difficultyId: difficulty.id, distance: Math.abs(distance) }] : [];
      });

      const nextDifficultyId = visibleSections.sort((left, right) => left.distance - right.distance)[0]?.difficultyId
        ?? groupedRecentSongs[0]?.difficulty.id
        ?? null;

      setActiveRecentDifficultyJumpId(nextDifficultyId);
    };

    const recentDifficultyObserver = new IntersectionObserver(() => {
      updateActiveRecentDifficultyFromScroll();
    }, {
      root,
      threshold: [0, 0.25, 0.5, 0.75, 1],
    });

    root.addEventListener('scroll', updateActiveRecentDifficultyFromScroll, { passive: true });

    for (const { difficulty } of groupedRecentSongs) {
      const section = recentDifficultySectionRefs.current[difficulty.id];
      if (section) {
        recentDifficultyObserver.observe(section);
      }
    }

    requestAnimationFrame(updateActiveRecentDifficultyFromScroll);

    return () => {
      root.removeEventListener('scroll', updateActiveRecentDifficultyFromScroll);
      recentDifficultyObserver.disconnect();
    };
  }, [browserMode, groupedRecentSongs, isPlaying]);

  const showResults = resultsVisible && scoringEnabled && judgedCount > 0;

  const getCurrentTime = useCallback(() => {
    return getCurrentSongTime(useTutorialStore.getState());
  }, []);

  const stopAudioSync = useCallback(() => {
    if (audioFrameRef.current !== null) {
      cancelAnimationFrame(audioFrameRef.current);
      audioFrameRef.current = null;
    }
  }, []);

  const syncAudioTime = useCallback(() => {
    const audio = audioRef.current;
    if (!audio) return;

    setExternalCurrentTime(audio.currentTime);
    audioFrameRef.current = requestAnimationFrame(syncAudioTime);
  }, [setExternalCurrentTime]);

  const audioLeadInRef = useRef<number | null>(null);

  const stopAudioLeadIn = useCallback(() => {
    if (audioLeadInRef.current !== null) {
      cancelAnimationFrame(audioLeadInRef.current);
      audioLeadInRef.current = null;
    }
  }, []);

  const resetPlaybackState = useCallback(() => {
    disableScoring();
    stopAudioSync();
    stopAudioLeadIn();
  }, [disableScoring, stopAudioLeadIn, stopAudioSync]);

  const pickRandomSong = useCallback((excludingSongId: string): TutorialSong | null => {
    if (allSongs.length === 0) {
      return null;
    }

    const candidates = allSongs.filter((entry) => entry.id !== excludingSongId);
    return candidates[Math.floor(Math.random() * candidates.length)] ?? allSongs[0] ?? null;
  }, [allSongs]);

  const changeSong = useCallback((
    nextSongId: string,
    {
      source = 'manual',
      autoStartMode,
      keepRandomChain = false,
    }: SongTransitionOptions = {},
  ) => {
    if (autoFinishTimerRef.current !== null) {
      clearTimeout(autoFinishTimerRef.current);
      autoFinishTimerRef.current = null;
    }

    if (isDemoPlaying) {
      clearDemoTimers();
      stopDemo();
    }

    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
    }

    stopAllNotes();
    stop();
    resetPlaybackState();
    selectSong(nextSongId);
    resetScoring();
    dismissResults();
    setBrowserMode(null);
    setSearchQuery('');
    setShowRandomChainControl(source === 'random' || keepRandomChain);
    setContinuousRandomEnabled(keepRandomChain);
    pendingAutoStartModeRef.current = autoStartMode ?? null;
  }, [
    clearDemoTimers,
    dismissResults,
    isDemoPlaying,
    resetPlaybackState,
    resetScoring,
    selectSong,
    stop,
    stopAllNotes,
    stopDemo,
  ]);

  const continueWithRandomSong = useCallback((autoStartMode: 'play' | 'demo') => {
    const nextSong = pickRandomSong(songId);
    if (!nextSong) {
      return;
    }

    changeSong(nextSong.id, {
      source: 'random',
      autoStartMode,
      keepRandomChain: true,
    });
  }, [changeSong, pickRandomSong, songId]);

  // Demo playback: schedule all notes as noteOn/noteOff using the audio engine
  useEffect(() => {
    if (!isDemoPlaying) {
      return;
    }

    clearDemoTimers();

    const notes = song.notes;
    const currentSpeed = useTutorialStore.getState().speed;
    const LEAD_IN_MS = 2000;
    const timers: ReturnType<typeof setTimeout>[] = [];

    for (const note of notes) {
      const onDelay = LEAD_IN_MS + (note.time / currentSpeed) * 1000;
      const offDelay = onDelay + (note.duration / currentSpeed) * 1000;

      timers.push(setTimeout(() => {
        noteOn(note.midi);
        demoActiveNotesRef.current.add(note.midi);
      }, onDelay));

      timers.push(setTimeout(() => {
        noteOff(note.midi);
        demoActiveNotesRef.current.delete(note.midi);
      }, offDelay));
    }

    const songEndDelay = LEAD_IN_MS + (song.duration / currentSpeed) * 1000 + 500;
    timers.push(setTimeout(() => {
      if (showRandomChainControl && continuousRandomEnabled) {
        continueWithRandomSong('demo');
        return;
      }

      stopDemo();
      stopAllNotes();
    }, songEndDelay));

    demoScheduledTimersRef.current = timers;

    return () => {
      clearDemoTimers();
    };
  }, [
    clearDemoTimers,
    continueWithRandomSong,
    continuousRandomEnabled,
    isDemoPlaying,
    noteOff,
    noteOn,
    showRandomChainControl,
    song,
    stopAllNotes,
    stopDemo,
  ]);

  // Auto-finish: after all notes are judged, wait briefly then show results
  useEffect(() => {
    if (songComplete && isPlaying && !isDemoPlaying && scoringEnabled && judgedCount > 0 && !resultsVisible) {
      autoFinishTimerRef.current = setTimeout(() => {
        const audio = audioRef.current;
        if (audio && song.audioUrl) {
          audio.pause();
        }
        pause(song.audioUrl ? audioRef.current?.currentTime : undefined);
        stopAudioSync();
        stopAudioLeadIn();
        autoFinishTimerRef.current = null;

        if (showRandomChainControl && continuousRandomEnabled) {
          continueWithRandomSong('play');
          return;
        }

        setResultsVisible(true);
      }, showRandomChainControl && continuousRandomEnabled ? 700 : 2000);

      return () => {
        if (autoFinishTimerRef.current !== null) {
          clearTimeout(autoFinishTimerRef.current);
          autoFinishTimerRef.current = null;
        }
      };
    }
  }, [
    continueWithRandomSong,
    continuousRandomEnabled,
    isDemoPlaying,
    isPlaying,
    judgedCount,
    pause,
    resultsVisible,
    scoringEnabled,
    showRandomChainControl,
    song.audioUrl,
    songComplete,
    stopAudioLeadIn,
    stopAudioSync,
  ]);

  const handleSongSelect = useCallback((nextSongId: string) => {
    changeSong(nextSongId);
  }, [changeSong]);

  const handleDifficultyJump = useCallback((difficultyId: TutorialSongDifficultyId) => {
    const section = difficultySectionRefs.current[difficultyId];
    if (!section) {
      return;
    }

    setActiveDifficultyJumpId(difficultyId);
    section.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, []);

  const handleRecentDifficultyJump = useCallback((difficultyId: TutorialSongDifficultyId) => {
    const section = recentDifficultySectionRefs.current[difficultyId];
    if (!section) {
      return;
    }

    setActiveRecentDifficultyJumpId(difficultyId);
    section.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }, []);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;

    if (!song.audioUrl) {
      stopAudioSync();
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
      setExternalCurrentTime(null);
      return;
    }

    stopAudioSync();
    audio.pause();
    audio.src = song.audioUrl;
    audio.currentTime = 0;
    audio.playbackRate = speed;
    audio.load();
    setExternalCurrentTime(0);

    const handleEnded = () => {
      // If auto-finish timer is pending, let it handle the transition
      if (autoFinishTimerRef.current !== null) return;
      if (showRandomChainControl && continuousRandomEnabled) {
        continueWithRandomSong(isDemoPlaying ? 'demo' : 'play');
        return;
      }
      stop();
      resetPlaybackState();
      setExternalCurrentTime(song.duration);
    };

    audio.addEventListener('ended', handleEnded);

    return () => {
      audio.removeEventListener('ended', handleEnded);
      audio.pause();
      stopAudioSync();
    };
  }, [
    continueWithRandomSong,
    continuousRandomEnabled,
    isDemoPlaying,
    resetPlaybackState,
    setExternalCurrentTime,
    showRandomChainControl,
    song.audioUrl,
    song.duration,
    song.id,
    speed,
    stop,
    stopAudioSync,
  ]);

  useEffect(() => {
    const pendingAutoStartMode = pendingAutoStartModeRef.current;
    if (pendingAutoStartMode === null) {
      return;
    }

    pendingAutoStartModeRef.current = null;
    const timerId = window.setTimeout(() => {
      if (pendingAutoStartMode === 'demo') {
        startDemo();
        return;
      }

      resetScoring();
      enableScoring();
      play();
    }, 20);

    return () => {
      window.clearTimeout(timerId);
    };
  }, [enableScoring, play, resetScoring, song.id, startDemo]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !song.audioUrl) return;
    audio.playbackRate = speed;
  }, [song.audioUrl, speed]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !song.audioUrl) return;

    if (isPlaying) {
      const targetTime = getCurrentSongTime(useTutorialStore.getState());

      // During lead-in (negative song time), poll until song time >= 0 then start audio
      if (targetTime < 0) {
        audio.pause();
        audio.currentTime = 0;
        // Clear externalCurrentTime so the internal clock drives timing during lead-in
        setExternalCurrentTime(null);

        const pollLeadIn = () => {
          const now = getCurrentSongTime(useTutorialStore.getState());
          if (now >= 0) {
            audio.currentTime = now;
            const playback = audio.play();
            if (playback && typeof playback.then === 'function') {
              void playback.then(() => {
                stopAudioSync();
                syncAudioTime();
              }).catch((error) => {
                console.error('Failed to play imported tutorial audio.', error);
                pause(audio.currentTime);
                disableScoring();
              });
            } else {
              stopAudioSync();
              syncAudioTime();
            }
            audioLeadInRef.current = null;
            return;
          }
          audioLeadInRef.current = requestAnimationFrame(pollLeadIn);
        };
        audioLeadInRef.current = requestAnimationFrame(pollLeadIn);

        return () => {
          stopAudioLeadIn();
        };
      }

      const safeTargetTime = Number.isFinite(audio.duration) && audio.duration > 0
        ? Math.min(targetTime, audio.duration)
        : targetTime;

      if (Math.abs(audio.currentTime - safeTargetTime) > 0.05) {
        audio.currentTime = safeTargetTime;
      }

      const playback = audio.play();
      if (playback && typeof playback.then === 'function') {
        void playback.then(() => {
          stopAudioSync();
          syncAudioTime();
        }).catch((error) => {
          console.error('Failed to play imported tutorial audio.', error);
          pause(audio.currentTime);
          disableScoring();
        });
      } else {
        stopAudioSync();
        syncAudioTime();
      }

      return () => {
        stopAudioSync();
        stopAudioLeadIn();
      };
    }

    stopAudioLeadIn();
    audio.pause();
    stopAudioSync();
    setExternalCurrentTime(audio.currentTime);
  }, [disableScoring, isPlaying, pause, setExternalCurrentTime, song.audioUrl, stopAudioLeadIn, stopAudioSync, syncAudioTime]);

  useEffect(() => {
    return () => {
      stopAudioSync();
      setExternalCurrentTime(null);
    };
  }, [setExternalCurrentTime, stopAudioSync]);

  const handlePlayPause = useCallback(() => {
    // If demo is playing, stop it first
    if (isDemoPlaying) {
      clearDemoTimers();
      stopDemo();
      stopAllNotes();
      if (song.audioUrl && audioRef.current) {
        audioRef.current.pause();
        audioRef.current.currentTime = 0;
      }
      return;
    }

    if (isPlaying) {
      pause(song.audioUrl ? audioRef.current?.currentTime : undefined);
      disableScoring();
      return;
    }

    const state = useTutorialStore.getState();
    const currentTime = song.audioUrl
      ? audioRef.current?.currentTime ?? getCurrentSongTime(state)
      : getCurrentSongTime(state);

    if (currentTime >= song.duration) {
      stop();
      resetPlaybackState();
      resetScoring();
      if (song.audioUrl && audioRef.current) {
        audioRef.current.currentTime = 0;
      }
      setTimeout(() => {
        enableScoring();
        play();
      }, 20);
      return;
    }

    const isFreshStart = currentTime < 0;
    if (isFreshStart) {
      resetScoring();
    }
    enableScoring();
    play();
  }, [isDemoPlaying, clearDemoTimers, stopDemo, stopAllNotes, disableScoring, enableScoring, isPlaying, pause, play, resetPlaybackState, resetScoring, song.audioUrl, song.duration, stop]);

  const handleStop = useCallback(() => {
    if (isDemoPlaying) {
      clearDemoTimers();
      stopAllNotes();
    }
    if (song.audioUrl && audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
    }
    stop();
    resetPlaybackState();
    resetScoring();
    dismissResults();
  }, [isDemoPlaying, clearDemoTimers, stopAllNotes, dismissResults, resetPlaybackState, resetScoring, song.audioUrl, stop]);

  const handlePlayAgain = useCallback(() => {
    if (song.audioUrl && audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
    }
    stop();
    resetPlaybackState();
    resetScoring();
    dismissResults();
    setTimeout(() => {
      enableScoring();
      play();
    }, 20);
  }, [dismissResults, enableScoring, play, resetPlaybackState, resetScoring, song.audioUrl, stop]);

  const handleChangeSong = useCallback(() => {
    if (song.audioUrl && audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
    }
    stop();
    resetPlaybackState();
    resetScoring();
    dismissResults();
    setShowRandomChainControl(false);
    setContinuousRandomEnabled(false);
    setBrowserMode('library');
    setSearchQuery('');
  }, [dismissResults, resetPlaybackState, resetScoring, song.audioUrl, stop]);

  const handleRandomSong = useCallback(() => {
    const randomSong = pickRandomSong(songId);
    if (!randomSong) {
      return;
    }

    changeSong(randomSong.id, {
      source: 'random',
      keepRandomChain: continuousRandomEnabled,
    });
  }, [changeSong, continuousRandomEnabled, pickRandomSong, songId]);

  const handleDemoToggle = useCallback(() => {
    if (isDemoPlaying) {
      clearDemoTimers();
      stopDemo();
      stopAllNotes();
      if (song.audioUrl && audioRef.current) {
        audioRef.current.pause();
        audioRef.current.currentTime = 0;
      }
      return;
    }

    // Stop normal playback first
    if (isPlaying) {
      if (song.audioUrl && audioRef.current) {
        audioRef.current.pause();
        audioRef.current.currentTime = 0;
      }
      stop();
      resetPlaybackState();
    }
    resetScoring();
    dismissResults();
    setBrowserMode(null);
    startDemo();
  }, [isDemoPlaying, isPlaying, clearDemoTimers, stopDemo, stopAllNotes, song.audioUrl, stop, resetPlaybackState, resetScoring, dismissResults, startDemo]);

  const zh = language === 'zh';
  const showRecentWindow = browserMode === 'recent' && !isPlaying;
  const showLibraryWindow = browserMode === 'library' && !isPlaying;
  const showRandomChainButton = showRandomChainControl || continuousRandomEnabled;
  const recentSongCount = recentSongs.length;
  const totalSongCount = allSongs.length;
  const resultCountLabel = zh
    ? `匹配 ${visibleCatalogSongs.length} 首`
    : `${visibleCatalogSongs.length} matches`;

  const toggleBrowserMode = useCallback((nextMode: BrowserMode) => {
    dismissResults();
    setBrowserMode((current) => (current === nextMode ? null : nextMode));
  }, [dismissResults]);

  return (
    <div className="tutorial-panel">
      <div className="tutorial-controls" data-no-window-drag>
        <div className="tutorial-toolbar-main">
          <div
            className="tutorial-btn tutorial-btn-pill tutorial-current-song"
            title={song.title[language]}
            aria-label={zh ? `当前曲目 ${song.title[language]}` : `Selected song ${song.title[language]}`}
          >
            <span className="tutorial-current-song-label">
              {zh ? '当前曲目' : 'Selected song'}
            </span>
            <strong className="tutorial-current-song-title">{song.title[language]}</strong>
          </div>

          <div className="tutorial-toolbar-actions">
            <button
              type="button"
              className={`tutorial-btn tutorial-btn-pill tutorial-browser-open-btn tutorial-recent-open-btn ${showRecentWindow ? 'active' : ''}`}
              onClick={() => toggleBrowserMode('recent')}
              aria-label={copy.music.recentSongsTitle}
              title={copy.music.recentSongsTitle}
              aria-expanded={showRecentWindow}
              disabled={isPlaying}
            >
              <span className="tutorial-btn-glyph" aria-hidden="true">↺</span>
              <span className="tutorial-browser-open-copy">
                <span className="tutorial-browser-open-label">{copy.music.recentSongsTitle}</span>
                <span className="tutorial-browser-open-meta">
                  {zh ? `${recentSongCount} 首` : `${recentSongCount} songs`}
                </span>
              </span>
            </button>

            <button
              type="button"
              className={`tutorial-btn tutorial-btn-pill tutorial-browser-open-btn tutorial-library-open-btn ${showLibraryWindow ? 'active' : ''}`}
              onClick={() => toggleBrowserMode('library')}
              aria-label={copy.music.libraryTitle}
              title={copy.music.libraryTitle}
              aria-expanded={showLibraryWindow}
              disabled={isPlaying}
            >
              <span className="tutorial-btn-glyph" aria-hidden="true">⌕</span>
              <span className="tutorial-browser-open-copy">
                <span className="tutorial-browser-open-label">{copy.music.libraryTitle}</span>
                <span className="tutorial-browser-open-meta">
                  {zh ? `${totalSongCount} 首` : `${totalSongCount} songs`}
                </span>
              </span>
            </button>

            <div className="tutorial-extra-actions">
              <button
                type="button"
                className="tutorial-btn tutorial-btn-pill tutorial-random-btn"
                onClick={handleRandomSong}
                aria-label={copy.music.randomSong}
                title={copy.music.randomSong}
                disabled={isPlaying && !isDemoPlaying}
              >
                <svg className="tutorial-btn-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <polyline points="16 3 21 3 21 8" />
                  <line x1="4" y1="20" x2="21" y2="3" />
                  <polyline points="21 16 21 21 16 21" />
                  <line x1="15" y1="15" x2="21" y2="21" />
                  <line x1="4" y1="4" x2="9" y2="9" />
                </svg>
                <span className="tutorial-extra-label">{copy.music.randomSong}</span>
              </button>

              {showRandomChainButton ? (
                <button
                  type="button"
                  className={`tutorial-btn tutorial-btn-pill tutorial-random-chain-btn ${continuousRandomEnabled ? 'active' : ''}`}
                  onClick={() => setContinuousRandomEnabled((current) => !current)}
                  aria-pressed={continuousRandomEnabled}
                  title={copy.music.randomChain}
                >
                  <span className="tutorial-random-chain-glyph" aria-hidden="true">∞</span>
                  <span className="tutorial-extra-label">{copy.music.randomChain}</span>
                </button>
              ) : null}

              <button
                type="button"
                className={`tutorial-btn tutorial-btn-pill tutorial-demo-btn ${isDemoPlaying ? 'active' : ''}`}
                onClick={handleDemoToggle}
                aria-label={isDemoPlaying ? copy.music.demoStop : copy.music.demoPlay}
                title={isDemoPlaying ? copy.music.demoStop : copy.music.demoPlay}
              >
                {isDemoPlaying ? (
                  <svg className="tutorial-btn-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <rect x="6" y="6" width="12" height="12" rx="1.5" />
                  </svg>
                ) : (
                  <svg className="tutorial-btn-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                    <path d="M3 18v-6a9 9 0 0 1 18 0v6" />
                    <path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3v5z" />
                    <path d="M3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3v5z" />
                  </svg>
                )}
                <span className="tutorial-extra-label">
                  {isDemoPlaying ? copy.music.demoPlaying : copy.music.demoPlay}
                </span>
              </button>
            </div>

            <div className="tutorial-control-group tutorial-transport-group">
              <button
                type="button"
                className={`tutorial-btn tutorial-btn-icon-only ${isPlaying ? 'playing' : ''}`}
                onClick={handlePlayPause}
                aria-label={isPlaying ? (zh ? '暂停' : 'Pause') : (zh ? '播放' : 'Play')}
              >
                {isPlaying ? '⏸' : '▶'}
              </button>
              <button
                type="button"
                className="tutorial-btn tutorial-btn-icon-only"
                onClick={handleStop}
                aria-label={zh ? '停止' : 'Stop'}
              >
                ⏹
              </button>
            </div>

            <div className="tutorial-control-group tutorial-speed-cluster">
              <span className="tutorial-control-label">
                {zh ? '速度' : 'Speed'}
              </span>
              <div className="tutorial-speed-group">
                {SPEED_OPTIONS.map((value) => (
                  <button
                    key={value}
                    type="button"
                    className={`tutorial-speed-btn ${speed === value ? 'active' : ''}`}
                    onClick={() => setSpeed(value)}
                  >
                    {speedLabel(value)}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>

        <div className="tutorial-toolbar-actions tutorial-toolbar-meta">
          <span className="tutorial-bpm">
            {Math.round(song.bpm * speed)} BPM
          </span>

          {scoringEnabled && isPlaying && !isDemoPlaying ? (
            <div className="tutorial-live-score">
              <span className="score-combo" data-combo={combo > 0}>
                {combo > 2 ? `${combo} ${copy.music.scoreCombo}` : ''}
              </span>
              <span className="score-accuracy">{accuracy}%</span>
            </div>
          ) : null}

          {isDemoPlaying ? (
            <span className="tutorial-demo-indicator">
              {copy.music.demoPlaying}
            </span>
          ) : null}
        </div>
      </div>

      <audio ref={audioRef} className="tutorial-audio-element" preload="auto" />

      <div className="tutorial-browser-shell">
        <div className="tutorial-stage-shell">
          <div className="tutorial-song-stage">
            <WaterfallCanvas
              song={song}
              getCurrentTime={getCurrentTime}
              isPlaying={isPlaying}
              isDark={theme === 'dark'}
              keyLabelByMidi={keyLabelByMidi}
            />

            {showResults ? (
              <div className="tutorial-results-stage-overlay" data-no-window-drag>
                <div className="tutorial-results-card">
                  <div className="tutorial-results-header">
                    <span className="tutorial-results-song-title">
                      {song.title[language]}
                    </span>
                    <span className="tutorial-results-rank" data-rank={computeRank(accuracy)}>
                      {computeRank(accuracy)}
                    </span>
                  </div>

                  <div className="tutorial-results-accuracy-bar">
                    <div className="tutorial-results-accuracy-fill" style={{ width: `${accuracy}%` }} />
                  </div>
                  <div className="tutorial-results-accuracy-label">
                    {copy.music.scoreAccuracy} <strong>{accuracy}%</strong>
                  </div>

                  <div className="tutorial-results-stats">
                    <div className="tutorial-results-stat">
                      <span className="stat-value perfect">{perfect}</span>
                      <span className="stat-label">{copy.music.scorePerfect}</span>
                    </div>
                    <div className="tutorial-results-stat">
                      <span className="stat-value good">{good}</span>
                      <span className="stat-label">{copy.music.scoreGood}</span>
                    </div>
                    <div className="tutorial-results-stat">
                      <span className="stat-value miss">{miss}</span>
                      <span className="stat-label">{copy.music.scoreMiss}</span>
                    </div>
                    <div className="tutorial-results-stat">
                      <span className="stat-value">{maxCombo}</span>
                      <span className="stat-label">{copy.music.scoreMaxCombo}</span>
                    </div>
                  </div>

                  <div className="tutorial-results-actions">
                    <button
                      type="button"
                      className="tutorial-results-btn tutorial-results-btn-primary"
                      onClick={handlePlayAgain}
                    >
                      {copy.music.scorePlayAgain}
                    </button>
                    <button
                      type="button"
                      className="tutorial-results-btn tutorial-results-btn-secondary"
                      onClick={handleChangeSong}
                    >
                      {copy.music.scoreChangeSong}
                    </button>
                  </div>
                </div>
              </div>
            ) : null}
          </div>

          {showRecentWindow ? (
            <div className="tutorial-browser-overlay" data-no-window-drag>
              <section className="tutorial-browser-window tutorial-recent-window">
                <div className="tutorial-browser-header">
                  <div className="tutorial-browser-title-group">
                    <span className="tutorial-control-label">{copy.music.recentSongsTitle}</span>
                    <span className="tutorial-browser-summary">
                      {zh
                        ? `${recentSongCount} 首亲手弹过的曲目 · 保留难度分组与风格标签`
                        : `${recentSongCount} hand-played songs · grouped by difficulty with style tags`}
                    </span>
                  </div>

                  <button
                    type="button"
                    className="tutorial-btn tutorial-btn-icon-only tutorial-browser-close-btn"
                    onClick={() => setBrowserMode(null)}
                    aria-label={copy.music.closeSongBrowser}
                    title={copy.music.closeSongBrowser}
                  >
                    ×
                  </button>
                </div>

                {groupedRecentSongs.length ? (
                  <nav
                    className="tutorial-difficulty-jump-nav tutorial-recent-jump-nav"
                    aria-label={zh ? '最近曲目按难度跳转' : 'Jump recent songs by difficulty'}
                  >
                    {groupedRecentSongs.map(({ difficulty, songs }) => {
                      const isActive = activeRecentDifficultyJumpId === difficulty.id;

                      return (
                        <button
                          key={difficulty.id}
                          type="button"
                          className={`tutorial-difficulty-jump-chip${isActive ? ' active' : ''}`}
                          onClick={() => handleRecentDifficultyJump(difficulty.id)}
                          aria-pressed={isActive}
                        >
                          <span>{difficulty.label[language]}</span>
                          <span className="tutorial-difficulty-jump-count">{songs.length}</span>
                        </button>
                      );
                    })}
                  </nav>
                ) : null}

                <div ref={recentBrowserBodyRef} className="tutorial-browser-body">
                  <section className="tutorial-browser-section">
                    {groupedRecentSongs.length ? (
                      groupedRecentSongs.map(({ difficulty, songs }) => (
                        <section
                          key={difficulty.id}
                          ref={(section) => {
                            recentDifficultySectionRefs.current[difficulty.id] = section;
                          }}
                          className="tutorial-recent-section"
                        >
                          <div className="tutorial-recent-section-heading">
                            <span className="tutorial-recent-section-title">
                              {difficulty.label[language]}
                            </span>
                            <span className="tutorial-recent-section-count">{songs.length}</span>
                          </div>

                          <div className="tutorial-recent-grid">
                            {songs.map((entry) => {
                              const isSelected = entry.id === songId;
                              const categoryLabel = songCategoryBadgeText(entry, language);

                              return (
                                <div key={entry.id} className="tutorial-recent-item">
                                  <button
                                    type="button"
                                    className={`tutorial-recent-button${isSelected ? ' selected' : ''}`}
                                    onClick={() => handleSongSelect(entry.id)}
                                    title={entry.title.en}
                                  >
                                    <span className="tutorial-recent-copy">
                                      <span className="tutorial-recent-label">{entry.title[language]}</span>
                                      {categoryLabel ? (
                                        <span className="tutorial-recent-meta">{categoryLabel}</span>
                                      ) : null}
                                    </span>
                                  </button>

                                  <button
                                    type="button"
                                    className="tutorial-recent-remove"
                                    onClick={() => removeRecentSong(entry.id)}
                                    title={zh ? `移除 ${entry.title[language]}` : `Remove ${entry.title.en}`}
                                    aria-label={zh
                                      ? `从最近曲目中移除 ${entry.title[language]}`
                                      : `Remove ${entry.title.en} from recent songs`}
                                  >
                                    ×
                                  </button>
                                </div>
                              );
                            })}
                          </div>
                        </section>
                      ))
                    ) : (
                      <div className="tutorial-recent-empty">
                        <span className="tutorial-recent-empty-title">
                          {zh ? '还没有最近练过的乐曲' : 'No recent songs yet'}
                        </span>
                        <span className="tutorial-recent-empty-copy">
                          {zh
                            ? '只有亲手弹过的曲目才会出现在这里，并按难度保留风格标签，方便快速回到刚才练过的旋律。'
                            : 'Only songs you actually played by hand appear here, grouped by difficulty with style tags for quick practice jumps.'}
                        </span>
                      </div>
                    )}
                  </section>
                </div>
              </section>
            </div>
          ) : null}

          {showLibraryWindow ? (
            <div className="tutorial-browser-overlay" data-no-window-drag>
              <section className="tutorial-browser-window tutorial-library-window">
                <div className="tutorial-browser-header">
                  <div className="tutorial-browser-title-group">
                    <span className="tutorial-control-label">{copy.music.libraryTitle}</span>
                    <span className="tutorial-browser-summary">
                      {zh
                        ? `${allSongs.length} 首内置公版曲 · 按难度分组`
                        : `${allSongs.length} built-in public-domain songs · grouped by difficulty`}
                    </span>
                  </div>

                  <button
                    type="button"
                    className="tutorial-btn tutorial-btn-icon-only tutorial-browser-close-btn"
                    onClick={() => setBrowserMode(null)}
                    aria-label={copy.music.closeSongBrowser}
                    title={copy.music.closeSongBrowser}
                  >
                    ×
                  </button>
                </div>

                <div className="tutorial-browser-toolbar">
                  <label className="tutorial-browser-search">
                    <span className="tutorial-browser-search-icon" aria-hidden="true">⌕</span>
                    <input
                      ref={searchInputRef}
                      value={searchQuery}
                      onChange={(event) => setSearchQuery(event.currentTarget.value)}
                      placeholder={copy.music.searchSongsPlaceholder}
                      aria-label={copy.music.searchSongs}
                    />
                  </label>
                  <span className="tutorial-browser-results-count">{resultCountLabel}</span>
                </div>

                {groupedCatalogSongs.length ? (
                  <nav
                    className="tutorial-difficulty-jump-nav"
                    aria-label={zh ? '按难度跳转' : 'Jump by difficulty'}
                  >
                    {groupedCatalogSongs.map(({ difficulty, songs }) => {
                      const isActive = activeDifficultyJumpId === difficulty.id;

                      return (
                        <button
                          key={difficulty.id}
                          type="button"
                          className={`tutorial-difficulty-jump-chip${isActive ? ' active' : ''}`}
                          onClick={() => handleDifficultyJump(difficulty.id)}
                          aria-pressed={isActive}
                        >
                          <span>{difficulty.label[language]}</span>
                          <span className="tutorial-difficulty-jump-count">{songs.length}</span>
                        </button>
                      );
                    })}
                  </nav>
                ) : null}

                <div ref={libraryBrowserBodyRef} className="tutorial-browser-body">
                  <section className="tutorial-browser-section">
                    <div className="tutorial-library-section-header">
                      <span className="tutorial-library-section-title">{copy.music.libraryTitle}</span>
                      <span className="tutorial-library-section-count">{visibleCatalogSongs.length}</span>
                    </div>

                    {groupedCatalogSongs.length ? (
                      groupedCatalogSongs.map(({ difficulty, songs }) => (
                        <section
                          key={difficulty.id}
                          ref={(section) => {
                            difficultySectionRefs.current[difficulty.id] = section;
                          }}
                          className="tutorial-library-section"
                        >
                          <div className="tutorial-library-section-header">
                            <span className="tutorial-library-section-title">{difficulty.label[language]}</span>
                            <span className="tutorial-library-section-count">{songs.length}</span>
                          </div>

                          <div className="tutorial-library-grid">
                            {songs.map((entry) => {
                              const isSelected = entry.id === songId;
                              const isRecent = recentSongIdSet.has(entry.id);
                              const categoryLabel = songCategoryBadgeText(entry, language);
                              const buttonClassName = isRecent
                                ? `tutorial-library-button recent${isSelected ? ' selected' : ''}`
                                : `tutorial-library-button${isSelected ? ' selected' : ''}`;

                              return (
                                <button
                                  key={entry.id}
                                  type="button"
                                  className={buttonClassName}
                                  onClick={() => handleSongSelect(entry.id)}
                                  title={entry.title.en}
                                >
                                  <span className="tutorial-library-button-top">
                                    <span className="tutorial-library-button-title">{entry.title[language]}</span>
                                    {isRecent || categoryLabel ? (
                                      <span className="tutorial-library-button-tags">
                                        {isRecent ? (
                                          <span className="tutorial-library-button-recent-pill">
                                            {copy.music.recentSongBadge}
                                          </span>
                                        ) : null}
                                        {categoryLabel ? (
                                          <span className="tutorial-library-button-tag">{categoryLabel}</span>
                                        ) : null}
                                      </span>
                                    ) : null}
                                  </span>
                                  <span className="tutorial-library-button-meta">
                                    {songLibraryMetaText(entry, language)}
                                  </span>
                                </button>
                              );
                            })}
                          </div>
                        </section>
                      ))
                    ) : (
                      <div className="tutorial-browser-empty">
                        <span className="tutorial-browser-empty-title">{copy.music.searchNoResults}</span>
                        <span className="tutorial-browser-empty-copy">{copy.music.searchTryAnother}</span>
                      </div>
                    )}
                  </section>
                </div>
              </section>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
};

export const TutorialPanel = memo(TutorialPanelComponent);
TutorialPanel.displayName = 'TutorialPanel';
