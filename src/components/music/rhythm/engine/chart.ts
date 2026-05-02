/**
 * Chart preparation: turns a `TutorialSong` into renderer-friendly
 * `PreparedNote[]` with deterministic colors and fast time lookup.
 */

import type { TutorialSong, TutorialNote } from '../../../../data/tutorialSongTypes';

/** Hue & saturation per pitch class (0..11), aligned with `noteColorClass`. */
const NOTE_HUE_SAT: ReadonlyArray<readonly [number, number]> = [
  [0, 72],
  [25, 80],
  [35, 85],
  [50, 80],
  [80, 60],
  [145, 55],
  [170, 55],
  [190, 70],
  [220, 65],
  [240, 55],
  [265, 55],
  [280, 55],
];

const BLACK_PCS = new Set([1, 3, 6, 8, 10]);

export function isBlackKey(midi: number): boolean {
  return BLACK_PCS.has(((midi % 12) + 12) % 12);
}

export interface PreparedNote {
  index: number;
  midi: number;
  time: number;
  duration: number;
  endTime: number;
  hue: number;
  sat: number;
}

export interface PreparedChart {
  notes: PreparedNote[];
  duration: number;
  bpm: number;
  maxDuration: number;
  minMidi: number;
  maxMidi: number;
}

export function prepareChart(song: TutorialSong): PreparedChart {
  if (song.notes.length === 0) {
    return {
      notes: [],
      duration: song.duration,
      bpm: song.bpm,
      maxDuration: 0,
      minMidi: 60,
      maxMidi: 72,
    };
  }

  const notes: PreparedNote[] = new Array(song.notes.length);
  let minMidi = 127;
  let maxMidi = 0;
  let maxDuration = 0;
  for (let i = 0; i < song.notes.length; i++) {
    const n: TutorialNote = song.notes[i];
    const pc = ((n.midi % 12) + 12) % 12;
    const [hue, sat] = NOTE_HUE_SAT[pc];
    notes[i] = {
      index: i,
      midi: n.midi,
      time: n.time,
      duration: n.duration,
      endTime: n.time + n.duration,
      hue,
      sat,
    };
    if (n.midi < minMidi) minMidi = n.midi;
    if (n.midi > maxMidi) maxMidi = n.midi;
    if (n.duration > maxDuration) maxDuration = n.duration;
  }
  notes.sort((a, b) => a.time - b.time);
  for (let i = 0; i < notes.length; i++) notes[i].index = i;
  return {
    notes,
    duration: song.duration,
    bpm: song.bpm,
    maxDuration,
    minMidi: Math.min(minMidi, maxMidi),
    maxMidi: Math.max(minMidi, maxMidi),
  };
}

/** Lower bound of notes whose `time` >= target. */
export function firstVisibleIndex(notes: readonly PreparedNote[], target: number): number {
  let lo = 0;
  let hi = notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (notes[mid].time < target) lo = mid + 1; else hi = mid;
  }
  return lo;
}

/** Upper bound of notes whose `time` <= target. */
export function lastVisibleIndex(notes: readonly PreparedNote[], target: number): number {
  let lo = 0;
  let hi = notes.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (notes[mid].time <= target) lo = mid + 1; else hi = mid;
  }
  return lo;
}
