/**
 * Scoring system for piano tutorial mode.
 *
 * Tracks hit accuracy (Perfect / Good / Miss), combo streaks,
 * hold-note sustain tracking, and per-note judgments rendered
 * as floating feedback text.
 */

import { create } from 'zustand';
import type { TutorialNote } from '../data/tutorialSongTypes';

// ── Timing windows (seconds) ────────────────────────────────────────

/** Within ±PERFECT_WINDOW of the note start → Perfect */
const PERFECT_WINDOW = 0.08;
/** Within ±GOOD_WINDOW → Good */
const GOOD_WINDOW = 0.18;

/** Only genuinely sustained notes should require hold input; normal melody notes stay tap notes. */
const HOLD_THRESHOLD = 0.9;
/** Grace period before the note end: releasing within this window is OK. */
const HOLD_RELEASE_TOLERANCE = 0.15;

export type HitGrade = 'perfect' | 'good' | 'miss';

export interface HitFeedback {
  id: number;
  grade: HitGrade;
  midi: number;
  /** Timestamp (performance.now) when the feedback was created. */
  createdAt: number;
}

export interface SuccessPulse {
  id: number;
  midi: number;
  createdAt: number;
}

/** A hold note that has been started but not yet fully resolved. */
export interface ActiveHold {
  noteIndex: number;
  midi: number;
  startGrade: HitGrade;
  noteEndTime: number;
}

export interface ScoringState {
  /** Whether scoring is active (follows tutorial isPlaying). */
  enabled: boolean;
  perfect: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  /** Recent hit feedbacks for floating text display (kept brief). */
  feedbacks: HitFeedback[];
  /** Brief per-hit pulses used for key flash confirmation. */
  successPulses: SuccessPulse[];
  /** Set of note indices that have already been judged. */
  judgedIndices: Set<number>;
  /** Hold notes currently being sustained by the player. */
  activeHolds: ActiveHold[];
  /** Set of MIDI note numbers currently in an active hold. */
  heldMidis: Set<number>;

  // ── Actions ──
  enable: () => void;
  disable: () => void;
  reset: () => void;
  /** Judge a user key-press against expected notes. */
  judgeHit: (pressedMidi: number, songTime: number, notes: TutorialNote[]) => void;
  /** Judge a key release — resolves active holds for this MIDI note. */
  judgeRelease: (releasedMidi: number, songTime: number) => void;
  /** Mark all unjudged notes that are past the good window as misses.
   *  Also resolves holds whose note end time has passed. */
  sweepMisses: (songTime: number, notes: TutorialNote[]) => void;
  /** Remove expired feedbacks (older than TTL). */
  cleanFeedbacks: () => void;
}

const FEEDBACK_TTL_MS = 900;
export const SUCCESS_PULSE_TTL_MS = 170;
let feedbackIdCounter = 0;
let successPulseIdCounter = 0;

interface SongRuntime {
  noteIndicesByTime: number[];
  noteIndicesByMidi: Map<number, number[]>;
  nextMissCursor: number;
  lastMissSweepTime: number;
}

let songRuntimeCache = new WeakMap<readonly TutorialNote[], SongRuntime>();

function createSongRuntime(notes: readonly TutorialNote[]): SongRuntime {
  const noteIndicesByTime = notes
    .map((_, index) => index)
    .sort((left, right) => notes[left].time - notes[right].time || notes[left].midi - notes[right].midi);
  const noteIndicesByMidi = new Map<number, number[]>();

  for (const noteIndex of noteIndicesByTime) {
    const note = notes[noteIndex];
    const currentIndices = noteIndicesByMidi.get(note.midi);

    if (currentIndices) {
      currentIndices.push(noteIndex);
    } else {
      noteIndicesByMidi.set(note.midi, [noteIndex]);
    }
  }

  return {
    noteIndicesByTime,
    noteIndicesByMidi,
    nextMissCursor: 0,
    lastMissSweepTime: Number.NEGATIVE_INFINITY,
  };
}

function getSongRuntime(notes: readonly TutorialNote[]): SongRuntime {
  const cachedRuntime = songRuntimeCache.get(notes);
  if (cachedRuntime) {
    return cachedRuntime;
  }

  const runtime = createSongRuntime(notes);
  songRuntimeCache.set(notes, runtime);
  return runtime;
}

function resetSongRuntimeCache() {
  songRuntimeCache = new WeakMap<readonly TutorialNote[], SongRuntime>();
}

function lowerBoundNoteTime(
  noteIndices: readonly number[],
  notes: readonly TutorialNote[],
  targetTime: number,
): number {
  let low = 0;
  let high = noteIndices.length;

  while (low < high) {
    const mid = (low + high) >> 1;
    if (notes[noteIndices[mid]].time < targetTime) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

function findClosestMatchingNoteIndex(
  pressedMidi: number,
  songTime: number,
  notes: readonly TutorialNote[],
  judgedIndices: ReadonlySet<number>,
  holdIndices: ReadonlySet<number>,
): number {
  const noteIndices = getSongRuntime(notes).noteIndicesByMidi.get(pressedMidi);
  if (!noteIndices || noteIndices.length === 0) {
    return -1;
  }

  const insertionPoint = lowerBoundNoteTime(noteIndices, notes, songTime);
  let bestIdx = -1;
  let bestDelta = Number.POSITIVE_INFINITY;

  for (let cursor = insertionPoint - 1; cursor >= 0; cursor--) {
    const noteIndex = noteIndices[cursor];
    const delta = songTime - notes[noteIndex].time;

    if (delta > GOOD_WINDOW) {
      break;
    }

    if (judgedIndices.has(noteIndex) || holdIndices.has(noteIndex)) {
      continue;
    }

    if (delta < bestDelta) {
      bestDelta = delta;
      bestIdx = noteIndex;
    }
  }

  for (let cursor = insertionPoint; cursor < noteIndices.length; cursor++) {
    const noteIndex = noteIndices[cursor];
    const delta = notes[noteIndex].time - songTime;

    if (delta > GOOD_WINDOW) {
      break;
    }

    if (judgedIndices.has(noteIndex) || holdIndices.has(noteIndex)) {
      continue;
    }

    if (delta < bestDelta) {
      bestDelta = delta;
      bestIdx = noteIndex;
    }
  }

  return bestIdx;
}

function pruneFeedbacks(feedbacks: HitFeedback[], now: number): HitFeedback[] {
  return feedbacks.filter((feedback) => now - feedback.createdAt < FEEDBACK_TTL_MS);
}

function addFeedback(
  feedbacks: HitFeedback[],
  grade: HitGrade,
  midi: number,
  now = performance.now(),
): HitFeedback[] {
  const next = pruneFeedbacks(feedbacks, now);
  next.push({ id: ++feedbackIdCounter, grade, midi, createdAt: now });
  return next;
}

export function pruneSuccessPulses(
  pulses: SuccessPulse[],
  now = performance.now(),
): SuccessPulse[] {
  return pulses.filter((pulse) => now - pulse.createdAt < SUCCESS_PULSE_TTL_MS);
}

export function addSuccessPulse(
  pulses: SuccessPulse[],
  midi: number,
  now = performance.now(),
): SuccessPulse[] {
  const next = pruneSuccessPulses(pulses, now);
  next.push({ id: ++successPulseIdCounter, midi, createdAt: now });
  return next;
}

export function getLatestActiveSuccessPulseIdsByMidi(
  pulses: SuccessPulse[],
  now = performance.now(),
): Map<number, number> {
  const latestPulseIdsByMidi = new Map<number, number>();
  for (const pulse of pruneSuccessPulses(pulses, now)) {
    latestPulseIdsByMidi.set(pulse.midi, pulse.id);
  }
  return latestPulseIdsByMidi;
}

function downgradeGrade(grade: HitGrade): HitGrade {
  if (grade === 'perfect') return 'good';
  return 'miss';
}

function deriveHeldMidis(holds: ActiveHold[]): Set<number> {
  const set = new Set<number>();
  for (const h of holds) set.add(h.midi);
  return set;
}

export const useScoringStore = create<ScoringState>((set, get) => ({
  enabled: false,
  perfect: 0,
  good: 0,
  miss: 0,
  combo: 0,
  maxCombo: 0,
  feedbacks: [],
  successPulses: [],
  judgedIndices: new Set(),
  activeHolds: [],
  heldMidis: new Set(),

  enable: () => set({ enabled: true }),
  disable: () => set({ enabled: false }),

  reset: () =>
    set(() => {
      resetSongRuntimeCache();

      return {
        perfect: 0,
        good: 0,
        miss: 0,
        combo: 0,
        maxCombo: 0,
        feedbacks: [],
        successPulses: [],
        judgedIndices: new Set(),
        activeHolds: [],
        heldMidis: new Set(),
      };
    }),

  judgeHit: (pressedMidi, songTime, notes) => {
    const { judgedIndices, activeHolds, enabled } = get();
    if (!enabled) return;

    // Skip notes already being held
    const holdIndices = new Set(activeHolds.map((h) => h.noteIndex));
    const bestIdx = findClosestMatchingNoteIndex(
      pressedMidi,
      songTime,
      notes,
      judgedIndices,
      holdIndices,
    );

    if (bestIdx === -1) return; // No matching note nearby

    const hitNote = notes[bestIdx];
    const bestDelta = Math.abs(songTime - hitNote.time);
    const grade: HitGrade = bestDelta <= PERFECT_WINDOW ? 'perfect' : 'good';
    const isHoldNote = hitNote.duration > HOLD_THRESHOLD;

    if (isHoldNote) {
      // For hold notes: register active hold, defer final scoring
      const newHold: ActiveHold = {
        noteIndex: bestIdx,
        midi: pressedMidi,
        startGrade: grade,
        noteEndTime: hitNote.time + hitNote.duration,
      };
      const nextHolds = [...activeHolds, newHold];
      set((s) => ({
        activeHolds: nextHolds,
        heldMidis: deriveHeldMidis(nextHolds),
        feedbacks: addFeedback(s.feedbacks, grade, pressedMidi),
        successPulses: addSuccessPulse(s.successPulses, pressedMidi),
      }));
    } else {
      // Short notes: judge immediately as before
      const newJudged = new Set(judgedIndices);
      newJudged.add(bestIdx);

      set((s) => {
        const newCombo = s.combo + 1;
        return {
          [grade]: s[grade] + 1,
          combo: newCombo,
          maxCombo: Math.max(s.maxCombo, newCombo),
          judgedIndices: newJudged,
          feedbacks: addFeedback(s.feedbacks, grade, pressedMidi),
          successPulses: addSuccessPulse(s.successPulses, pressedMidi),
        };
      });
    }
  },

  judgeRelease: (releasedMidi, songTime) => {
    const { activeHolds, enabled } = get();
    if (!enabled) return;

    // Find the active hold for this MIDI (take the earliest if multiple)
    const holdIdx = activeHolds.findIndex((h) => h.midi === releasedMidi);
    if (holdIdx === -1) return;

    const hold = activeHolds[holdIdx];
    const nextHolds = activeHolds.filter((_, i) => i !== holdIdx);

    // Determine final grade based on release timing
    const releasedEarly = songTime < hold.noteEndTime - HOLD_RELEASE_TOLERANCE;
    const finalGrade = releasedEarly ? downgradeGrade(hold.startGrade) : hold.startGrade;

    const newJudged = new Set(get().judgedIndices);
    newJudged.add(hold.noteIndex);

    set((s) => {
      const newCombo = finalGrade === 'miss' ? 0 : s.combo + 1;
      return {
        [finalGrade]: s[finalGrade] + 1,
        combo: newCombo,
        maxCombo: Math.max(s.maxCombo, newCombo),
        judgedIndices: newJudged,
        activeHolds: nextHolds,
        heldMidis: deriveHeldMidis(nextHolds),
        feedbacks: addFeedback(s.feedbacks, finalGrade, releasedMidi),
      };
    });
  },

  sweepMisses: (songTime, notes) => {
    const { judgedIndices, activeHolds, enabled } = get();
    if (!enabled) return;
    const songRuntime = getSongRuntime(notes);

    // ── Resolve expired holds (player held through or past the end) ──
    const expiredHolds: ActiveHold[] = [];
    const remainingHolds: ActiveHold[] = [];

    for (const hold of activeHolds) {
      if (songTime > hold.noteEndTime + HOLD_RELEASE_TOLERANCE) {
        expiredHolds.push(hold);
      } else {
        remainingHolds.push(hold);
      }
    }

    // ── Sweep missed notes ──
    const holdIndices = new Set(activeHolds.map((h) => h.noteIndex));
    let missCount = 0;
    const newJudged = new Set(judgedIndices);
    const missedMidis: number[] = [];
    let cursor = songRuntime.nextMissCursor;

    if (songTime + GOOD_WINDOW < songRuntime.lastMissSweepTime) {
      cursor = 0;
    }

    for (; cursor < songRuntime.noteIndicesByTime.length; cursor++) {
      const noteIndex = songRuntime.noteIndicesByTime[cursor];
      const note = notes[noteIndex];

      if (songTime <= note.time + GOOD_WINDOW) {
        break;
      }

      if (newJudged.has(noteIndex) || holdIndices.has(noteIndex)) {
        continue;
      }

      newJudged.add(noteIndex);
      missCount++;
      missedMidis.push(note.midi);
    }

    songRuntime.nextMissCursor = cursor;
    songRuntime.lastMissSweepTime = songTime;

    // ── Finalize expired holds (held through the end = keep start grade) ──
    let holdPerfect = 0;
    let holdGood = 0;
    for (const hold of expiredHolds) {
      newJudged.add(hold.noteIndex);
      if (hold.startGrade === 'perfect') holdPerfect++;
      else holdGood++;
    }

    if (missCount === 0 && expiredHolds.length === 0) return;

    set((s) => {
      let feedbacks = s.feedbacks;
      for (let i = 0; i < Math.min(missCount, 3); i++) {
        feedbacks = addFeedback(feedbacks, 'miss', missedMidis[i]);
      }
      for (const hold of expiredHolds) {
        feedbacks = addFeedback(feedbacks, hold.startGrade, hold.midi);
      }

      const holdCount = expiredHolds.length;
      const newCombo = missCount > 0 ? 0 : s.combo + holdCount;
      return {
        perfect: s.perfect + holdPerfect,
        good: s.good + holdGood,
        miss: s.miss + missCount,
        combo: newCombo,
        maxCombo: Math.max(s.maxCombo, newCombo),
        judgedIndices: newJudged,
        activeHolds: remainingHolds,
        heldMidis: deriveHeldMidis(remainingHolds),
        feedbacks,
      };
    });
  },

  cleanFeedbacks: () => {
    const now = performance.now();
    const { feedbacks, successPulses } = get();
    const nextFeedbacks = pruneFeedbacks(feedbacks, now);
    const nextSuccessPulses = pruneSuccessPulses(successPulses, now);

    if (
      nextFeedbacks.length === feedbacks.length
      && nextSuccessPulses.length === successPulses.length
    ) {
      return;
    }

    set({
      feedbacks: nextFeedbacks,
      successPulses: nextSuccessPulses,
    });
  },
}));

/** Compute accuracy percentage. */
export function getAccuracy(state: Pick<ScoringState, 'perfect' | 'good' | 'miss'>): number {
  const total = state.perfect + state.good + state.miss;
  if (total === 0) return 100;
  // Perfect = 100%, Good = 60%, Miss = 0%
  return Math.round(((state.perfect * 100 + state.good * 60) / (total * 100)) * 100);
}

/** Check if the song is finished (all notes judged). */
export function isSongComplete(
  judgedCount: number,
  totalNotes: number,
): boolean {
  return totalNotes > 0 && judgedCount >= totalNotes;
}
