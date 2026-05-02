/**
 * Rhythm game playback / song-selection store.
 *
 * Replaces the legacy `tutorialStore`. Public surface is kept compatible
 * with what `MusicKeyboard.tsx` expects so the wiring around the bottom
 * piano keyboard stays minimal.
 */

import { create } from 'zustand';
import { TUTORIAL_SONGS, getTutorialSong } from '../data/tutorialSongs';
import type { TutorialSong } from '../data/tutorialSongTypes';

/** Lead-in time before the first note reaches the judgment line. */
export const LEAD_IN_SEC = 2;
const RECENT_SONGS_STORAGE_KEY = 'yorling.rhythmRecentSongs';

function findSong(id: string): TutorialSong | undefined {
  return getTutorialSong(id);
}

function sanitizeRecentSongIds(ids: readonly string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const id of ids) {
    if (!findSong(id) || seen.has(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

export function parseStoredRecentSongIds(raw: string | null): string[] {
  if (raw === null || raw.trim() === '') return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return sanitizeRecentSongIds(parsed.filter((it): it is string => typeof it === 'string'));
  } catch {
    return [];
  }
}

function readStoredRecentSongIds(): string[] {
  if (typeof window === 'undefined') return [];
  return parseStoredRecentSongIds(window.localStorage.getItem(RECENT_SONGS_STORAGE_KEY));
}

function persistRecentSongIds(ids: string[]): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(RECENT_SONGS_STORAGE_KEY, JSON.stringify(ids));
  }
}

export function rememberRecentSongId(currentIds: readonly string[], nextId: string): string[] {
  if (!findSong(nextId)) {
    throw new Error(`Unknown rhythm song: ${nextId}`);
  }
  const sanitized = sanitizeRecentSongIds(currentIds);
  return sanitized.includes(nextId) ? sanitized : [...sanitized, nextId];
}

export function removeRecentSongId(currentIds: readonly string[], idToRemove: string): string[] {
  return sanitizeRecentSongIds(currentIds).filter((id) => id !== idToRemove);
}

export interface RhythmGameState {
  active: boolean;
  songId: string;
  recentSongIds: string[];
  isPlaying: boolean;
  isDemoPlaying: boolean;
  speed: number;
  externalCurrentTime: number | null;
  /** performance.now() timestamp at the most recent play start. */
  playStartTs: number;
  /** Accumulated song-time (seconds) when last paused. */
  elapsedAtPause: number;

  setActive: (active: boolean) => void;
  selectSong: (id: string) => void;
  removeRecentSong: (id: string) => void;
  play: () => void;
  pause: (currentTimeSeconds?: number) => void;
  stop: () => void;
  setSpeed: (speed: number) => void;
  setExternalCurrentTime: (time: number | null) => void;
  startDemo: () => void;
  stopDemo: () => void;
}

export const useRhythmGameStore = create<RhythmGameState>((set, get) => ({
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
      set({
        active: false,
        isPlaying: false,
        isDemoPlaying: false,
        elapsedAtPause: -LEAD_IN_SEC,
        externalCurrentTime: null,
      });
      return;
    }
    set({ active });
  },

  selectSong: (id) => {
    if (!findSong(id)) return;
    const recentSongIds = rememberRecentSongId(get().recentSongIds, id);
    persistRecentSongIds(recentSongIds);
    set({
      songId: id,
      recentSongIds,
      isPlaying: false,
      isDemoPlaying: false,
      elapsedAtPause: -LEAD_IN_SEC,
      externalCurrentTime: null,
    });
  },

  removeRecentSong: (id) => {
    const recentSongIds = removeRecentSongId(get().recentSongIds, id);
    persistRecentSongIds(recentSongIds);
    set({ recentSongIds });
  },

  play: () => set({ isPlaying: true, playStartTs: performance.now() }),

  pause: (currentTimeSeconds) => {
    const { isPlaying, playStartTs, elapsedAtPause, externalCurrentTime, speed } = get();
    if (!isPlaying) return;
    const elapsed = currentTimeSeconds
      ?? externalCurrentTime
      ?? elapsedAtPause + ((performance.now() - playStartTs) / 1000) * speed;
    set({
      isPlaying: false,
      elapsedAtPause: elapsed,
      externalCurrentTime: currentTimeSeconds ?? externalCurrentTime,
    });
  },

  stop: () => set({
    isPlaying: false,
    isDemoPlaying: false,
    elapsedAtPause: -LEAD_IN_SEC,
    externalCurrentTime: null,
  }),

  setSpeed: (speed) => {
    const { isPlaying, playStartTs, elapsedAtPause, externalCurrentTime, speed: oldSpeed } = get();
    if (!isPlaying) {
      set({ speed });
      return;
    }
    const elapsed = externalCurrentTime
      ?? elapsedAtPause + ((performance.now() - playStartTs) / 1000) * oldSpeed;
    set({ speed, elapsedAtPause: elapsed, playStartTs: performance.now() });
  },

  setExternalCurrentTime: (time) => set({ externalCurrentTime: time }),

  startDemo: () => set({
    isDemoPlaying: true,
    isPlaying: true,
    playStartTs: performance.now(),
    elapsedAtPause: -LEAD_IN_SEC,
    externalCurrentTime: null,
  }),

  stopDemo: () => set({
    isDemoPlaying: false,
    isPlaying: false,
    elapsedAtPause: -LEAD_IN_SEC,
    externalCurrentTime: null,
  }),
}));

export function getCurrentSongTime(
  state: Pick<RhythmGameState, 'isPlaying' | 'playStartTs' | 'elapsedAtPause' | 'externalCurrentTime' | 'speed'>,
): number {
  if (state.externalCurrentTime !== null) return state.externalCurrentTime;
  if (!state.isPlaying) return state.elapsedAtPause;
  return state.elapsedAtPause + ((performance.now() - state.playStartTs) / 1000) * state.speed;
}

export function getSelectedSong(songId: string): TutorialSong {
  return findSong(songId) ?? TUTORIAL_SONGS[0];
}
