import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('validates SoundFont playability before promoting it to the active engine', () => {
  const source = readFileSync('src/hooks/useMusicAudio.ts', 'utf8');

  assert.ok(/function probeSoundfontPlayback\(sf: Soundfont\)/.test(source));
  assert.ok(source.includes('no playable samples decoded'));

  const probeIndex = source.indexOf('if (!probeSoundfontPlayback(sf)) {');
  const activateIndex = source.indexOf('activeSfRef.current = sf;');
  assert.ok(probeIndex >= 0);
  assert.ok(activateIndex >= 0);
  assert.ok(probeIndex < activateIndex);
});

test('invalidates pending SoundFont loads before switching back to the built-in synth', () => {
  const source = readFileSync('src/hooks/useMusicAudio.ts', 'utf8');

  const seqIndex = source.indexOf('const seq = ++switchSeqRef.current;');
  const builtInBranchIndex = source.indexOf('if (isBuiltIn) {');
  assert.ok(seqIndex >= 0);
  assert.ok(builtInBranchIndex >= 0);
  assert.ok(seqIndex < builtInBranchIndex);
});
