/**
 * Rhythm game scoring store.
 *
 * Pure judging logic lives in `engine/judge.ts`. This store wraps it
 * with reactive React state and exposes the `successPulses` /
 * `heldMidis` channels that `MusicKeyboard.tsx` reads to flash the
 * physical keys on a confirmed hit.
 */

import { create } from 'zustand';
import type { TutorialNote } from '../data/tutorialSongTypes';
import {
  judgeHit as pureJudgeHit,
  judgeRelease as pureJudgeRelease,
  sweepMisses as pureSweepMisses,
  type ActiveHold,
  type HitFeedback,
  type HitGrade,
  type SuccessPulse,
} from '../components/music/rhythm/engine/judge';

export type { ActiveHold, HitFeedback, HitGrade, SuccessPulse };

export interface ScoreState {
  enabled: boolean;
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: HitFeedback[];
  successPulses: SuccessPulse[];
  judgedIndices: Set<number>;
  activeHolds: ActiveHold[];
  heldMidis: Set<number>;

  enable: () => void;
  disable: () => void;
  reset: () => void;
  judgeHit: (pressedMidi: number, songTime: number, notes: readonly TutorialNote[]) => void;
  judgeRelease: (releasedMidi: number, songTime: number) => void;
  sweepMisses: (songTime: number, notes: readonly TutorialNote[]) => void;
  cleanFeedbacks: () => void;
}

const FEEDBACK_TTL_MS = 900;
const PULSE_TTL_MS = 360;

function emptyState(): Omit<ScoreState,
  'enable' | 'disable' | 'reset' | 'judgeHit' | 'judgeRelease' | 'sweepMisses' | 'cleanFeedbacks'
> {
  return {
    enabled: false,
    perfect: 0,
    great: 0,
    good: 0,
    miss: 0,
    combo: 0,
    maxCombo: 0,
    feedbacks: [],
    successPulses: [],
    judgedIndices: new Set<number>(),
    activeHolds: [],
    heldMidis: new Set<number>(),
  };
}

export const useRhythmScoreStore = create<ScoreState>((set, get) => ({
  ...emptyState(),

  enable: () => set({ ...emptyState(), enabled: true }),
  disable: () => set({ ...emptyState(), enabled: false }),
  reset: () => set({ ...emptyState(), enabled: get().enabled }),

  judgeHit: (pressedMidi, songTime, notes) => {
    const state = get();
    if (!state.enabled) return;
    const result = pureJudgeHit({
      pressedMidi,
      songTime,
      notes,
      now: performance.now(),
      perfect: state.perfect,
      great: state.great,
      good: state.good,
      miss: state.miss,
      combo: state.combo,
      maxCombo: state.maxCombo,
      feedbacks: state.feedbacks,
      successPulses: state.successPulses,
      judgedIndices: state.judgedIndices,
      activeHolds: state.activeHolds,
      heldMidis: state.heldMidis,
    });
    set(result);
  },

  judgeRelease: (releasedMidi, songTime) => {
    const state = get();
    if (!state.enabled) return;
    const result = pureJudgeRelease({
      releasedMidi,
      songTime,
      now: performance.now(),
      perfect: state.perfect,
      great: state.great,
      good: state.good,
      miss: state.miss,
      combo: state.combo,
      maxCombo: state.maxCombo,
      feedbacks: state.feedbacks,
      successPulses: state.successPulses,
      activeHolds: state.activeHolds,
      heldMidis: state.heldMidis,
    });
    set(result);
  },

  sweepMisses: (songTime, notes) => {
    const state = get();
    if (!state.enabled) return;
    const result = pureSweepMisses({
      songTime,
      notes,
      now: performance.now(),
      perfect: state.perfect,
      great: state.great,
      good: state.good,
      miss: state.miss,
      combo: state.combo,
      maxCombo: state.maxCombo,
      feedbacks: state.feedbacks,
      judgedIndices: state.judgedIndices,
      activeHolds: state.activeHolds,
      heldMidis: state.heldMidis,
    });
    const shouldUpdate = result.miss !== state.miss
      || result.combo !== state.combo
      || result.feedbacks.length !== state.feedbacks.length
      || result.judgedIndices.size !== state.judgedIndices.size
      || result.activeHolds.length !== state.activeHolds.length
      || result.heldMidis.size !== state.heldMidis.size;
    if (shouldUpdate) {
      set(result);
    }
  },

  cleanFeedbacks: () => {
    const now = performance.now();
    const { feedbacks, successPulses } = get();
    const nextFeedbacks = feedbacks.filter((fb) => now - fb.createdAt < FEEDBACK_TTL_MS);
    const nextPulses = successPulses.filter((p) => now - p.createdAt < PULSE_TTL_MS);
    if (nextFeedbacks.length !== feedbacks.length || nextPulses.length !== successPulses.length) {
      set({ feedbacks: nextFeedbacks, successPulses: nextPulses });
    }
  },
}));

/** Returns the latest active success-pulse id per MIDI, used by the
 *  bottom keyboard to flash the matching physical key briefly. */
export function getLatestActiveSuccessPulseIdsByMidi(
  pulses: readonly SuccessPulse[],
): Map<number, number> {
  const now = performance.now();
  const latest = new Map<number, SuccessPulse>();
  for (const p of pulses) {
    if (now - p.createdAt > PULSE_TTL_MS) continue;
    const prev = latest.get(p.midi);
    if (!prev || prev.createdAt < p.createdAt) {
      latest.set(p.midi, p);
    }
  }
  const result = new Map<number, number>();
  for (const [midi, pulse] of latest.entries()) {
    result.set(midi, pulse.id);
  }
  return result;
}

/** Computed accuracy 0..100 across judged notes. */
export function getAccuracy(s: Pick<ScoreState, 'perfect' | 'great' | 'good' | 'miss'>): number {
  const total = s.perfect + s.great + s.good + s.miss;
  if (total === 0) return 0;
  return Math.round(((s.perfect + s.great * 0.7 + s.good * 0.4) / total) * 100);
}

export function isSongComplete(
  s: Pick<ScoreState, 'judgedIndices'>,
  totalNotes: number,
): boolean {
  return totalNotes > 0 && s.judgedIndices.size >= totalNotes;
}
