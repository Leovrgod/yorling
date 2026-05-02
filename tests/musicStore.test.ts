import test from 'node:test';
import assert from 'node:assert/strict';
import { DEFAULT_INSTRUMENT_ID } from '../src/data/instruments.ts';
import {
  parseStoredMusicVolume,
  parseStoredRecentInstrumentIds,
  rememberRecentInstrumentId,
  removeRecentInstrumentId,
  useMusicStore,
} from '../src/stores/musicStore.ts';

function resetMusicStore() {
  useMusicStore.setState({
    selectedInstrument: DEFAULT_INSTRUMENT_ID,
    recentInstrumentIds: [],
    keyboardLayout: 'classic',
    pianoLayoutOctave: 4,
    pianoLowerLayoutOctave: 3,
    pianoNumberLayoutOctave: 5,
    volume: 100,
  });
}

test('defaults missing stored music panel split to a balanced layout', async () => {
  const musicStore = await import('../src/stores/musicStore.ts');
  const parseStoredMusicPanelSplit = Reflect.get(musicStore, 'parseStoredMusicPanelSplit') as
    | ((raw: string | null) => number)
    | undefined;

  assert.strictEqual(typeof parseStoredMusicPanelSplit, 'function');
  assert.strictEqual(parseStoredMusicPanelSplit?.(null), 0.42);
  assert.strictEqual(parseStoredMusicPanelSplit?.(''), 0.42);
});

test('clamps stored music panel split into the supported resize range', async () => {
  const musicStore = await import('../src/stores/musicStore.ts');
  const parseStoredMusicPanelSplit = Reflect.get(musicStore, 'parseStoredMusicPanelSplit') as
    | ((raw: string | null) => number)
    | undefined;

  assert.strictEqual(typeof parseStoredMusicPanelSplit, 'function');
  assert.strictEqual(parseStoredMusicPanelSplit?.('0.12'), 0.28);
  assert.strictEqual(parseStoredMusicPanelSplit?.('0.58'), 0.58);
  assert.strictEqual(parseStoredMusicPanelSplit?.('0.91'), 0.72);
});

test('falls back to the default split for invalid stored music panel split values', async () => {
  const musicStore = await import('../src/stores/musicStore.ts');
  const parseStoredMusicPanelSplit = Reflect.get(musicStore, 'parseStoredMusicPanelSplit') as
    | ((raw: string | null) => number)
    | undefined;

  assert.strictEqual(typeof parseStoredMusicPanelSplit, 'function');
  assert.strictEqual(parseStoredMusicPanelSplit?.('oops'), 0.42);
});

test('defaults missing stored music volume to a usable level', () => {
  assert.strictEqual(parseStoredMusicVolume(null), 100);
  assert.strictEqual(parseStoredMusicVolume(''), 100);
});

test('clamps stored music volume into the supported MIDI range', () => {
  assert.strictEqual(parseStoredMusicVolume('-12'), 0);
  assert.strictEqual(parseStoredMusicVolume('88'), 88);
  assert.strictEqual(parseStoredMusicVolume('300'), 127);
});

test('falls back to default volume for invalid stored values', () => {
  assert.strictEqual(parseStoredMusicVolume('oops'), 100);
});

test('defaults missing stored music keyboard layout to the classic layout', async () => {
  const musicStore = await import('../src/stores/musicStore.ts');
  const parseStoredMusicKeyboardLayout = Reflect.get(musicStore, 'parseStoredMusicKeyboardLayout') as
    | ((raw: string | null) => string)
    | undefined;
  const parseStoredPianoLayoutOctave = Reflect.get(musicStore, 'parseStoredPianoLayoutOctave') as
    | ((raw: string | null) => number)
    | undefined;
  const parseStoredPianoLowerLayoutOctave = Reflect.get(musicStore, 'parseStoredPianoLowerLayoutOctave') as
    | ((raw: string | null) => number)
    | undefined;
  const parseStoredPianoNumberLayoutOctave = Reflect.get(musicStore, 'parseStoredPianoNumberLayoutOctave') as
    | ((raw: string | null) => number)
    | undefined;

  assert.strictEqual(typeof parseStoredMusicKeyboardLayout, 'function');
  assert.strictEqual(typeof parseStoredPianoLayoutOctave, 'function');
  assert.strictEqual(typeof parseStoredPianoLowerLayoutOctave, 'function');
  assert.strictEqual(typeof parseStoredPianoNumberLayoutOctave, 'function');
  assert.strictEqual(parseStoredMusicKeyboardLayout?.(null), 'classic');
  assert.strictEqual(parseStoredMusicKeyboardLayout?.('unknown'), 'classic');
  assert.strictEqual(parseStoredPianoLayoutOctave?.(null), 4);
  assert.strictEqual(parseStoredPianoLayoutOctave?.('3'), 3);
  assert.strictEqual(parseStoredPianoLowerLayoutOctave?.(null), 3);
  assert.strictEqual(parseStoredPianoLowerLayoutOctave?.('2'), 2);
  assert.strictEqual(parseStoredPianoNumberLayoutOctave?.(null), 5);
  assert.strictEqual(parseStoredPianoNumberLayoutOctave?.('6'), 6);
});

test('defaults missing or invalid stored recent instruments to an empty list', () => {
  assert.deepEqual(parseStoredRecentInstrumentIds(null), []);
  assert.deepEqual(parseStoredRecentInstrumentIds(''), []);
  assert.deepEqual(parseStoredRecentInstrumentIds('oops'), []);
  assert.deepEqual(parseStoredRecentInstrumentIds('{"id":"violin"}'), []);
});

test('sanitizes stored recent instruments to known unique ids while preserving first-seen order', () => {
  const raw = JSON.stringify([
    'violin',
    'flute',
    'violin',
    'not-a-real-instrument',
    'synth-piano',
    'clarinet',
    'trumpet',
    'cello',
    'oboe',
    'tuba',
    'harpsichord',
  ]);

  assert.deepEqual(
    parseStoredRecentInstrumentIds(raw),
    [
      'violin',
      'flute',
      'synth-piano',
      'clarinet',
      'trumpet',
      'cello',
      'oboe',
      'tuba',
      'harpsichord',
    ],
  );
});

test('keeps first-seen ordering and ignores repeat selections in recent instruments', () => {
  assert.deepEqual(
    rememberRecentInstrumentId(['flute', 'violin'], 'violin'),
    ['flute', 'violin'],
  );
  assert.deepEqual(
    rememberRecentInstrumentId(['flute', 'clarinet'], 'synth-piano'),
    ['flute', 'clarinet', 'synth-piano'],
  );
});

test('removes a recent instrument without disturbing the rest of the history', () => {
  assert.deepEqual(
    removeRecentInstrumentId(['violin', 'flute', 'clarinet'], 'flute'),
    ['violin', 'clarinet'],
  );
});

test('keeps recent instrument history in sync with selection and deletion actions', () => {
  resetMusicStore();

  useMusicStore.getState().setInstrument('violin');
  useMusicStore.getState().setInstrument('flute');
  useMusicStore.getState().setInstrument('violin');

  assert.strictEqual(useMusicStore.getState().selectedInstrument, 'violin');
  assert.deepEqual(useMusicStore.getState().recentInstrumentIds, ['violin', 'flute']);

  useMusicStore.getState().removeRecentInstrument('violin');

  assert.strictEqual(useMusicStore.getState().selectedInstrument, 'violin');
  assert.deepEqual(useMusicStore.getState().recentInstrumentIds, ['flute']);
});

test('keeps the draggable music panel split in sync with store updates', () => {
  const musicState = useMusicStore.getState() as unknown as Record<string, unknown>;
  const setPanelSplit = Reflect.get(musicState, 'setPanelSplit') as
    | ((value: number) => void)
    | undefined;

  assert.strictEqual(typeof setPanelSplit, 'function');

  setPanelSplit?.(0.76);
  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'panelSplit'),
    0.72,
  );

  setPanelSplit?.(0.37);
  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'panelSplit'),
    0.37,
  );
});

test('keeps the selected music keyboard layout and per-zone piano octaves in sync with store actions', () => {
  resetMusicStore();

  const musicState = useMusicStore.getState() as unknown as Record<string, unknown>;
  const setKeyboardLayout = Reflect.get(musicState, 'setKeyboardLayout') as
    | ((layoutId: string) => void)
    | undefined;
  const shiftPianoLayoutOctave = Reflect.get(musicState, 'shiftPianoLayoutOctave') as
    | ((delta: number) => void)
    | undefined;
  const shiftPianoLowerLayoutOctave = Reflect.get(musicState, 'shiftPianoLowerLayoutOctave') as
    | ((delta: number) => void)
    | undefined;
  const shiftPianoNumberLayoutOctave = Reflect.get(musicState, 'shiftPianoNumberLayoutOctave') as
    | ((delta: number) => void)
    | undefined;

  assert.strictEqual(typeof setKeyboardLayout, 'function');
  assert.strictEqual(typeof shiftPianoLayoutOctave, 'function');
  assert.strictEqual(typeof shiftPianoLowerLayoutOctave, 'function');
  assert.strictEqual(typeof shiftPianoNumberLayoutOctave, 'function');

  setKeyboardLayout?.('piano');
  shiftPianoLayoutOctave?.(-1);
  shiftPianoLayoutOctave?.(3);
  shiftPianoLowerLayoutOctave?.(-1);
  shiftPianoNumberLayoutOctave?.(1);

  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'keyboardLayout'),
    'piano',
  );
  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'pianoLayoutOctave'),
    6,
  );
  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'pianoLowerLayoutOctave'),
    2,
  );
  assert.strictEqual(
    Reflect.get(useMusicStore.getState() as unknown as Record<string, unknown>, 'pianoNumberLayoutOctave'),
    6,
  );
});
