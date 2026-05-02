import test from 'node:test';
import assert from 'node:assert/strict';
import {
  firstVisibleIndex,
  lastVisibleIndex,
  prepareChart,
} from '../src/components/music/rhythm/engine/chart.ts';
import type { TutorialSong } from '../src/data/tutorialSongTypes.ts';

test('prepareChart sorts notes and records the playable pitch span', () => {
  const song: TutorialSong = {
    id: 'chart-case',
    title: { zh: '测试', en: 'Test' },
    bpm: 128,
    duration: 4,
    source: 'builtin-abc',
    categoryId: 'featured-favorites',
    difficultyId: 'stage-03',
    notes: [
      { midi: 72, time: 2.4, duration: 0.25 },
      { midi: 60, time: 0.5, duration: 1.1 },
      { midi: 67, time: 1.2, duration: 0.4 },
    ],
  };

  const chart = prepareChart(song);

  assert.deepEqual(chart.notes.map((note) => note.midi), [60, 67, 72]);
  assert.strictEqual(chart.minMidi, 60);
  assert.strictEqual(chart.maxMidi, 72);
  assert.strictEqual(chart.maxDuration, 1.1);
  assert.strictEqual(firstVisibleIndex(chart.notes, 1), 1);
  assert.strictEqual(lastVisibleIndex(chart.notes, 1.25), 2);
});

test('prepareChart provides stable defaults for empty songs', () => {
  const song: TutorialSong = {
    id: 'empty-chart',
    title: { zh: '空', en: 'Empty' },
    bpm: 90,
    duration: 0,
    source: 'builtin-abc',
    categoryId: 'featured-favorites',
    difficultyId: 'stage-01',
    notes: [],
  };

  const chart = prepareChart(song);

  assert.strictEqual(chart.notes.length, 0);
  assert.strictEqual(chart.minMidi, 60);
  assert.strictEqual(chart.maxMidi, 72);
  assert.strictEqual(chart.maxDuration, 0);
});
