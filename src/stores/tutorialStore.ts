import { create } from 'zustand';
import { TUTORIAL_SONGS, getTutorialSong } from '../data/tutorialSongs.ts';
import type { TutorialSong } from '../data/tutorialSongTypes';

/**
 * Lead-in time before the first note reaches the judgment line.
 * Playback starts at -LEAD_IN_SEC so players can see notes approaching.
 */
export const LEAD_IN_SEC = 2;
const RECENT_SONGS_STORAGE_KEY = 'yorling.musicRecentSongs';

function findSong(id: string): TutorialSong | undefined {
  return getTutorialSong(id);
}

function sanitizeRecentSongIds(ids: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];

  for (const id of ids) {
    if (!findSong(id) || seen.has(id)) {
      continue;
    }

    seen.add(id);
    result.push(id);
  }

  return result;
}

export function parseStoredRecentSongIds(raw: string | null): string[] {
  if (raw === null || raw.trim() === '') {
    return [];
  }

  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return sanitizeRecentSongIds(
      parsed.filter((item): item is string => typeof item === 'string'),
    );
  } catch {
    return [];
  }
}

function readStoredRecentSongIds(): string[] {
  if (typeof window === 'undefined') {
    return [];
  }

  return parseStoredRecentSongIds(window.localStorage.getItem(RECENT_SONGS_STORAGE_KEY));
}

function persistRecentSongIds(ids: string[]): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(RECENT_SONGS_STORAGE_KEY, JSON.stringify(ids));
  }
}

export function rememberRecentSongId(currentIds: string[], nextId: string): string[] {
  if (!findSong(nextId)) {
    throw new Error(`Unknown tutorial song: ${nextId}`);
  }

  const sanitizedCurrentIds = sanitizeRecentSongIds(currentIds);
  return sanitizedCurrentIds.includes(nextId)
    ? sanitizedCurrentIds
    : [...sanitizedCurrentIds, nextId];
}

export function removeRecentSongId(currentIds: string[], idToRemove: string): string[] {
  return sanitizeRecentSongIds(currentIds).filter((id) => id !== idToRemove);
}

export interface TutorialState {
  /** Whether tutorial mode UI is visible. */
  active: boolean;
  songId: string;
  recentSongIds: string[];
  isPlaying: boolean;
  /** Whether demo playback is active (auto-plays notes, no scoring). */
  isDemoPlaying: boolean;
  speed: number;
  externalCurrentTime: number | null;

  /** performance.now() timestamp when playback last (re)started. */
  playStartTs: number;
  /** Accumulated elapsed song-time (seconds) when last paused. */
  elapsedAtPause: number;

  // ── actions ──
  setActive: (active: boolean) => void;
  selectSong: (id: string) => void;
  markCurrentSongPracticed: () => void;
  removeRecentSong: (id: string) => void;
  play: () => void;
  pause: (currentTimeSeconds?: number) => void;
  stop: () => void;
  setSpeed: (speed: number) => void;
  setExternalCurrentTime: (time: number | null) => void;
  startDemo: () => void;
  stopDemo: () => void;
}

export const useTutorialStore = create<TutorialState>((set, get) => ({
  active: false,
  songId: TUTORIAL_SONGS[0]?.id ?? '',
  recentSongIds: readStoredRecentSongIds(),
  isPlaying: false,
  isDemoPlaying: false,
  speed: 1,
  externalCurrentTime: null,
  playStartTs: 0,
  elapsedAtPause: -LEAD_IN_SEC,

  setActive: (active) => {
    if (!active && get().isPlaying) {
      set({ active: false, isPlaying: false, isDemoPlaying: false, elapsedAtPause: -LEAD_IN_SEC, externalCurrentTime: null });
      return;
    }
    set({ active });
  },

  selectSong: (id) => {
    if (!findSong(id)) return;
    set({
      songId: id,
      isPlaying: false,
      isDemoPlaying: false,
      elapsedAtPause: -LEAD_IN_SEC,
      externalCurrentTime: null,
    });
  },

  markCurrentSongPracticed: () => {
    const songId = get().songId;
    if (!findSong(songId)) return;

    const recentSongIds = rememberRecentSongId(get().recentSongIds, songId);
    if (recentSongIds.length === get().recentSongIds.length) {
      return;
    }

    persistRecentSongIds(recentSongIds);
    set({ recentSongIds });
  },

  removeRecentSong: (id) => {
    const recentSongIds = removeRecentSongId(get().recentSongIds, id);
    persistRecentSongIds(recentSongIds);
    set({ recentSongIds });
  },

  play: () => {
    set({ isPlaying: true, playStartTs: performance.now() });
  },

  pause: (currentTimeSeconds) => {
    const { isPlaying, playStartTs, elapsedAtPause, externalCurrentTime, speed } = get();
    if (!isPlaying) return;
    const elapsed = currentTimeSeconds ?? externalCurrentTime
      ?? elapsedAtPause + (performance.now() - playStartTs) / 1000 * speed;
    set({ isPlaying: false, elapsedAtPause: elapsed, externalCurrentTime: currentTimeSeconds ?? externalCurrentTime });
  },

  stop: () => {
    set({ isPlaying: false, isDemoPlaying: false, elapsedAtPause: -LEAD_IN_SEC, externalCurrentTime: null });
  },

  setSpeed: (speed) => {
    const { isPlaying, playStartTs, elapsedAtPause, externalCurrentTime, speed: oldSpeed } = get();
    if (!isPlaying) {
      set({ speed });
      return;
    }
    const elapsed = externalCurrentTime
      ?? elapsedAtPause + (performance.now() - playStartTs) / 1000 * oldSpeed;
    set({ speed, elapsedAtPause: elapsed, playStartTs: performance.now() });
  },
  setExternalCurrentTime: (time) => {
    set({ externalCurrentTime: time });
  },
  startDemo: () => {
    set({
      isDemoPlaying: true,
      isPlaying: true,
      playStartTs: performance.now(),
      elapsedAtPause: -LEAD_IN_SEC,
      externalCurrentTime: null,
    });
  },
  stopDemo: () => {
    set({
      isDemoPlaying: false,
      isPlaying: false,
      elapsedAtPause: -LEAD_IN_SEC,
      externalCurrentTime: null,
    });
  },
}));

/** Compute the current song time in seconds. */
export function getCurrentSongTime(
  state: Pick<TutorialState, 'isPlaying' | 'playStartTs' | 'elapsedAtPause' | 'externalCurrentTime' | 'speed'>,
): number {
  if (state.externalCurrentTime !== null) return state.externalCurrentTime;
  if (!state.isPlaying) return state.elapsedAtPause;
  return state.elapsedAtPause + (performance.now() - state.playStartTs) / 1000 * state.speed;
}

/** Get the selected TutorialSong or the first one as fallback. */
export function getSelectedSong(songId: string): TutorialSong {
  return findSong(songId) ?? TUTORIAL_SONGS[0];
}
