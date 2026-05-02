/**
 * Pure-function judging logic for the rhythm game.
 *
 * No React or Zustand dependency — keeps the inner loop testable and
 * reusable.  All public functions return a *partial state delta* that
 * the store can merge with `set()`.
 */

import type { TutorialNote } from '../../../../data/tutorialSongTypes';

// ── Tunable timing windows (seconds) ──────────────────────────────────

/** Within ±PERFECT_WINDOW of a note start ⇒ Perfect */
export const PERFECT_WINDOW = 0.05;
/** Within ±GREAT_WINDOW ⇒ Great */
export const GREAT_WINDOW = 0.10;
/** Within ±GOOD_WINDOW ⇒ Good */
export const GOOD_WINDOW = 0.18;
/** A note becomes a hold-style note above this duration. */
export const HOLD_THRESHOLD = 0.9;
/** Releasing within this window before the note end is fine. */
export const HOLD_RELEASE_TOLERANCE = 0.15;

export type HitGrade = 'perfect' | 'great' | 'good' | 'miss';

export interface HitFeedback {
  id: number;
  grade: HitGrade;
  midi: number;
  /** screen-space x ratio 0..1 to position floating text without coupling
   *  judging logic to renderer geometry. The renderer maps midi→x itself. */
  createdAt: number;
}

export interface SuccessPulse {
  id: number;
  midi: number;
  grade: Exclude<HitGrade, 'miss'>;
  createdAt: number;
}

export interface ActiveHold {
  noteIndex: number;
  midi: number;
  startGrade: Exclude<HitGrade, 'miss'>;
  noteEndTime: number;
}

let nextFeedbackId = 1;
let nextPulseId = 1;
function freshFeedbackId(): number { return nextFeedbackId++; }
function freshPulseId(): number { return nextPulseId++; }

function gradeFromOffset(absOffset: number): HitGrade {
  if (absOffset <= PERFECT_WINDOW) return 'perfect';
  if (absOffset <= GREAT_WINDOW) return 'great';
  if (absOffset <= GOOD_WINDOW) return 'good';
  return 'miss';
}

interface JudgeHitInput {
  pressedMidi: number;
  songTime: number;
  notes: readonly TutorialNote[];
  now: number;
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: readonly HitFeedback[];
  successPulses: readonly SuccessPulse[];
  judgedIndices: ReadonlySet<number>;
  activeHolds: readonly ActiveHold[];
  heldMidis: ReadonlySet<number>;
}

interface JudgeHitOutput {
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
}

/** Find the not-yet-judged note for `midi` whose start is closest to `songTime`. */
function findClosestUnjudged(
  notes: readonly TutorialNote[],
  judged: ReadonlySet<number>,
  midi: number,
  songTime: number,
): { index: number; note: TutorialNote } | null {
  let bestIndex = -1;
  let bestAbs = Infinity;
  for (let i = 0; i < notes.length; i++) {
    if (judged.has(i)) continue;
    const n = notes[i];
    if (n.midi !== midi) continue;
    const dt = Math.abs(n.time - songTime);
    if (dt > GOOD_WINDOW + 0.5) continue;
    if (dt < bestAbs) {
      bestAbs = dt;
      bestIndex = i;
    }
  }
  if (bestIndex < 0) return null;
  return { index: bestIndex, note: notes[bestIndex] };
}

export function judgeHit(input: JudgeHitInput): JudgeHitOutput {
  const candidate = findClosestUnjudged(input.notes, input.judgedIndices, input.pressedMidi, input.songTime);

  let { perfect, great, good, miss, combo, maxCombo } = input;
  const feedbacks = input.feedbacks.slice();
  const successPulses = input.successPulses.slice();
  const judgedIndices = new Set(input.judgedIndices);
  const activeHolds = input.activeHolds.slice();
  const heldMidis = new Set(input.heldMidis);

  if (!candidate) {
    return { perfect, great, good, miss, combo, maxCombo, feedbacks, successPulses, judgedIndices, activeHolds, heldMidis };
  }

  const offset = candidate.note.time - input.songTime;
  const grade = gradeFromOffset(Math.abs(offset));

  if (grade === 'miss') {
    return { perfect, great, good, miss, combo, maxCombo, feedbacks, successPulses, judgedIndices, activeHolds, heldMidis };
  }

  if (grade === 'perfect') perfect++;
  else if (grade === 'great') great++;
  else good++;
  combo++;
  if (combo > maxCombo) maxCombo = combo;
  judgedIndices.add(candidate.index);

  feedbacks.push({ id: freshFeedbackId(), grade, midi: input.pressedMidi, createdAt: input.now });
  successPulses.push({ id: freshPulseId(), midi: input.pressedMidi, grade, createdAt: input.now });

  if (candidate.note.duration >= HOLD_THRESHOLD) {
    activeHolds.push({
      noteIndex: candidate.index,
      midi: candidate.note.midi,
      startGrade: grade,
      noteEndTime: candidate.note.time + candidate.note.duration,
    });
    heldMidis.add(candidate.note.midi);
  }

  return { perfect, great, good, miss, combo, maxCombo, feedbacks, successPulses, judgedIndices, activeHolds, heldMidis };
}

interface JudgeReleaseInput {
  releasedMidi: number;
  songTime: number;
  now: number;
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: readonly HitFeedback[];
  successPulses: readonly SuccessPulse[];
  activeHolds: readonly ActiveHold[];
  heldMidis: ReadonlySet<number>;
}

interface JudgeReleaseOutput {
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: HitFeedback[];
  successPulses: SuccessPulse[];
  activeHolds: ActiveHold[];
  heldMidis: Set<number>;
}

export function judgeRelease(input: JudgeReleaseInput): JudgeReleaseOutput {
  const idx = input.activeHolds.findIndex((h) => h.midi === input.releasedMidi);
  if (idx < 0) {
    return {
      perfect: input.perfect,
      great: input.great,
      good: input.good,
      miss: input.miss,
      combo: input.combo,
      maxCombo: input.maxCombo,
      feedbacks: input.feedbacks.slice(),
      successPulses: input.successPulses.slice(),
      activeHolds: input.activeHolds.slice(),
      heldMidis: new Set(input.heldMidis),
    };
  }

  const hold = input.activeHolds[idx];
  let { combo, maxCombo, miss } = input;
  const feedbacks = input.feedbacks.slice();
  const successPulses = input.successPulses.slice();
  const activeHolds = input.activeHolds.slice();
  activeHolds.splice(idx, 1);
  const heldMidis = new Set(input.heldMidis);
  heldMidis.delete(hold.midi);

  const earlyBy = hold.noteEndTime - input.songTime;
  if (earlyBy > HOLD_RELEASE_TOLERANCE) {
    miss++;
    combo = 0;
    feedbacks.push({ id: freshFeedbackId(), grade: 'miss', midi: hold.midi, createdAt: input.now });
  } else {
    if (combo > maxCombo) maxCombo = combo;
    successPulses.push({ id: freshPulseId(), midi: hold.midi, grade: hold.startGrade, createdAt: input.now });
  }

  return {
    perfect: input.perfect,
    great: input.great,
    good: input.good,
    miss,
    combo,
    maxCombo,
    feedbacks,
    successPulses,
    activeHolds,
    heldMidis,
  };
}

interface SweepMissesInput {
  songTime: number;
  notes: readonly TutorialNote[];
  now: number;
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: readonly HitFeedback[];
  judgedIndices: ReadonlySet<number>;
  activeHolds: readonly ActiveHold[];
  heldMidis: ReadonlySet<number>;
}

interface SweepMissesOutput {
  perfect: number;
  great: number;
  good: number;
  miss: number;
  combo: number;
  maxCombo: number;
  feedbacks: HitFeedback[];
  judgedIndices: Set<number>;
  activeHolds: ActiveHold[];
  heldMidis: Set<number>;
}

export function sweepMisses(input: SweepMissesInput): SweepMissesOutput {
  let { miss, combo } = input;
  const feedbacks = input.feedbacks.slice();
  const judgedIndices = new Set(input.judgedIndices);
  const heldMidis = new Set(input.heldMidis);
  const activeHolds: ActiveHold[] = [];
  let { maxCombo } = input;

  for (let i = 0; i < input.notes.length; i++) {
    if (judgedIndices.has(i)) continue;
    const n = input.notes[i];
    if (input.songTime - n.time > GOOD_WINDOW) {
      judgedIndices.add(i);
      miss++;
      combo = 0;
      feedbacks.push({ id: freshFeedbackId(), grade: 'miss', midi: n.midi, createdAt: input.now });
    }
  }

  for (const hold of input.activeHolds) {
    if (input.songTime > hold.noteEndTime + HOLD_RELEASE_TOLERANCE) {
      heldMidis.delete(hold.midi);
      continue;
    }
    activeHolds.push(hold);
  }

  return {
    perfect: input.perfect,
    great: input.great,
    good: input.good,
    miss,
    combo,
    maxCombo,
    feedbacks,
    judgedIndices,
    activeHolds,
    heldMidis,
  };
}
