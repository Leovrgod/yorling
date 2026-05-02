import { create } from 'zustand';
import { DEFAULT_INSTRUMENT_ID, getInstrumentDef } from '../data/instruments.ts';
import {
  clampPianoLayoutOctave,
  DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID,
  DEFAULT_PIANO_LAYOUT_OCTAVE,
  DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE,
  DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE,
  isMusicKeyboardLayoutId,
  type MusicKeyboardLayoutId,
} from '../data/musicKeyMapping.ts';

const INSTRUMENT_STORAGE_KEY = 'yorling.musicInstrument';
const RECENT_INSTRUMENTS_STORAGE_KEY = 'yorling.musicRecentInstruments';
const PANEL_SPLIT_STORAGE_KEY = 'yorling.musicPanelSplit';
const VOLUME_STORAGE_KEY = 'yorling.musicVolume';
const MUSIC_KEYBOARD_LAYOUT_STORAGE_KEY = 'yorling.musicKeyboardLayout';
const PIANO_LAYOUT_OCTAVE_STORAGE_KEY = 'yorling.musicPianoLayoutOctave';
const PIANO_LOWER_LAYOUT_OCTAVE_STORAGE_KEY = 'yorling.musicPianoLowerLayoutOctave';
const PIANO_NUMBER_LAYOUT_OCTAVE_STORAGE_KEY = 'yorling.musicPianoNumberLayoutOctave';
const DEFAULT_MUSIC_PANEL_SPLIT = 0.42;
const MIN_MUSIC_PANEL_SPLIT = 0.28;
const MAX_MUSIC_PANEL_SPLIT = 0.72;
const DEFAULT_MUSIC_VOLUME = 100;

function isKnownInstrumentId(value: string): boolean {
  return getInstrumentDef(value) !== undefined;
}

function sanitizeRecentInstrumentIds(ids: string[]): string[] {
  const seen = new Set<string>();
  const result: string[] = [];

  for (const id of ids) {
    if (!isKnownInstrumentId(id) || seen.has(id)) {
      continue;
    }

    seen.add(id);
    result.push(id);
  }

  return result;
}

function readStoredInstrument(): string {
  if (typeof window === 'undefined') return DEFAULT_INSTRUMENT_ID;

  const storedId = window.localStorage.getItem(INSTRUMENT_STORAGE_KEY);
  return storedId && isKnownInstrumentId(storedId) ? storedId : DEFAULT_INSTRUMENT_ID;
}

export function parseStoredMusicKeyboardLayout(raw: string | null): MusicKeyboardLayoutId {
  return isMusicKeyboardLayoutId(raw) ? raw : DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID;
}

function readStoredMusicKeyboardLayout(): MusicKeyboardLayoutId {
  if (typeof window === 'undefined') {
    return DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID;
  }

  return parseStoredMusicKeyboardLayout(
    window.localStorage.getItem(MUSIC_KEYBOARD_LAYOUT_STORAGE_KEY),
  );
}

function parseStoredPianoZoneOctave(raw: string | null, fallback: number): number {
  if (raw === null || raw.trim() === '') {
    return fallback;
  }

  const value = Number(raw);
  return Number.isFinite(value)
    ? clampPianoLayoutOctave(value)
    : fallback;
}

export function parseStoredPianoLayoutOctave(raw: string | null): number {
  return parseStoredPianoZoneOctave(raw, DEFAULT_PIANO_LAYOUT_OCTAVE);
}

export function parseStoredPianoLowerLayoutOctave(raw: string | null): number {
  return parseStoredPianoZoneOctave(raw, DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE);
}

export function parseStoredPianoNumberLayoutOctave(raw: string | null): number {
  return parseStoredPianoZoneOctave(raw, DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE);
}

function readStoredPianoLayoutOctave(): number {
  if (typeof window === 'undefined') {
    return DEFAULT_PIANO_LAYOUT_OCTAVE;
  }

  return parseStoredPianoLayoutOctave(
    window.localStorage.getItem(PIANO_LAYOUT_OCTAVE_STORAGE_KEY),
  );
}

function readStoredPianoLowerLayoutOctave(): number {
  if (typeof window === 'undefined') {
    return DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE;
  }

  return parseStoredPianoLowerLayoutOctave(
    window.localStorage.getItem(PIANO_LOWER_LAYOUT_OCTAVE_STORAGE_KEY),
  );
}

function readStoredPianoNumberLayoutOctave(): number {
  if (typeof window === 'undefined') {
    return DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE;
  }

  return parseStoredPianoNumberLayoutOctave(
    window.localStorage.getItem(PIANO_NUMBER_LAYOUT_OCTAVE_STORAGE_KEY),
  );
}

export function parseStoredRecentInstrumentIds(raw: string | null): string[] {
  if (raw === null || raw.trim() === '') {
    return [];
  }

  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return sanitizeRecentInstrumentIds(
      parsed.filter((item): item is string => typeof item === 'string'),
    );
  } catch {
    return [];
  }
}

export function clampMusicPanelSplit(value: number): number {
  const clamped = Math.min(MAX_MUSIC_PANEL_SPLIT, Math.max(MIN_MUSIC_PANEL_SPLIT, value));
  return Math.round(clamped * 1000) / 1000;
}

export function parseStoredMusicPanelSplit(raw: string | null): number {
  if (raw === null || raw.trim() === '') {
    return DEFAULT_MUSIC_PANEL_SPLIT;
  }

  const value = Number(raw);
  return Number.isFinite(value) ? clampMusicPanelSplit(value) : DEFAULT_MUSIC_PANEL_SPLIT;
}

function readStoredMusicPanelSplit(): number {
  if (typeof window === 'undefined') {
    return DEFAULT_MUSIC_PANEL_SPLIT;
  }

  return parseStoredMusicPanelSplit(window.localStorage.getItem(PANEL_SPLIT_STORAGE_KEY));
}

function readStoredRecentInstrumentIds(): string[] {
  if (typeof window === 'undefined') return [];

  return parseStoredRecentInstrumentIds(
    window.localStorage.getItem(RECENT_INSTRUMENTS_STORAGE_KEY),
  );
}

export function rememberRecentInstrumentId(currentIds: string[], nextId: string): string[] {
  if (!isKnownInstrumentId(nextId)) {
    throw new Error(`Unknown music instrument: ${nextId}`);
  }

  const sanitizedCurrentIds = sanitizeRecentInstrumentIds(currentIds);
  return sanitizedCurrentIds.includes(nextId)
    ? sanitizedCurrentIds
    : [...sanitizedCurrentIds, nextId];
}

export function removeRecentInstrumentId(currentIds: string[], idToRemove: string): string[] {
  return sanitizeRecentInstrumentIds(currentIds).filter((id) => id !== idToRemove);
}

export function parseStoredMusicVolume(raw: string | null): number {
  if (raw === null || raw.trim() === '') {
    return DEFAULT_MUSIC_VOLUME;
  }

  const v = Number(raw);
  return Number.isFinite(v) ? Math.max(0, Math.min(127, v)) : DEFAULT_MUSIC_VOLUME;
}

function readStoredVolume(): number {
  if (typeof window === 'undefined') return DEFAULT_MUSIC_VOLUME;
  return parseStoredMusicVolume(window.localStorage.getItem(VOLUME_STORAGE_KEY));
}

function persistRecentInstrumentIds(ids: string[]): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(RECENT_INSTRUMENTS_STORAGE_KEY, JSON.stringify(ids));
  }
}

interface MusicStoreState {
  selectedInstrument: string;
  recentInstrumentIds: string[];
  panelSplit: number;
  volume: number;
  keyboardLayout: MusicKeyboardLayoutId;
  pianoLayoutOctave: number;
  pianoLowerLayoutOctave: number;
  pianoNumberLayoutOctave: number;
  setInstrument: (id: string) => void;
  removeRecentInstrument: (id: string) => void;
  setPanelSplit: (split: number, persist?: boolean) => void;
  setVolume: (v: number) => void;
  setKeyboardLayout: (layoutId: MusicKeyboardLayoutId) => void;
  shiftPianoLayoutOctave: (delta: number) => void;
  shiftPianoLowerLayoutOctave: (delta: number) => void;
  shiftPianoNumberLayoutOctave: (delta: number) => void;
}

export const useMusicStore = create<MusicStoreState>((set, get) => ({
  selectedInstrument: readStoredInstrument(),
  recentInstrumentIds: readStoredRecentInstrumentIds(),
  panelSplit: readStoredMusicPanelSplit(),
  volume: readStoredVolume(),
  keyboardLayout: readStoredMusicKeyboardLayout(),
  pianoLayoutOctave: readStoredPianoLayoutOctave(),
  pianoLowerLayoutOctave: readStoredPianoLowerLayoutOctave(),
  pianoNumberLayoutOctave: readStoredPianoNumberLayoutOctave(),
  setInstrument: (id) => {
    if (!isKnownInstrumentId(id)) {
      throw new Error(`Unknown music instrument: ${id}`);
    }

    const recentInstrumentIds = rememberRecentInstrumentId(get().recentInstrumentIds, id);

    if (typeof window !== 'undefined') {
      window.localStorage.setItem(INSTRUMENT_STORAGE_KEY, id);
    }
    persistRecentInstrumentIds(recentInstrumentIds);
    set({ selectedInstrument: id, recentInstrumentIds });
  },
  removeRecentInstrument: (id) => {
    const recentInstrumentIds = removeRecentInstrumentId(get().recentInstrumentIds, id);
    persistRecentInstrumentIds(recentInstrumentIds);
    set({ recentInstrumentIds });
  },
  setPanelSplit: (split, persist = true) => {
    const clamped = clampMusicPanelSplit(split);
    if (persist && typeof window !== 'undefined') {
      window.localStorage.setItem(PANEL_SPLIT_STORAGE_KEY, String(clamped));
    }
    set({ panelSplit: clamped });
  },
  setVolume: (v) => {
    const clamped = Math.max(0, Math.min(127, Math.round(v)));
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(VOLUME_STORAGE_KEY, String(clamped));
    }
    set({ volume: clamped });
  },
  setKeyboardLayout: (layoutId) => {
    if (!isMusicKeyboardLayoutId(layoutId)) {
      throw new Error(`Unknown music keyboard layout: ${layoutId}`);
    }

    if (typeof window !== 'undefined') {
      window.localStorage.setItem(MUSIC_KEYBOARD_LAYOUT_STORAGE_KEY, layoutId);
    }

    set({ keyboardLayout: layoutId });
  },
  shiftPianoLayoutOctave: (delta) => {
    const nextOctave = clampPianoLayoutOctave(get().pianoLayoutOctave + delta);

    if (typeof window !== 'undefined') {
      window.localStorage.setItem(PIANO_LAYOUT_OCTAVE_STORAGE_KEY, String(nextOctave));
    }

    set({ pianoLayoutOctave: nextOctave });
  },
  shiftPianoLowerLayoutOctave: (delta) => {
    const nextOctave = clampPianoLayoutOctave(get().pianoLowerLayoutOctave + delta);

    if (typeof window !== 'undefined') {
      window.localStorage.setItem(PIANO_LOWER_LAYOUT_OCTAVE_STORAGE_KEY, String(nextOctave));
    }

    set({ pianoLowerLayoutOctave: nextOctave });
  },
  shiftPianoNumberLayoutOctave: (delta) => {
    const nextOctave = clampPianoLayoutOctave(get().pianoNumberLayoutOctave + delta);

    if (typeof window !== 'undefined') {
      window.localStorage.setItem(PIANO_NUMBER_LAYOUT_OCTAVE_STORAGE_KEY, String(nextOctave));
    }

    set({ pianoNumberLayoutOctave: nextOctave });
  },
}));
