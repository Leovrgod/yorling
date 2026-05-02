import { memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  TUTORIAL_SONGS,
  getTutorialSongCategory,
  getTutorialSongDifficulty,
  groupTutorialSongsByDifficulty,
} from '../../data/tutorialSongs';
import { noteColorClass, noteInfoFromMidi, noteLabel } from '../../data/musicKeyMapping';
import type { TutorialSong } from '../../data/tutorialSongTypes';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import {
  LEAD_IN_SEC,
  getCurrentSongTime,
  getSelectedSong,
  useRhythmGameStore,
} from '../../stores/rhythmGameStore';
import { getAccuracy, useRhythmScoreStore } from '../../stores/rhythmScoreStore';
import type { AppLanguageId } from '../../types';
import { RhythmStage } from './rhythm/RhythmStage';

const SPEED_OPTIONS = [0.75, 1, 1.25, 1.5] as const;

type BrowserMode = 'library' | 'recent';

function speedLabel(value: number): string {
  return value === 1 ? '1×' : `${value}×`;
}

function formatSeconds(value: number): string {
  const safe = Math.max(0, Math.round(value));
  const minutes = Math.floor(safe / 60);
  const seconds = safe % 60;
  return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

function computeRank(accuracy: number): string {
  if (accuracy >= 98) return 'S';
  if (accuracy >= 92) return 'A';
  if (accuracy >= 84) return 'B';
  if (accuracy >= 72) return 'C';
  return 'D';
}

function songMetaText(song: TutorialSong, language: AppLanguageId): string {
  return language === 'zh'
    ? `${Math.round(song.bpm)} BPM · ${song.notes.length} 音符`
    : `${Math.round(song.bpm)} BPM · ${song.notes.length} notes`;
}

function matchesSongQuery(song: TutorialSong, query: string): boolean {
  if (!query) return true;
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
  return haystack.includes(query);
}

interface RhythmGamePanelProps {
  noteOn: (midi: number) => void;
  noteOff: (midi: number) => void;
  stopAllNotes: () => void;
}

function MediaControlButton({
  kind,
  label,
  onClick,
  active = false,
}: {
  kind: 'play' | 'pause' | 'stop';
  label: string;
  onClick: () => void;
  active?: boolean;
}) {
  return (
    <button
      type="button"
      className={`rhythm-media-btn ${active ? 'active' : ''}`}
      onClick={onClick}
      aria-label={label}
      title={label}
    >
      <span className={`rhythm-media-icon rhythm-media-icon--${kind}`} aria-hidden="true" />
    </button>
  );
}

export const RhythmGamePanel = memo(function RhythmGamePanel({
  noteOn,
  noteOff,
  stopAllNotes,
}: RhythmGamePanelProps) {
  const language = useAppStore((state) => state.language) as AppLanguageId;
  const theme = useAppStore((state) => state.theme);
  const copy = getUiCopy(language);
  const zh = language === 'zh';

  const songId = useRhythmGameStore((state) => state.songId);
  const recentSongIds = useRhythmGameStore((state) => state.recentSongIds);
  const isPlaying = useRhythmGameStore((state) => state.isPlaying);
  const isDemoPlaying = useRhythmGameStore((state) => state.isDemoPlaying);
  const speed = useRhythmGameStore((state) => state.speed);
  const selectSong = useRhythmGameStore((state) => state.selectSong);
  const removeRecentSong = useRhythmGameStore((state) => state.removeRecentSong);
  const play = useRhythmGameStore((state) => state.play);
  const pause = useRhythmGameStore((state) => state.pause);
  const stop = useRhythmGameStore((state) => state.stop);
  const setSpeed = useRhythmGameStore((state) => state.setSpeed);
  const setExternalCurrentTime = useRhythmGameStore((state) => state.setExternalCurrentTime);
  const startDemo = useRhythmGameStore((state) => state.startDemo);
  const stopDemo = useRhythmGameStore((state) => state.stopDemo);

  const perfect = useRhythmScoreStore((state) => state.perfect);
  const great = useRhythmScoreStore((state) => state.great);
  const good = useRhythmScoreStore((state) => state.good);
  const miss = useRhythmScoreStore((state) => state.miss);
  const combo = useRhythmScoreStore((state) => state.combo);
  const maxCombo = useRhythmScoreStore((state) => state.maxCombo);
  const feedbacks = useRhythmScoreStore((state) => state.feedbacks);
  const judgedCount = useRhythmScoreStore((state) => state.judgedIndices.size);
  const activeHoldCount = useRhythmScoreStore((state) => state.activeHolds.length);
  const scoringEnabled = useRhythmScoreStore((state) => state.enabled);
  const enableScoring = useRhythmScoreStore((state) => state.enable);
  const disableScoring = useRhythmScoreStore((state) => state.disable);
  const resetScoring = useRhythmScoreStore((state) => state.reset);

  const [browserMode, setBrowserMode] = useState<BrowserMode | null>(null);
  const [searchQuery, setSearchQuery] = useState('');
  const [resultsVisible, setResultsVisible] = useState(false);
  const [displaySongTime, setDisplaySongTime] = useState(() => getCurrentSongTime(useRhythmGameStore.getState()));
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioFrameRef = useRef<number | null>(null);
  const audioLeadInRef = useRef<number | null>(null);
  const demoTimersRef = useRef<ReturnType<typeof setTimeout>[]>([]);
  const demoActiveNotesRef = useRef<Set<number>>(new Set());

  const dismissResults = useCallback(() => {
    setResultsVisible(false);
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

  const stopAudioLeadIn = useCallback(() => {
    if (audioLeadInRef.current !== null) {
      cancelAnimationFrame(audioLeadInRef.current);
      audioLeadInRef.current = null;
    }
  }, []);

  const clearDemoTimers = useCallback(() => {
    for (const timer of demoTimersRef.current) {
      clearTimeout(timer);
    }
    demoTimersRef.current = [];
    for (const midi of demoActiveNotesRef.current) {
      noteOff(midi);
    }
    demoActiveNotesRef.current.clear();
  }, [noteOff]);

  const allSongs = TUTORIAL_SONGS;

  const song = useMemo(() => {
    return getSelectedSong(songId);
  }, [songId]);

  const recentSongs = useMemo(() => {
    const byId = new Map(allSongs.map((entry) => [entry.id, entry] as const));
    return recentSongIds
      .map((id) => byId.get(id))
      .filter((entry): entry is TutorialSong => entry !== undefined);
  }, [allSongs, recentSongIds]);

  const visibleLibrarySongs = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    return allSongs.filter((entry) => matchesSongQuery(entry, query));
  }, [allSongs, searchQuery]);

  const libraryGroups = useMemo(
    () => groupTutorialSongsByDifficulty(visibleLibrarySongs),
    [visibleLibrarySongs],
  );

  const recentGroups = useMemo(
    () => groupTutorialSongsByDifficulty(recentSongs),
    [recentSongs],
  );

  const recentSongIdSet = useMemo(
    () => new Set(recentSongs.map((entry) => entry.id)),
    [recentSongs],
  );

  const accuracy = getAccuracy({ perfect, great, good, miss });
  const rank = computeRank(accuracy);
  const lastFeedback = feedbacks.length > 0 ? feedbacks[feedbacks.length - 1] : null;
  const progress = ((displaySongTime + LEAD_IN_SEC) / Math.max(song.duration + LEAD_IN_SEC, 0.001)) * 100;
  const progressPercent = Math.max(0, Math.min(100, progress));
  const upcomingNotes = useMemo(() => {
    const midis = new Set<number>();
    const nextNotes: { midi: number; label: string; colorClass: string }[] = [];

    for (const note of song.notes) {
      if (note.time < displaySongTime - 0.08) continue;
      if (midis.has(note.midi)) continue;

      const noteInfo = noteInfoFromMidi(note.midi);
      midis.add(note.midi);
      nextNotes.push({
        midi: note.midi,
        label: noteLabel(noteInfo),
        colorClass: noteColorClass(noteInfo.note),
      });

      if (nextNotes.length >= 6) break;
    }
    return nextNotes;
  }, [displaySongTime, song.notes]);
  const resultBreakdown = [
    { key: 'perfect', label: 'Perfect', value: perfect, tone: 'perfect' },
    { key: 'great', label: 'Great', value: great, tone: 'great' },
    { key: 'good', label: copy.music.scoreGood, value: good, tone: 'good' },
    { key: 'miss', label: copy.music.scoreMiss, value: miss, tone: 'miss' },
    { key: 'combo', label: copy.music.scoreMaxCombo, value: maxCombo, tone: 'neutral' },
    { key: 'notes', label: copy.music.scoreTotalNotes, value: song.notes.length, tone: 'neutral' },
  ] as const;

  const concludeRun = useCallback((currentTime?: number) => {
    const game = useRhythmGameStore.getState();
    if (!game.isPlaying || game.isDemoPlaying) return;

    const audio = audioRef.current;
    const finalTime = currentTime ?? (song.audioUrl
      ? audio?.currentTime ?? getCurrentSongTime(game)
      : getCurrentSongTime(game));

    if (audio) {
      audio.pause();
    }
    stopAllNotes();
    pause(finalTime);
    stopAudioSync();
    stopAudioLeadIn();

    const liveScore = useRhythmScoreStore.getState();
    if (liveScore.enabled && liveScore.judgedIndices.size > 0) {
      setResultsVisible(true);
      return;
    }

    disableScoring();
  }, [disableScoring, pause, song.audioUrl, stopAllNotes, stopAudioLeadIn, stopAudioSync]);

  const fullyStopPlayback = useCallback(() => {
    clearDemoTimers();
    stopDemo();
    stopAllNotes();
    stopAudioSync();
    stopAudioLeadIn();
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.currentTime = 0;
    }
    stop();
  }, [clearDemoTimers, stop, stopAllNotes, stopAudioLeadIn, stopAudioSync, stopDemo]);

  const startFreshRun = useCallback(() => {
    fullyStopPlayback();
    resetScoring();
    enableScoring();
    dismissResults();
    play();
  }, [dismissResults, enableScoring, fullyStopPlayback, play, resetScoring]);

  const handleSongSelect = useCallback((nextSongId: string) => {
    fullyStopPlayback();
    disableScoring();
    dismissResults();
    setSearchQuery('');
    setBrowserMode(null);
    selectSong(nextSongId);
  }, [disableScoring, dismissResults, fullyStopPlayback, selectSong]);

  const handlePlayPause = useCallback(() => {
    if (isDemoPlaying) {
      fullyStopPlayback();
      disableScoring();
      dismissResults();
      return;
    }

    if (isPlaying) {
      pause(song.audioUrl ? audioRef.current?.currentTime : undefined);
      stopAudioSync();
      stopAudioLeadIn();
      stopAllNotes();
      return;
    }

    const currentTime = song.audioUrl
      ? audioRef.current?.currentTime ?? getCurrentSongTime(useRhythmGameStore.getState())
      : getCurrentSongTime(useRhythmGameStore.getState());

    const shouldRestart = resultsVisible
      || currentTime >= song.duration - 0.05
      || (!scoringEnabled && currentTime <= -LEAD_IN_SEC + 0.02);

    if (shouldRestart) {
      startFreshRun();
      return;
    }

    dismissResults();
    if (!scoringEnabled) {
      enableScoring();
    }
    play();
  }, [
    disableScoring,
    dismissResults,
    enableScoring,
    fullyStopPlayback,
    isDemoPlaying,
    isPlaying,
    pause,
    play,
    resultsVisible,
    scoringEnabled,
    song.audioUrl,
    song.duration,
    startFreshRun,
    stopAllNotes,
    stopAudioLeadIn,
    stopAudioSync,
  ]);

  const handleStop = useCallback(() => {
    fullyStopPlayback();
    disableScoring();
    dismissResults();
  }, [disableScoring, dismissResults, fullyStopPlayback]);

  const handleDemoToggle = useCallback(() => {
    if (isDemoPlaying) {
      fullyStopPlayback();
      disableScoring();
      dismissResults();
      return;
    }

    fullyStopPlayback();
    resetScoring();
    dismissResults();
    startDemo();
  }, [disableScoring, dismissResults, fullyStopPlayback, isDemoPlaying, resetScoring, startDemo]);

  const handleRandomSong = useCallback(() => {
    if (allSongs.length === 0) return;
    const candidates = allSongs.filter((entry) => entry.id !== song.id);
    const nextSong = candidates[Math.floor(Math.random() * candidates.length)] ?? allSongs[0];
    handleSongSelect(nextSong.id);
  }, [allSongs, handleSongSelect, song.id]);

  const handleReplay = useCallback(() => {
    startFreshRun();
  }, [startFreshRun]);

  const handleChangeSong = useCallback(() => {
    fullyStopPlayback();
    disableScoring();
    dismissResults();
    setBrowserMode('library');
  }, [disableScoring, dismissResults, fullyStopPlayback]);

  useEffect(() => {
    if (!isDemoPlaying) {
      return undefined;
    }

    clearDemoTimers();
    const timers: ReturnType<typeof setTimeout>[] = [];
    for (const note of song.notes) {
      const onDelay = (LEAD_IN_SEC + note.time / speed) * 1000;
      const offDelay = onDelay + (note.duration / speed) * 1000;

      timers.push(setTimeout(() => {
        noteOn(note.midi);
        demoActiveNotesRef.current.add(note.midi);
      }, onDelay));

      timers.push(setTimeout(() => {
        noteOff(note.midi);
        demoActiveNotesRef.current.delete(note.midi);
      }, offDelay));
    }

    timers.push(setTimeout(() => {
      stopDemo();
      stopAllNotes();
    }, (LEAD_IN_SEC + song.duration / speed) * 1000 + 420));

    demoTimersRef.current = timers;
    return () => {
      clearDemoTimers();
    };
  }, [clearDemoTimers, isDemoPlaying, noteOff, noteOn, song, speed, stopAllNotes, stopDemo]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return undefined;

    stopAudioSync();
    stopAudioLeadIn();

    if (!song.audioUrl) {
      audio.pause();
      audio.removeAttribute('src');
      audio.load();
      setExternalCurrentTime(null);
      return undefined;
    }

    audio.pause();
    audio.src = song.audioUrl;
    audio.currentTime = 0;
    audio.playbackRate = speed;
    audio.load();
    setExternalCurrentTime(0);

    const handleEnded = () => {
      concludeRun(audio.currentTime);
    };

    audio.addEventListener('ended', handleEnded);
    return () => {
      audio.removeEventListener('ended', handleEnded);
      audio.pause();
      stopAudioSync();
      stopAudioLeadIn();
    };
  }, [
    concludeRun,
    setExternalCurrentTime,
    song.audioUrl,
    song.id,
    speed,
    stopAudioLeadIn,
    stopAudioSync,
  ]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !song.audioUrl) return;
    audio.playbackRate = speed;
  }, [song.audioUrl, speed]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !song.audioUrl) return undefined;

    if (!isPlaying) {
      stopAudioSync();
      stopAudioLeadIn();
      audio.pause();
      return undefined;
    }

    const targetTime = getCurrentSongTime(useRhythmGameStore.getState());
    if (targetTime < 0) {
      audio.pause();
      audio.currentTime = 0;
      setExternalCurrentTime(null);

      const pollLeadIn = () => {
        const liveTime = getCurrentSongTime(useRhythmGameStore.getState());
        if (liveTime >= 0) {
          audio.currentTime = liveTime;
          const playback = audio.play();
          if (playback && typeof playback.then === 'function') {
            void playback.then(() => {
              stopAudioSync();
              syncAudioTime();
            }).catch(() => {
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

    if (Math.abs(audio.currentTime - targetTime) > 0.05) {
      audio.currentTime = targetTime;
    }
    const playback = audio.play();
    if (playback && typeof playback.then === 'function') {
      void playback.then(() => {
        stopAudioSync();
        syncAudioTime();
      }).catch(() => {
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
  }, [
    disableScoring,
    isPlaying,
    pause,
    setExternalCurrentTime,
    song.audioUrl,
    stopAudioLeadIn,
    stopAudioSync,
    syncAudioTime,
  ]);

  useEffect(() => {
    if (isPlaying) {
      setBrowserMode(null);
    }
  }, [isPlaying]);

  useEffect(() => {
    let frameId = 0;
    let lastPainted = Number.NaN;

    const tick = () => {
      const liveTime = getCurrentSongTime(useRhythmGameStore.getState());
      if (!Number.isFinite(lastPainted) || Math.abs(liveTime - lastPainted) >= 0.04) {
        lastPainted = liveTime;
        setDisplaySongTime(liveTime);
      }

      const game = useRhythmGameStore.getState();
      const liveScore = useRhythmScoreStore.getState();
      const shouldFinish = game.isPlaying
        && !game.isDemoPlaying
        && liveScore.enabled
        && song.notes.length > 0
        && liveScore.judgedIndices.size >= song.notes.length
        && liveScore.activeHolds.length === 0
        && liveTime >= song.duration - 0.03;

      const shouldTimeOut = game.isPlaying
        && !game.isDemoPlaying
        && !song.audioUrl
        && liveTime > song.duration + 0.4;

      if (!resultsVisible && (shouldFinish || shouldTimeOut)) {
        concludeRun(liveTime);
        return;
      }

      frameId = requestAnimationFrame(tick);
    };

    tick();
    return () => {
      cancelAnimationFrame(frameId);
    };
  }, [concludeRun, resultsVisible, song.audioUrl, song.duration, song.notes.length]);

  const browserGroups = browserMode === 'recent' ? recentGroups : libraryGroups;
  const showResults = resultsVisible && judgedCount > 0;

  return (
    <div className="rhythm-panel">
      <div className="rhythm-hero" data-no-window-drag>
        <div className="rhythm-hero-main">
          <div className="rhythm-hero-copy">
            <h3 className="rhythm-title">{song.title[language]}</h3>
            <div className="rhythm-hero-metrics" aria-live="polite">
              <span className="rhythm-inline-metric">
                {copy.music.scoreAccuracy} <strong>{accuracy}%</strong>
              </span>
              <span className="rhythm-inline-metric">
                {copy.music.scoreCombo} <strong>{combo}</strong>
              </span>
              <span className="rhythm-inline-metric">
                {copy.music.scoreMaxCombo} <strong>{maxCombo}</strong>
              </span>
            </div>
          </div>
        </div>

        <div className="rhythm-controls">
          <div className="rhythm-button-row">
            <MediaControlButton
              kind={isPlaying ? 'pause' : 'play'}
              label={isPlaying ? (zh ? '暂停' : 'Pause') : (zh ? '播放' : 'Play')}
              onClick={handlePlayPause}
              active={isPlaying}
            />
            <MediaControlButton
              kind="stop"
              label={zh ? '结束' : 'Stop'}
              onClick={handleStop}
            />
            <button type="button" className="rhythm-btn" onClick={handleDemoToggle}>
              {isDemoPlaying ? copy.music.demoStop : copy.music.demoPlay}
            </button>
            <button type="button" className="rhythm-btn" onClick={handleRandomSong}>
              {copy.music.randomSong}
            </button>
          </div>

          <div className="rhythm-button-row">
            <button
              type="button"
              className={`rhythm-btn ${browserMode === 'library' ? 'active' : ''}`}
              onClick={() => setBrowserMode(browserMode === 'library' ? null : 'library')}
              disabled={isPlaying}
            >
              {copy.music.libraryTitle}
            </button>
            <button
              type="button"
              className={`rhythm-btn ${browserMode === 'recent' ? 'active' : ''}`}
              onClick={() => setBrowserMode(browserMode === 'recent' ? null : 'recent')}
              disabled={isPlaying}
            >
              {copy.music.recentSongsTitle}
            </button>
          </div>

          <div className="rhythm-speed-strip" role="group" aria-label="Playback speed">
            {SPEED_OPTIONS.map((option) => (
              <button
                key={option}
                type="button"
                className={`rhythm-speed-chip ${speed === option ? 'active' : ''}`}
                onClick={() => setSpeed(option)}
              >
                {speedLabel(option)}
              </button>
            ))}
          </div>
        </div>
      </div>

      <div className="rhythm-stage-card">
        <div className="rhythm-stage-header" data-no-window-drag>
          <div className="rhythm-progress-meta">
            <span>{zh ? '进度' : 'Progress'}</span>
            <strong>{formatSeconds(displaySongTime)} / {formatSeconds(song.duration)}</strong>
          </div>
          <div className="rhythm-progress-bar" aria-hidden="true">
            <span style={{ width: `${progressPercent}%` }} />
          </div>
          <div className="rhythm-stage-badges">
            <span className="rhythm-stage-badge">
              {zh ? '判定' : 'Judged'} {judgedCount}/{song.notes.length}
            </span>
            <span className="rhythm-stage-badge">
              {zh ? '长按' : 'Holds'} {activeHoldCount}
            </span>
            {displaySongTime < 0 ? (
              <span className="rhythm-stage-badge accent">
                {zh ? `预备 ${Math.ceil(-displaySongTime)}s` : `Lead-in ${Math.ceil(-displaySongTime)}s`}
              </span>
            ) : null}
          </div>
        </div>

        <RhythmStage song={song} isDark={theme === 'dark'} />

        <div className="rhythm-stage-footer" data-no-window-drag>
          {upcomingNotes.length > 0 ? (
            <div className="rhythm-upcoming" aria-label={zh ? '接下来音符' : 'Upcoming notes'}>
              <div className="rhythm-upcoming-list">
                {upcomingNotes.map((note) => (
                  <span key={note.midi} className={`rhythm-upcoming-chip ${note.colorClass}`}>
                    {note.label}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          <div className="rhythm-results-strip">
            {resultBreakdown.slice(0, 4).map((item) => (
              <span key={item.key}>
                <strong>{item.value}</strong> {item.label}
              </span>
            ))}
          </div>
        </div>

        {lastFeedback ? (
          <div key={lastFeedback.id} className={`rhythm-feedback-burst grade-${lastFeedback.grade}`}>
            {lastFeedback.grade.toUpperCase()}
          </div>
        ) : null}

        {combo >= 3 && isPlaying && !isDemoPlaying ? (
          <div className="rhythm-combo-burst">
            <strong>{combo}</strong>
            <span>{copy.music.scoreCombo}</span>
          </div>
        ) : null}

        {showResults ? (
          <div className="rhythm-results-overlay" data-no-window-drag>
            <div className="rhythm-results-card">
              <div className="rhythm-results-header">
                <div className="rhythm-results-rank" data-rank={rank}>{rank}</div>
                <div className="rhythm-results-copy">
                  <span className="rhythm-results-eyebrow">{copy.music.scoreResults}</span>
                  <span className="rhythm-results-title">{song.title[language]}</span>
                  <span className="rhythm-results-accuracy">
                    {copy.music.scoreAccuracy}
                    <strong>{accuracy}%</strong>
                  </span>
                </div>
              </div>

              <div className="rhythm-results-highlight">
                <span className="rhythm-results-highlight-label">{copy.music.scoreCombo}</span>
                <div className="rhythm-results-highlight-value">{combo}</div>
              </div>

              <div className="rhythm-results-grid">
                {resultBreakdown.map((item) => (
                  <div key={item.key} className={`rhythm-results-stat rhythm-results-stat--${item.tone}`}>
                    <span className="rhythm-results-stat-label">{item.label}</span>
                    <strong className="rhythm-results-stat-value">{item.value}</strong>
                  </div>
                ))}
              </div>
              <div className="rhythm-results-actions">
                <button
                  type="button"
                  className="rhythm-results-action rhythm-results-action--primary"
                  onClick={handleReplay}
                >
                  {copy.music.scorePlayAgain}
                </button>
                <button
                  type="button"
                  className="rhythm-results-action"
                  onClick={handleChangeSong}
                >
                  {copy.music.scoreChangeSong}
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>

      {browserMode ? (
        <div className="rhythm-browser-overlay" data-no-window-drag>
          <div className="rhythm-browser-window">
            <div className="rhythm-browser-header">
              <div>
                <h4>{browserMode === 'recent' ? copy.music.recentSongsTitle : copy.music.libraryTitle}</h4>
                <p>
                  {browserMode === 'recent'
                    ? (zh ? '刚刚练过的谱面会继续留在这里。' : 'Recently played charts stay here for quick restarts.')
                    : (zh ? '保留曲库，重建的是引擎、渲染和反馈。' : 'The song library stays; the engine, rendering, and feedback are rebuilt.')}
                </p>
              </div>
              <button type="button" className="rhythm-btn" onClick={() => setBrowserMode(null)}>
                {copy.music.closeSongBrowser}
              </button>
            </div>

            {browserMode === 'library' ? (
              <label className="rhythm-browser-search">
                <span>{copy.music.searchSongs}</span>
                <input
                  type="search"
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.currentTarget.value)}
                  placeholder={copy.music.searchSongsPlaceholder}
                />
              </label>
            ) : null}

            <div className="rhythm-browser-body">
              {browserGroups.length > 0 ? browserGroups.map(({ difficulty, songs }) => (
                <section key={difficulty.id} className="rhythm-browser-group">
                  <header className="rhythm-browser-group-header">
                    <span>{difficulty.label[language]}</span>
                    <small>{songs.length}</small>
                  </header>

                  <div className="rhythm-song-grid">
                    {songs.map((entry) => {
                      const category = getTutorialSongCategory(entry.categoryId);
                      const isSelected = entry.id === song.id;
                      return (
                        <article
                          key={entry.id}
                          className={`rhythm-song-card ${isSelected ? 'selected' : ''}`}
                        >
                          <button
                            type="button"
                            className="rhythm-song-card-button"
                            onClick={() => handleSongSelect(entry.id)}
                          >
                            <div className="rhythm-song-card-top">
                              <strong>{entry.title[language]}</strong>
                              {recentSongIdSet.has(entry.id) ? (
                                <span className="rhythm-song-pill">{copy.music.recentSongBadge}</span>
                              ) : null}
                            </div>
                            <p>{songMetaText(entry, language)}</p>
                             <div className="rhythm-song-tags">
                               <span>{category.label[language]}</span>
                               <span>ABC</span>
                             </div>
                           </button>

                          {browserMode === 'recent' ? (
                            <button
                              type="button"
                              className="rhythm-song-remove"
                              onClick={() => removeRecentSong(entry.id)}
                              aria-label={zh ? '移除最近曲目' : 'Remove recent song'}
                            >
                              ×
                            </button>
                          ) : null}
                        </article>
                      );
                    })}
                  </div>
                </section>
              )) : (
                <div className="rhythm-browser-empty">
                  <strong>{browserMode === 'recent' ? copy.music.recentSongsTitle : copy.music.searchNoResults}</strong>
                  <span>
                    {browserMode === 'recent'
                      ? (zh ? '演奏过一首歌后，这里会出现快捷回放列表。' : 'After you play a song, quick replays show up here.')
                      : copy.music.searchTryAnother}
                  </span>
                </div>
              )}
            </div>
          </div>
        </div>
      ) : null}

      <audio ref={audioRef} className="tutorial-audio-element" preload="auto" />
    </div>
  );
});
