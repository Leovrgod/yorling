/**
 * Music keyboard mapping — maps physical keyboard keys to musical notes.
 *
 * Layout: the app's original custom note layout distributed across the
 * visible keyboard surface.
 *
 * Three ID systems are bridged here:
 *   1. Layout key ID  — matches KEYBOARD_LAYOUT_ROWS  ('A', '1', 'Space', …)
 *   2. DOM event code  — KeyboardEvent.code           ('KeyA', 'Digit1', 'Space', …)
 *   3. Note + Octave   — { note: 'C', octave: 4 }
 */

// ── Note names (chromatic) ──────────────────────────────────────────

export type NoteName =
  | 'C' | 'C#' | 'D' | 'D#' | 'E'
  | 'F' | 'F#' | 'G' | 'G#' | 'A' | 'A#' | 'B';

export const NOTE_SEMITONE: Record<NoteName, number> = {
  'C': 0, 'C#': 1, 'D': 2, 'D#': 3, 'E': 4, 'F': 5,
  'F#': 6, 'G': 7, 'G#': 8, 'A': 9, 'A#': 10, 'B': 11,
};

export interface NoteInfo {
  note: NoteName;
  octave: number;
  /** MIDI note number (C2 = 36, C4 = 60) */
  midi: number;
}

export interface MusicEventResolution {
  shouldSwallow: boolean;
  midi: number | null;
  controlAction: MusicKeyboardControlAction | null;
}

export type MusicKeyboardLayoutId = 'classic' | 'piano';
export type MusicKeyboardControlAction =
  | 'mainOctaveDown'
  | 'mainOctaveUp'
  | 'lowerOctaveDown'
  | 'lowerOctaveUp'
  | 'numberOctaveDown'
  | 'numberOctaveUp';

export interface PianoLayoutState {
  mainOctave: number;
  lowerOctave: number;
  numberOctave: number;
}

type PianoLayoutStateInput = number | Partial<PianoLayoutState>;

export interface MusicKeyboardInterceptorStatus {
  running: boolean;
  enabled: boolean;
}

export interface MusicNoteKeyBinding {
  kind: 'note';
  noteInfo: NoteInfo;
}

export interface MusicControlKeyBinding {
  kind: 'control';
  action: MusicKeyboardControlAction;
  display: string;
}

export type MusicKeyBinding = MusicNoteKeyBinding | MusicControlKeyBinding;

export const LOCK_KEY_AUTO_RELEASE_MS = 180;
export const DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID: MusicKeyboardLayoutId = 'classic';
export const DEFAULT_PIANO_LAYOUT_OCTAVE = 4;
export const DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE = 3;
export const DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE = 5;
export const MIN_PIANO_LAYOUT_OCTAVE = 0;
export const MAX_PIANO_LAYOUT_OCTAVE = 8;

export function isMusicKeyboardLayoutId(
  value: string | null | undefined,
): value is MusicKeyboardLayoutId {
  return value === 'classic' || value === 'piano';
}

export function clampPianoLayoutOctave(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_PIANO_LAYOUT_OCTAVE;
  }

  return Math.min(
    MAX_PIANO_LAYOUT_OCTAVE,
    Math.max(MIN_PIANO_LAYOUT_OCTAVE, Math.round(value)),
  );
}

export function normalizePianoLayoutState(
  value: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): PianoLayoutState {
  if (typeof value === 'number') {
    return {
      mainOctave: clampPianoLayoutOctave(value),
      lowerOctave: DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE,
      numberOctave: DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE,
    };
  }

  return {
    mainOctave: clampPianoLayoutOctave(value.mainOctave ?? DEFAULT_PIANO_LAYOUT_OCTAVE),
    lowerOctave: clampPianoLayoutOctave(value.lowerOctave ?? DEFAULT_PIANO_LOWER_LAYOUT_OCTAVE),
    numberOctave: clampPianoLayoutOctave(value.numberOctave ?? DEFAULT_PIANO_NUMBER_LAYOUT_OCTAVE),
  };
}

function n(note: NoteName, octave: number): NoteInfo {
  return { note, octave, midi: (octave + 1) * 12 + NOTE_SEMITONE[note] };
}

export const LOCKING_EVENT_CODES = new Set(['CapsLock']);

// ── Primary mapping: layout key ID → note ───────────────────────────

export const KEY_NOTE_MAP: Record<string, NoteInfo> = {
  // Number row
  '`':  n('F#', 5), '1': n('F#', 2), '2': n('F#', 3), '3': n('F#', 4),
  '4':  n('G', 5),  '5': n('G', 2),  '6': n('G#', 4),
  '7':  n('G#', 3), '8': n('G#', 2), '9': n('G#', 5),
  '0':  n('A', 5),  '-': n('A#', 2), '=': n('A#', 5), 'Backspace': n('B', 5),

  // Tab row
  'Tab': n('F', 5),  'Q': n('F', 2),  'W': n('F', 3),
  'E': n('F', 3),    'R': n('G', 3),  'T': n('G', 3),
  'Y': n('G', 3),    'U': n('G', 3),  'I': n('A', 3),
  'O': n('A', 2),    'P': n('A#', 4),
  '[': n('A#', 3),   ']': n('B', 3),   '\\': n('B', 2),

  // Caps row
  'Caps': n('C', 3), 'A': n('F', 3),
  'S': n('F', 4),    'D': n('F', 4),   'F': n('G', 4),
  'G': n('G', 4),    'H': n('G', 4),   'J': n('G', 4), 'K': n('A', 4),
  'L': n('A', 4),    ';': n('A', 4),   "'": n('A', 4), 'Enter': n('B', 4),

  // Shift row
  'Shift': n('C', 2),  'Z': n('C', 4),
  'X': n('C#', 3),     'C': n('C#', 4), 'V': n('D', 4),
  'B': n('D', 3),      'N': n('D', 2),
  'M': n('D#', 4),     ',': n('D#', 3), '.': n('E', 4),
  '/': n('E', 3),      'ShiftRight': n('E', 2),

  // Bottom row
  'Ctrl': n('C', 5),      'Opt': n('C#', 5),    'Cmd': n('C#', 2),
  'Space': n('D', 5),
  'CmdRight': n('D#', 2), 'OptRight': n('D#', 5),
};

function createNoteBindings(keyNoteMap: Record<string, NoteInfo>): Record<string, MusicKeyBinding> {
  return Object.entries(keyNoteMap).reduce<Record<string, MusicKeyBinding>>((bindings, [keyId, noteInfo]) => {
    bindings[keyId] = { kind: 'note', noteInfo };
    return bindings;
  }, {});
}

const CLASSIC_KEY_BINDINGS = createNoteBindings(KEY_NOTE_MAP);

const PIANO_MAIN_KEY_DEFS: Record<string, { note: NoteName; octaveOffset?: number }> = {
  'S': { note: 'C' },
  'D': { note: 'D' },
  'F': { note: 'E' },
  'J': { note: 'F' },
  'K': { note: 'G' },
  'L': { note: 'A' },
  ';': { note: 'B' },
  'W': { note: 'C#' },
  'E': { note: 'D#' },
  'R': { note: 'D#' },
  'U': { note: 'F#' },
  'I': { note: 'G#' },
  'O': { note: 'A#' },
  '[': { note: 'C', octaveOffset: 1 },
};

const PIANO_LOWER_KEY_DEFS: Record<string, { note: NoteName }> = {
  'X': { note: 'C' },
  'C': { note: 'D' },
  'V': { note: 'E' },
  'N': { note: 'F' },
  'M': { note: 'G' },
  ',': { note: 'A' },
  '.': { note: 'B' },
};

const PIANO_NUMBER_KEY_DEFS: Record<string, { note: NoteName }> = {
  '2': { note: 'C' },
  '3': { note: 'D' },
  '4': { note: 'E' },
  '8': { note: 'F' },
  '9': { note: 'G' },
  '0': { note: 'A' },
  '-': { note: 'B' },
};

function appendPianoZoneBindings(
  bindings: Record<string, MusicKeyBinding>,
  keyDefs: Record<string, { note: NoteName; octaveOffset?: number }>,
  octave: number,
): void {
  for (const [keyId, keyDef] of Object.entries(keyDefs)) {
    bindings[keyId] = {
      kind: 'note',
      noteInfo: n(keyDef.note, octave + (keyDef.octaveOffset ?? 0)),
    };
  }
}

function buildPianoKeyBindings(
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): Record<string, MusicKeyBinding> {
  const { mainOctave, lowerOctave, numberOctave } = normalizePianoLayoutState(pianoLayoutState);
  const bindings = {} as Record<string, MusicKeyBinding>;

  appendPianoZoneBindings(bindings, PIANO_MAIN_KEY_DEFS, mainOctave);
  appendPianoZoneBindings(bindings, PIANO_LOWER_KEY_DEFS, lowerOctave);
  appendPianoZoneBindings(bindings, PIANO_NUMBER_KEY_DEFS, numberOctave);

  bindings.Caps = {
    kind: 'control',
    action: 'mainOctaveDown',
    display: '-8va',
  };
  bindings.Enter = {
    kind: 'control',
    action: 'mainOctaveUp',
    display: '+8va',
  };
  bindings.Shift = {
    kind: 'control',
    action: 'lowerOctaveDown',
    display: '-8va',
  };
  bindings.ShiftRight = {
    kind: 'control',
    action: 'lowerOctaveUp',
    display: '+8va',
  };
  bindings.Tab = {
    kind: 'control',
    action: 'numberOctaveDown',
    display: '-8va',
  };
  bindings['\\'] = {
    kind: 'control',
    action: 'numberOctaveUp',
    display: '+8va',
  };

  return bindings;
}

// ── DOM event.code ↔ layout key ID conversion ──────────────────────

export const EVENT_CODE_TO_KEY_ID: Record<string, string> = {
  'Escape': 'Escape',
  'F1': 'F1', 'F2': 'F2', 'F3': 'F3', 'F4': 'F4',
  'F5': 'F5', 'F6': 'F6', 'F7': 'F7', 'F8': 'F8',
  'F9': 'F9', 'F10': 'F10', 'F11': 'F11', 'F12': 'F12',
  'Backquote': '`',
  'Digit1': '1', 'Digit2': '2', 'Digit3': '3', 'Digit4': '4', 'Digit5': '5',
  'Digit6': '6', 'Digit7': '7', 'Digit8': '8', 'Digit9': '9', 'Digit0': '0',
  'Minus': '-', 'Equal': '=', 'Backspace': 'Backspace',

  'Tab': 'Tab',
  'KeyQ': 'Q', 'KeyW': 'W', 'KeyE': 'E', 'KeyR': 'R', 'KeyT': 'T',
  'KeyY': 'Y', 'KeyU': 'U', 'KeyI': 'I', 'KeyO': 'O', 'KeyP': 'P',
  'BracketLeft': '[', 'BracketRight': ']', 'Backslash': '\\',

  'CapsLock': 'Caps',
  'KeyA': 'A', 'KeyS': 'S', 'KeyD': 'D', 'KeyF': 'F', 'KeyG': 'G',
  'KeyH': 'H', 'KeyJ': 'J', 'KeyK': 'K', 'KeyL': 'L',
  'Semicolon': ';', 'Quote': "'", 'Enter': 'Enter',

  'ShiftLeft': 'Shift',
  'KeyZ': 'Z', 'KeyX': 'X', 'KeyC': 'C', 'KeyV': 'V', 'KeyB': 'B',
  'KeyN': 'N', 'KeyM': 'M',
  'Comma': ',', 'Period': '.', 'Slash': '/',
  'ShiftRight': 'ShiftRight',

  'ControlLeft': 'Ctrl', 'AltLeft': 'Opt', 'MetaLeft': 'Cmd',
  'Space': 'Space',
  'MetaRight': 'CmdRight', 'AltRight': 'OptRight',
  // 'Fn' is not reliably available in DOM KeyboardEvent
};

/** Look up note info from a DOM KeyboardEvent.code. */
export function getMusicKeyBindings(
  layoutId: MusicKeyboardLayoutId = DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID,
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): Record<string, MusicKeyBinding> {
  return layoutId === 'piano'
    ? buildPianoKeyBindings(pianoLayoutState)
    : CLASSIC_KEY_BINDINGS;
}

export function getMusicKeyBinding(
  keyId: string,
  layoutId: MusicKeyboardLayoutId = DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID,
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): MusicKeyBinding | undefined {
  return getMusicKeyBindings(layoutId, pianoLayoutState)[keyId];
}

/** Look up note info from a DOM KeyboardEvent.code. */
export function getNoteFromEventCode(
  code: string,
  layoutId: MusicKeyboardLayoutId = DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID,
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): NoteInfo | undefined {
  const keyId = EVENT_CODE_TO_KEY_ID[code];
  if (!keyId) {
    return undefined;
  }

  const binding = getMusicKeyBinding(keyId, layoutId, pianoLayoutState);
  return binding?.kind === 'note' ? binding.noteInfo : undefined;
}

export function resolveMusicKeyboardEvent(
  code: string,
  purePlayActive: boolean,
  layoutId: MusicKeyboardLayoutId = DEFAULT_MUSIC_KEYBOARD_LAYOUT_ID,
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): MusicEventResolution {
  if (!purePlayActive) {
    return { shouldSwallow: false, midi: null, controlAction: null };
  }

  const keyId = EVENT_CODE_TO_KEY_ID[code];
  const binding = keyId ? getMusicKeyBinding(keyId, layoutId, pianoLayoutState) : undefined;

  return {
    shouldSwallow: true,
    midi: binding?.kind === 'note' ? binding.noteInfo.midi : null,
    controlAction: binding?.kind === 'control' ? binding.action : null,
  };
}

export function getMusicInputAutoReleaseMs(code: string): number | null {
  return LOCKING_EVENT_CODES.has(code) ? LOCK_KEY_AUTO_RELEASE_MS : null;
}

export function shouldPauseNativeKeyboardInterceptor(
  purePlayActive: boolean,
  status: MusicKeyboardInterceptorStatus,
): boolean {
  return purePlayActive && status.running && status.enabled;
}

export function shouldEnableMusicNativeKeySuppression(
  _layoutId: MusicKeyboardLayoutId,
  purePlayActive: boolean,
  audioUnlocked = true,
): boolean {
  return purePlayActive && audioUnlocked;
}

// ── Note display helpers ────────────────────────────────────────────

export function noteLabel(info: NoteInfo): string {
  return `${info.note}${info.octave}`;
}

const NOTE_NAME_BY_SEMITONE: readonly NoteName[] = [
  'C', 'C#', 'D', 'D#', 'E', 'F',
  'F#', 'G', 'G#', 'A', 'A#', 'B',
];

export function noteInfoFromMidi(midi: number): NoteInfo {
  const normalizedMidi = Math.round(midi);
  const pitchClass = ((normalizedMidi % 12) + 12) % 12;

  return {
    note: NOTE_NAME_BY_SEMITONE[pitchClass],
    octave: Math.floor(normalizedMidi / 12) - 1,
    midi: normalizedMidi,
  };
}

/** Frequency in Hz for a given MIDI note. */
export function midiToFrequency(midi: number): number {
  return 440 * Math.pow(2, (midi - 69) / 12);
}

// ── Note color CSS class suffix ─────────────────────────────────────

const NOTE_COLOR_CLASS: Record<NoteName, string> = {
  'C': 'c', 'C#': 'cs', 'D': 'd', 'D#': 'ds', 'E': 'e', 'F': 'f',
  'F#': 'fs', 'G': 'g', 'G#': 'gs', 'A': 'a', 'A#': 'as', 'B': 'b',
};

export function noteColorClass(note: NoteName): string {
  return `note-${NOTE_COLOR_CLASS[note]}`;
}

// ── Reverse mapping: MIDI → layout key ID ───────────────────────────

export function getMidiToKeyIdMap(
  layoutId: MusicKeyboardLayoutId,
  pianoLayoutState: PianoLayoutStateInput = DEFAULT_PIANO_LAYOUT_OCTAVE,
): Map<number, string> {
  const bindings = getMusicKeyBindings(layoutId, pianoLayoutState);
  const map = new Map<number, string>();

  for (const [keyId, binding] of Object.entries(bindings)) {
    if (binding.kind === 'note' && !map.has(binding.noteInfo.midi)) {
      map.set(binding.noteInfo.midi, keyId);
    }
  }

  return map;
}
