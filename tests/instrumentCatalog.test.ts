import test from 'node:test';
import assert from 'node:assert/strict';
import { getSoundfontNames } from 'smplr';

import { INSTRUMENTS, INSTRUMENT_CATEGORIES } from '../src/data/instruments.ts';

test('exposes every smplr GM soundfont instrument alongside the built-in synth', () => {
  const builtInInstruments = INSTRUMENTS.filter((instrument) => instrument.builtIn);
  assert.strictEqual(builtInInstruments.length, 1);
  assert.strictEqual(builtInInstruments[0]?.id, 'synth-piano');

  const soundfontIds = INSTRUMENTS
    .filter((instrument) => !instrument.builtIn)
    .map((instrument) => instrument.id);
  const expectedIds = getSoundfontNames();

  assert.strictEqual(soundfontIds.length, expectedIds.length);
  assert.deepEqual([...soundfontIds].sort(), [...expectedIds].sort());
});

test('keeps every instrument in a visible selector category', () => {
  const categoryIds = new Set(INSTRUMENT_CATEGORIES.map((category) => category.id));

  for (const instrument of INSTRUMENTS) {
    assert.ok(
      categoryIds.has(instrument.categoryId),
      `${instrument.id} is assigned to an unknown category: ${instrument.categoryId}`,
    );
  }
});
