import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('describes the current piano engine honestly for future upgrades', () => {
  const source = readFileSync('src/hooks/usePianoAudio.ts', 'utf8');

  assert.ok(/export const PIANO_ENGINE_PROFILE/.test(source));
  assert.ok(/kind:\s*'modeled-synth'/.test(source));
  assert.ok(/isRealPiano:\s*false/.test(source));
  assert.ok(/supportsSamplePlayback:\s*true/.test(source));
});

test('keeps the improved modeled synth as the default timbre', () => {
  const source = readFileSync('src/hooks/usePianoAudio.ts', 'utf8');

  assert.ok(/createPeriodicWave|setPeriodicWave/.test(source));
  assert.ok(/createBiquadFilter/.test(source));
  assert.ok(!/type = 'triangle'/.test(source));
});
