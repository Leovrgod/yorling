import test from 'node:test';
import assert from 'node:assert/strict';
import { KEYBOARD_LAYOUT_ROWS } from '../src/components/keyboard/keyboardUiModel.ts';
import * as musicMappingModule from '../src/data/musicKeyMapping.ts';

const musicMapping = musicMappingModule as Record<string, unknown>;
const {
  EVENT_CODE_TO_KEY_ID,
  KEY_NOTE_MAP,
} = musicMappingModule;
type PianoLayoutState = {
  mainOctave: number;
  lowerOctave: number;
  numberOctave: number;
};
const getNoteFromEventCode = musicMapping.getNoteFromEventCode as
  | ((code: string, layoutId?: string, pianoLayoutState?: number | PianoLayoutState) => {
      note: string;
      octave: number;
      midi: number;
    } | undefined)
  | undefined;
const resolveMusicKeyboardEvent = musicMapping.resolveMusicKeyboardEvent as
  | ((code: string, purePlayActive: boolean, layoutId?: string, pianoLayoutState?: number | PianoLayoutState) => {
      shouldSwallow: boolean;
      midi: number | null;
      controlAction: string | null;
    })
  | undefined;
const getMusicInputAutoReleaseMs = musicMapping.getMusicInputAutoReleaseMs as
  | ((code: string, layoutId?: string) => number | null)
  | undefined;

const KEY_ID_TO_EVENT_CODE = Object.fromEntries(
  Object.entries(EVENT_CODE_TO_KEY_ID).map(([code, keyId]) => [keyId, code]),
);
const DEFAULT_PIANO_LAYOUT_STATE: PianoLayoutState = {
  mainOctave: 4,
  lowerOctave: 3,
  numberOctave: 5,
};

function resolvedNoteLabel(
  resolved:
    | {
        note: string;
        octave: number;
      }
    | undefined,
): string | undefined {
  return resolved ? `${resolved.note}${resolved.octave}` : undefined;
}

test('resolves the corrected music keys to the intended notes', () => {
  assert.strictEqual(typeof getNoteFromEventCode, 'function');

  const expectedNotes: Record<string, string> = {
    'A': 'F3',
    'X': 'C#3',
    'C': 'C#4',
    'M': 'D#4',
    ',': 'D#3',
    '.': 'E4',
    '/': 'E3',
    'S': 'F4',
    'D': 'F4',
    'E': 'F3',
    'G': 'G4',
    'H': 'G4',
    'J': 'G4',
    '4': 'G5',
    '5': 'G2',
    '6': 'G#4',
    'L': 'A4',
    ';': 'A4',
    "'": 'A4',
    'I': 'A3',
    'P': 'A#4',
    '[': 'A#3',
  };

  for (const [keyId, expectedNote] of Object.entries(expectedNotes)) {
    const eventCode = KEY_ID_TO_EVENT_CODE[keyId];
    assert.ok(eventCode, `Missing DOM event code for ${keyId}`);
    const resolved = getNoteFromEventCode?.(eventCode);
    if (resolved === undefined) {
      throw new Error(`Missing note for ${keyId}`);
    }
    assert.strictEqual(resolvedNoteLabel(resolved), expectedNote, `${keyId} should resolve to ${expectedNote}`);
  }
});

test('does not map function keys as playable notes in the music layouts', () => {
  assert.strictEqual(typeof getNoteFromEventCode, 'function');

  for (const eventCode of ['F1', 'F2', 'F3', 'F5', 'F6', 'F7', 'F8']) {
    assert.strictEqual(getNoteFromEventCode?.(eventCode), undefined, `${eventCode} should be silent in classic layout`);
    assert.strictEqual(
      getNoteFromEventCode?.(eventCode, 'piano', DEFAULT_PIANO_LAYOUT_STATE),
      undefined,
      `${eventCode} should be silent in piano layout`,
    );
  }
});

test('keeps every playable layout key reachable from DOM events and mapped to a note', () => {
  const playableLayoutKeys = KEYBOARD_LAYOUT_ROWS
    .flat()
    .map((key) => key.id)
    .filter((keyId) => keyId !== 'Fn');

  for (const keyId of playableLayoutKeys) {
    const eventCode = KEY_ID_TO_EVENT_CODE[keyId];
    assert.ok(eventCode, `${keyId} is visible in the layout but has no DOM event mapping`);
    assert.ok(KEY_NOTE_MAP[keyId], `${keyId} is visible in the layout but has no note mapping`);
    assert.ok(getNoteFromEventCode?.(eventCode), `${keyId} resolves from DOM but does not produce a note`);
  }
});

test('does not advertise notes for layout keys without DOM event support', () => {
  const layoutKeysWithoutEvent = KEYBOARD_LAYOUT_ROWS
    .flat()
    .map((key) => key.id)
    .filter((keyId) => !KEY_ID_TO_EVENT_CODE[keyId]);

  assert.deepEqual(layoutKeysWithoutEvent, ['Fn']);

  for (const keyId of layoutKeysWithoutEvent) {
    assert.strictEqual(
      KEY_NOTE_MAP[keyId],
      undefined,
      `${keyId} should not advertise a note without DOM event support`,
    );
  }
});

test('assigns an auto-release fallback to lock-style music keys', () => {
  assert.strictEqual(typeof getMusicInputAutoReleaseMs, 'function');

  if (!getMusicInputAutoReleaseMs) {
    throw new Error('Expected getMusicInputAutoReleaseMs to be defined');
  }

  const capsLockAutoReleaseMs = getMusicInputAutoReleaseMs('CapsLock');
  assert.ok(
    typeof capsLockAutoReleaseMs === 'number' && capsLockAutoReleaseMs > 0,
    'CapsLock should auto-release to avoid stuck visual/note state',
  );
  assert.strictEqual(getMusicInputAutoReleaseMs('KeyA'), null);
});

test('resolves the alternate piano layout keys to the confirmed notes', () => {
  assert.strictEqual(typeof getNoteFromEventCode, 'function');

  const expectedNotes: Record<string, string> = {
    'S': 'C4',
    'D': 'D4',
    'F': 'E4',
    'J': 'F4',
    'K': 'G4',
    'L': 'A4',
    ';': 'B4',
    'W': 'C#4',
    'E': 'D#4',
    'R': 'D#4',
    'U': 'F#4',
    'I': 'G#4',
    'O': 'A#4',
    '[': 'C5',
  };

  for (const [keyId, expectedNote] of Object.entries(expectedNotes)) {
    const eventCode = KEY_ID_TO_EVENT_CODE[keyId];
    assert.ok(eventCode, `Missing DOM event code for ${keyId}`);
    const resolved = getNoteFromEventCode?.(eventCode, 'piano', DEFAULT_PIANO_LAYOUT_STATE);
    if (resolved === undefined) {
      throw new Error(`Missing alternate layout note for ${keyId}`);
    }
    assert.strictEqual(resolvedNoteLabel(resolved), expectedNote, `${keyId} should resolve to ${expectedNote}`);
  }
});

test('maps the lower piano row and number row to their default octaves', () => {
  assert.strictEqual(typeof getNoteFromEventCode, 'function');

  const expectedNotes: Record<string, string> = {
    'X': 'C3',
    'C': 'D3',
    'V': 'E3',
    'N': 'F3',
    'M': 'G3',
    ',': 'A3',
    '.': 'B3',
    '2': 'C5',
    '3': 'D5',
    '4': 'E5',
    '8': 'F5',
    '9': 'G5',
    '0': 'A5',
    '-': 'B5',
  };

  for (const [keyId, expectedNote] of Object.entries(expectedNotes)) {
    const eventCode = KEY_ID_TO_EVENT_CODE[keyId] ?? keyId;
    const resolved = getNoteFromEventCode?.(eventCode, 'piano', DEFAULT_PIANO_LAYOUT_STATE);
    if (resolved === undefined) {
      throw new Error(`Missing extended piano layout note for ${keyId}`);
    }
    assert.strictEqual(resolvedNoteLabel(resolved), expectedNote, `${keyId} should resolve to ${expectedNote}`);
  }
});

test('treats alternate piano octave controls per zone and keeps H inert', () => {
  assert.strictEqual(typeof getNoteFromEventCode, 'function');
  assert.strictEqual(typeof resolveMusicKeyboardEvent, 'function');

  assert.strictEqual(getNoteFromEventCode?.('CapsLock', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('KeyH', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('Enter', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('ShiftLeft', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('ShiftRight', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('Tab', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('Backslash', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);
  assert.strictEqual(getNoteFromEventCode?.('KeyP', 'piano', DEFAULT_PIANO_LAYOUT_STATE), undefined);

  assert.strictEqual(
    resolvedNoteLabel(getNoteFromEventCode?.('KeyS', 'piano', { ...DEFAULT_PIANO_LAYOUT_STATE, mainOctave: 3 })),
    'C3',
  );
  assert.strictEqual(
    resolvedNoteLabel(getNoteFromEventCode?.('KeyX', 'piano', { ...DEFAULT_PIANO_LAYOUT_STATE, lowerOctave: 2 })),
    'C2',
  );
  assert.strictEqual(
    resolvedNoteLabel(getNoteFromEventCode?.('Digit2', 'piano', { ...DEFAULT_PIANO_LAYOUT_STATE, numberOctave: 4 })),
    'C4',
  );

  assert.deepEqual(resolveMusicKeyboardEvent?.('CapsLock', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'mainOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Enter', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'mainOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('ShiftLeft', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'lowerOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('ShiftRight', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'lowerOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Tab', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'numberOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Backslash', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'numberOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('KeyP', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: null,
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('KeyH', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: null,
  });
});
