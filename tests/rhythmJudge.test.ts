import test from 'node:test';
import assert from 'node:assert/strict';
import type { TutorialNote } from '../src/data/tutorialSongTypes.ts';
import {
  judgeHit,
  judgeRelease,
  sweepMisses,
} from '../src/components/music/rhythm/engine/judge.ts';

function emptyJudgeState() {
  return {
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

test('judges a close tap as a successful hit and opens a hold when duration is long enough', () => {
  const notes: TutorialNote[] = [
    { midi: 60, time: 1, duration: 1.2 },
  ];

  const result = judgeHit({
    pressedMidi: 60,
    songTime: 1.02,
    notes,
    now: 1000,
    ...emptyJudgeState(),
  });

  assert.strictEqual(result.perfect, 1);
  assert.strictEqual(result.combo, 1);
  assert.strictEqual(result.judgedIndices.has(0), true);
  assert.strictEqual(result.successPulses.length, 1);
  assert.strictEqual(result.activeHolds.length, 1);
  assert.strictEqual(result.heldMidis.has(60), true);
});

test('releasing an active hold after its tail keeps the combo alive', () => {
  const releaseResult = judgeRelease({
    releasedMidi: 60,
    songTime: 2.24,
    now: 1400,
    perfect: 1,
    great: 0,
    good: 0,
    miss: 0,
    combo: 1,
    maxCombo: 1,
    feedbacks: [],
    successPulses: [],
    activeHolds: [{
      noteIndex: 0,
      midi: 60,
      startGrade: 'perfect',
      noteEndTime: 2.2,
    }],
    heldMidis: new Set([60]),
  });

  assert.strictEqual(releaseResult.miss, 0);
  assert.strictEqual(releaseResult.combo, 1);
  assert.strictEqual(releaseResult.activeHolds.length, 0);
  assert.strictEqual(releaseResult.heldMidis.has(60), false);
  assert.strictEqual(releaseResult.successPulses.length, 1);
});

test('sweeps notes that pass the timing window into misses', () => {
  const notes: TutorialNote[] = [
    { midi: 64, time: 0.5, duration: 0.2 },
    { midi: 67, time: 1.8, duration: 0.2 },
  ];

  const result = sweepMisses({
    songTime: 0.75,
    notes,
    now: 1500,
    ...emptyJudgeState(),
  });

  assert.strictEqual(result.miss, 1);
  assert.strictEqual(result.combo, 0);
  assert.strictEqual(result.feedbacks.length, 1);
  assert.strictEqual(result.judgedIndices.has(0), true);
  assert.strictEqual(result.judgedIndices.has(1), false);
});
