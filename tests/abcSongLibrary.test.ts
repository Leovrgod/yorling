import test from 'node:test';
import assert from 'node:assert/strict';

test('builds public-domain tutorial songs through abcjs', async () => {
  const abcModule = await import('../src/data/abcSongLibrary.ts');
  const buildTutorialSongFromAbc = Reflect.get(abcModule, 'buildTutorialSongFromAbc') as
    | ((definition: {
        id: string;
        title: { zh: string; en: string };
        bpm: number;
        abc: string;
      }) => {
        source: string;
        bpm: number;
        notes: { midi: number; time: number; duration: number }[];
      })
    | undefined;

  assert.strictEqual(typeof buildTutorialSongFromAbc, 'function');

  const song = buildTutorialSongFromAbc?.({
    id: 'abc-scale',
    title: { zh: '音阶', en: 'Scale' },
    bpm: 120,
    abc: 'X:1\nT:Scale\nM:4/4\nL:1/4\nQ:1/4=120\nK:C\nC D E F|G A B c|',
  });

  assert.strictEqual(song?.source, 'builtin-abc');
  assert.strictEqual(song?.bpm, 120);
  assert.deepEqual(song?.notes.map((note) => note.midi), [60, 62, 64, 65, 67, 69, 71, 72]);
});
