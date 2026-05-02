import abcjs from 'abcjs';
import { estimateTutorialSongDifficultyId } from './tutorialSongDifficulty.js';
import type { TutorialNote, TutorialSong, TutorialSongCategoryId } from './tutorialSongTypes.js';

interface LocalizedTitle {
  zh: string;
  en: string;
}

type SequenceNote = readonly [number, number];

export interface AbcSongDefinition {
  id: string;
  title: LocalizedTitle;
  bpm: number;
  abc: string;
  categoryId?: TutorialSongCategoryId;
}

interface AbcSequenceSongDefinition {
  id: string;
  title: LocalizedTitle;
  bpm: number;
  sequence: ReadonlyArray<SequenceNote>;
  categoryId?: TutorialSongCategoryId;
}

interface AbcTune {
  lines?: Array<{
    staff?: Array<{
      voices?: AbcVoiceElement[][];
    }>;
  }>;
  getBeatLength?: () => number;
  getBpm?: () => number;
  getKeySignature?: () => {
    accidentals?: Array<{
      acc?: string;
      note?: string;
    }>;
  };
}

type AbcVoiceElement =
  | {
      el_type: 'bar';
    }
  | {
      el_type: 'note';
      duration?: number;
      pitches?: Array<{
        pitch: number;
        accidental?: string;
      }>;
      rest?: {
        type: string;
      };
    };

const PITCH_CLASS_TO_ABC = ['C', '^C', 'D', '^D', 'E', 'F', '^F', 'G', '^G', 'A', '^A', 'B'];

const C4 = 60, D4 = 62, E4 = 64, F4 = 65, Fs4 = 66, G4 = 67, A4 = 69, Bb4 = 70, B4 = 71;
const C5 = 72, D5 = 74, Ds5 = 75, E5 = 76, F5 = 77, G5 = 79;
const Gs4 = 68;

function gcd(left: number, right: number): number {
  let a = Math.abs(left);
  let b = Math.abs(right);
  while (b !== 0) {
    const next = a % b;
    a = b;
    b = next;
  }
  return a || 1;
}

function durationToAbc(beats: number): string {
  const numerator = Math.round(beats * 4);
  const denominator = 4;
  const divisor = gcd(numerator, denominator);
  const normalizedNumerator = numerator / divisor;
  const normalizedDenominator = denominator / divisor;

  if (normalizedNumerator === normalizedDenominator) {
    return '';
  }

  if (normalizedDenominator === 1) {
    return String(normalizedNumerator);
  }

  if (normalizedNumerator === 1) {
    return `1/${normalizedDenominator}`;
  }

  return `${normalizedNumerator}/${normalizedDenominator}`;
}

function midiToAbcToken(midi: number): string {
  const pitchClass = ((midi % 12) + 12) % 12;
  const octave = Math.floor(midi / 12) - 1;
  const pitchToken = PITCH_CLASS_TO_ABC[pitchClass];
  const accidental = pitchToken.length > 1 ? pitchToken.slice(0, -1) : '';
  const letter = pitchToken[pitchToken.length - 1] ?? 'C';

  if (octave >= 5) {
    return accidental + letter.toLowerCase() + "'".repeat(octave - 5);
  }

  return accidental + letter.toUpperCase() + ','.repeat(Math.max(0, 4 - octave));
}

function buildAbcBodyFromSequence(sequence: ReadonlyArray<SequenceNote>): string {
  const measures: string[] = [];
  let currentMeasure: string[] = [];
  let beatsInMeasure = 0;

  for (const [midi, beats] of sequence) {
    const duration = durationToAbc(beats);
    const token = `${midi > 0 ? midiToAbcToken(midi) : 'z'}${duration}`;
    currentMeasure.push(token);
    beatsInMeasure += beats;

    if (Math.abs(beatsInMeasure - 4) < 0.0001) {
      measures.push(currentMeasure.join(' '));
      currentMeasure = [];
      beatsInMeasure = 0;
    }
  }

  if (currentMeasure.length > 0) {
    measures.push(currentMeasure.join(' '));
  }

  return measures.join(' | ');
}

function buildAbcFromSequence(definition: AbcSequenceSongDefinition): string {
  return [
    'X:1',
    `T:${definition.title.en}`,
    'M:4/4',
    'L:1/4',
    `Q:1/4=${definition.bpm}`,
    'K:C',
    buildAbcBodyFromSequence(definition.sequence),
  ].join('\n');
}

const LETTERS = ['C', 'D', 'E', 'F', 'G', 'A', 'B'] as const;
const NATURAL_SEMITONES: Record<(typeof LETTERS)[number], number> = {
  C: 0,
  D: 2,
  E: 4,
  F: 5,
  G: 7,
  A: 9,
  B: 11,
};

function floorDiv(value: number, divisor: number) {
  return Math.floor(value / divisor);
}

function normalizeLetterForPitch(pitch: number) {
  return LETTERS[((pitch % 7) + 7) % 7];
}

function normalizeOctaveForPitch(pitch: number) {
  return 4 + floorDiv(pitch, 7);
}

function accidentalToOffset(accidental: string | undefined) {
  switch (accidental) {
    case 'sharp':
      return 1;
    case 'flat':
      return -1;
    case 'dblsharp':
      return 2;
    case 'dblflat':
      return -2;
    case 'natural':
    default:
      return 0;
  }
}

function getKeySignatureOffsets(tune: AbcTune) {
  const offsets = new Map<string, number>();
  for (const accidental of tune.getKeySignature?.()?.accidentals ?? []) {
    const note = accidental.note?.trim().charAt(0).toUpperCase();
    if (!note) {
      continue;
    }
    offsets.set(note, accidentalToOffset(accidental.acc));
  }
  return offsets;
}

function resolveMidiFromPitch(
  pitch: { pitch: number; accidental?: string },
  keySignatureOffsets: ReadonlyMap<string, number>,
  measureAccidentals: Map<string, number>,
) {
  const letter = normalizeLetterForPitch(pitch.pitch);
  const octave = normalizeOctaveForPitch(pitch.pitch);
  const measureKey = `${letter}:${octave}`;
  const explicitOffset =
    pitch.accidental === undefined ? null : accidentalToOffset(pitch.accidental);

  if (explicitOffset !== null) {
    measureAccidentals.set(measureKey, explicitOffset);
  }

  const accidentalOffset =
    explicitOffset
    ?? measureAccidentals.get(measureKey)
    ?? keySignatureOffsets.get(letter)
    ?? 0;

  return (octave + 1) * 12 + NATURAL_SEMITONES[letter] + accidentalOffset;
}

function roundTiming(value: number) {
  return Number(value.toFixed(6));
}

function extractTutorialNotesFromAbc(definition: AbcSongDefinition) {
  const tune = abcjs.parseOnly(definition.abc)?.[0] as AbcTune | undefined;
  if (!tune) {
    throw new Error(`Unable to parse ABC song "${definition.id}".`);
  }

  const bpm = Math.round(tune.getBpm?.() ?? definition.bpm ?? 120);
  const beatLength = tune.getBeatLength?.() ?? 0.25;
  const secondsPerWholeNote = (60 / bpm) / beatLength;
  const keySignatureOffsets = getKeySignatureOffsets(tune);
  const notes: TutorialNote[] = [];
  const voiceState = new Map<string, { wholeNoteOffset: number; measureAccidentals: Map<string, number> }>();

  for (const [lineIndex, line] of (tune.lines ?? []).entries()) {
    for (const [staffIndex, staff] of (line.staff ?? []).entries()) {
      for (const [voiceIndex, voice] of (staff.voices ?? []).entries()) {
        const voiceKey = `${staffIndex}:${voiceIndex}`;
        const state = voiceState.get(voiceKey) ?? {
          wholeNoteOffset: 0,
          measureAccidentals: new Map<string, number>(),
        };

        for (const item of voice) {
          if (item.el_type === 'bar') {
            state.measureAccidentals.clear();
            continue;
          }

          if (item.el_type !== 'note') {
            continue;
          }

          const durationWhole = Math.max(0, item.duration ?? 0);
          if (item.rest) {
            state.wholeNoteOffset += durationWhole;
            continue;
          }

          const time = roundTiming(state.wholeNoteOffset * secondsPerWholeNote);
          const duration = roundTiming(durationWhole * secondsPerWholeNote);

          for (const pitch of item.pitches ?? []) {
            notes.push({
              midi: resolveMidiFromPitch(pitch, keySignatureOffsets, state.measureAccidentals),
              time,
              duration,
            });
          }

          state.wholeNoteOffset += durationWhole;
        }

        voiceState.set(voiceKey, state);
      }
    }
    void lineIndex;
  }

  notes.sort((left, right) => left.time - right.time || left.midi - right.midi);

  return {
    bpm,
    notes,
    duration: notes.length > 0
      ? Math.max(...notes.map((note) => note.time + note.duration))
      : 0,
  };
}

export function buildTutorialSongFromAbc(definition: AbcSongDefinition): TutorialSong {
  const { bpm, notes, duration } = extractTutorialNotesFromAbc(definition);

  return {
    id: definition.id,
    title: definition.title,
    bpm,
    notes,
    duration,
    source: 'builtin-abc',
    categoryId: definition.categoryId ?? 'featured-favorites',
    difficultyId: estimateTutorialSongDifficultyId({ notes, duration, bpm }),
  };
}

function buildTutorialSongFromSequence(definition: AbcSequenceSongDefinition): TutorialSong {
  return buildTutorialSongFromAbc({
    id: definition.id,
    title: definition.title,
    bpm: definition.bpm,
    abc: buildAbcFromSequence(definition),
    categoryId: definition.categoryId ?? 'featured-favorites',
  });
}

const SONG_DEFINITIONS: AbcSequenceSongDefinition[] = [
  {
    id: 'twinkle',
    title: { zh: '小星星', en: 'Twinkle Twinkle' },
    bpm: 100,
    sequence: [
      [C4, 1], [C4, 1], [G4, 1], [G4, 1], [A4, 1], [A4, 1], [G4, 2],
      [F4, 1], [F4, 1], [E4, 1], [E4, 1], [D4, 1], [D4, 1], [C4, 2],
      [G4, 1], [G4, 1], [F4, 1], [F4, 1], [E4, 1], [E4, 1], [D4, 2],
      [G4, 1], [G4, 1], [F4, 1], [F4, 1], [E4, 1], [E4, 1], [D4, 2],
      [C4, 1], [C4, 1], [G4, 1], [G4, 1], [A4, 1], [A4, 1], [G4, 2],
      [F4, 1], [F4, 1], [E4, 1], [E4, 1], [D4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'ode-to-joy',
    title: { zh: '欢乐颂', en: 'Ode to Joy' },
    bpm: 108,
    sequence: [
      [E4, 1], [E4, 1], [F4, 1], [G4, 1], [G4, 1], [F4, 1], [E4, 1], [D4, 1],
      [C4, 1], [C4, 1], [D4, 1], [E4, 1], [E4, 1.5], [D4, 0.5], [D4, 2],
      [E4, 1], [E4, 1], [F4, 1], [G4, 1], [G4, 1], [F4, 1], [E4, 1], [D4, 1],
      [C4, 1], [C4, 1], [D4, 1], [E4, 1], [D4, 1.5], [C4, 0.5], [C4, 2],
      [D4, 1], [D4, 1], [E4, 1], [C4, 1], [D4, 1], [E4, 0.5], [F4, 0.5], [E4, 1], [C4, 1],
      [D4, 1], [E4, 0.5], [F4, 0.5], [E4, 1], [D4, 1], [C4, 1], [D4, 1], [G4, 2],
      [E4, 1], [E4, 1], [F4, 1], [G4, 1], [G4, 1], [F4, 1], [E4, 1], [D4, 1],
      [C4, 1], [C4, 1], [D4, 1], [E4, 1], [D4, 1.5], [C4, 0.5], [C4, 2],
    ],
  },
  {
    id: 'mary',
    title: { zh: '玛丽有只小羊羔', en: 'Mary Had a Little Lamb' },
    bpm: 120,
    sequence: [
      [E4, 1], [D4, 1], [C4, 1], [D4, 1], [E4, 1], [E4, 1], [E4, 2],
      [D4, 1], [D4, 1], [D4, 2], [E4, 1], [G4, 1], [G4, 2],
      [E4, 1], [D4, 1], [C4, 1], [D4, 1], [E4, 1], [E4, 1], [E4, 1], [E4, 1],
      [D4, 1], [D4, 1], [E4, 1], [D4, 1], [C4, 2], [0, 2],
    ],
  },
  {
    id: 'fur-elise',
    title: { zh: '致爱丽丝', en: 'Für Elise' },
    bpm: 130,
    sequence: [
      [E5, 0.5], [Ds5, 0.5], [E5, 0.5], [Ds5, 0.5], [E5, 0.5], [B4, 0.5], [D5, 0.5], [C5, 0.5],
      [A4, 1], [0, 0.5], [C4, 0.5], [E4, 0.5], [A4, 0.5], [B4, 1],
      [0, 0.5], [E4, 0.5], [Gs4, 0.5], [B4, 0.5], [C5, 1],
      [0, 0.5], [E4, 0.5], [E5, 0.5], [Ds5, 0.5], [E5, 0.5], [Ds5, 0.5], [E5, 0.5], [B4, 0.5], [D5, 0.5], [C5, 0.5],
      [A4, 1], [0, 0.5], [C4, 0.5], [E4, 0.5], [A4, 0.5], [B4, 1],
      [0, 0.5], [E4, 0.5], [C5, 0.5], [B4, 0.5], [A4, 1], [0, 1],
    ],
  },
  {
    id: 'row-row-row-your-boat',
    title: { zh: '划呀划呀划小船', en: 'Row Row Row Your Boat' },
    bpm: 108,
    sequence: [
      [C4, 1], [C4, 1], [C4, 1], [D4, 1], [E4, 2],
      [E4, 1], [D4, 1], [E4, 1], [F4, 1], [G4, 2],
      [C5, 0.75], [C5, 0.75], [C5, 0.5], [G4, 0.75], [G4, 0.75], [G4, 0.5],
      [E4, 0.75], [E4, 0.75], [E4, 0.5], [C4, 1], [C4, 1], [C4, 1],
      [G4, 1], [F4, 1], [E4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'london-bridge',
    title: { zh: '伦敦桥', en: 'London Bridge' },
    bpm: 116,
    sequence: [
      [G4, 1], [A4, 1], [G4, 1], [F4, 1], [E4, 1], [F4, 1], [G4, 2],
      [D4, 1], [E4, 1], [F4, 2],
      [E4, 1], [F4, 1], [G4, 2],
      [G4, 1], [A4, 1], [G4, 1], [F4, 1], [E4, 1], [F4, 1], [G4, 2],
      [D4, 2], [G4, 2], [E4, 2], [C4, 2],
    ],
  },
  {
    id: 'jingle-bells',
    title: { zh: '铃儿响叮当', en: 'Jingle Bells' },
    bpm: 118,
    sequence: [
      [E4, 1], [E4, 1], [E4, 2],
      [E4, 1], [E4, 1], [E4, 2],
      [E4, 1], [G4, 1], [C4, 1], [D4, 1], [E4, 4],
      [F4, 1], [F4, 1], [F4, 1], [F4, 1], [F4, 1], [E4, 1], [E4, 1], [E4, 1],
      [E4, 1], [D4, 1], [D4, 1], [E4, 1], [D4, 2], [G4, 2],
    ],
  },
  {
    id: 'silent-night',
    title: { zh: '平安夜', en: 'Silent Night' },
    bpm: 96,
    sequence: [
      [G4, 1], [A4, 1], [G4, 1.5], [E4, 0.5],
      [G4, 1], [A4, 1], [G4, 1.5], [E4, 0.5],
      [D5, 2], [D5, 1], [B4, 1],
      [C5, 2], [C5, 1], [G4, 1],
      [A4, 2], [A4, 1], [C5, 1],
      [B4, 1.5], [A4, 0.5], [G4, 2],
      [A4, 1], [A4, 1], [C5, 1], [B4, 1],
      [A4, 1.5], [G4, 0.5], [E4, 2],
      [G4, 1], [A4, 1], [G4, 1.5], [E4, 0.5],
      [A4, 1], [F4, 1], [C4, 2],
      [G4, 1.5], [E4, 0.5], [C4, 2],
    ],
  },
  {
    id: 'greensleeves',
    title: { zh: '绿袖子', en: 'Greensleeves' },
    bpm: 92,
    sequence: [
      [A4, 1], [C5, 1], [D5, 1], [E5, 1],
      [F5, 2], [E5, 1], [D5, 1],
      [B4, 1], [G4, 1], [A4, 1], [B4, 1],
      [C5, 2], [A4, 1], [G4, 1],
      [E4, 1], [G4, 1], [A4, 1], [B4, 1],
      [C5, 1], [B4, 1], [A4, 1], [G4, 1],
      [A4, 1.5], [G4, 0.5], [E4, 2],
      [A4, 1], [C5, 1], [D5, 1], [E5, 1],
      [F5, 2], [E5, 1], [D5, 1],
      [C5, 1], [B4, 1], [A4, 1], [G4, 1], [A4, 2],
    ],
  },
  {
    id: 'auld-lang-syne',
    title: { zh: '友谊地久天长', en: 'Auld Lang Syne' },
    bpm: 100,
    sequence: [
      [C4, 1], [F4, 1], [F4, 1], [A4, 1], [G4, 2],
      [F4, 1], [G4, 1], [A4, 1], [G4, 1], [F4, 2],
      [D4, 1], [D4, 1], [F4, 1], [C4, 1], [D4, 2],
      [F4, 1], [F4, 1], [A4, 1], [G4, 1], [F4, 2],
      [A4, 1], [C5, 1], [D5, 1], [C5, 1], [A4, 2],
      [G4, 1], [F4, 1], [G4, 1], [C5, 1], [A4, 2],
      [F4, 1], [D4, 1], [D4, 1], [C4, 1], [0, 2],
    ],
  },
  {
    id: 'happy-birthday',
    title: { zh: '生日快乐', en: 'Happy Birthday' },
    bpm: 120,
    sequence: [
      [G4, 0.75], [G4, 0.25], [A4, 1], [G4, 1], [C5, 1], [B4, 2],
      [G4, 0.75], [G4, 0.25], [A4, 1], [G4, 1], [D5, 1], [C5, 2],
      [G4, 0.75], [G4, 0.25], [G5, 1], [E5, 1], [C5, 1], [B4, 1], [A4, 2],
      [F5, 0.75], [F5, 0.25], [E5, 1], [C5, 1], [D5, 1], [C5, 2],
    ],
  },
  {
    id: 'frere-jacques',
    title: { zh: '两只老虎', en: 'Frère Jacques' },
    bpm: 120,
    sequence: [
      [G4, 1], [A4, 1], [B4, 1], [G4, 1],
      [G4, 1], [A4, 1], [B4, 1], [G4, 1],
      [B4, 1], [C5, 1], [D5, 2],
      [B4, 1], [C5, 1], [D5, 2],
      [D5, 0.5], [E5, 0.5], [D5, 0.5], [C5, 0.5], [B4, 1], [G4, 1],
      [D5, 0.5], [E5, 0.5], [D5, 0.5], [C5, 0.5], [B4, 1], [G4, 1],
      [G4, 1], [D4, 1], [G4, 2],
      [G4, 1], [D4, 1], [G4, 2],
    ],
  },
  {
    id: 'hot-cross-buns',
    title: { zh: '热十字面包', en: 'Hot Cross Buns' },
    bpm: 100,
    sequence: [
      [E4, 1], [D4, 1], [C4, 2],
      [E4, 1], [D4, 1], [C4, 2],
      [C4, 0.5], [C4, 0.5], [C4, 0.5], [C4, 0.5], [D4, 0.5], [D4, 0.5], [D4, 0.5], [D4, 0.5],
      [E4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'three-blind-mice',
    title: { zh: '三只瞎老鼠', en: 'Three Blind Mice' },
    bpm: 110,
    sequence: [
      [E4, 1], [D4, 1], [C4, 2],
      [E4, 1], [D4, 1], [C4, 2],
      [G4, 1], [F4, 1], [F4, 1], [E4, 1],
      [G4, 1], [F4, 1], [F4, 1], [E4, 1],
      [G4, 0.5], [G4, 0.5], [A4, 0.5], [G4, 0.5], [F4, 1], [E4, 1],
      [D4, 1], [E4, 1], [C4, 2],
    ],
  },
  {
    id: 'lightly-row',
    title: { zh: '小蜜蜂', en: 'Lightly Row' },
    bpm: 110,
    sequence: [
      [E4, 1], [C4, 1], [E4, 1], [C4, 1],
      [F4, 1], [E4, 1], [D4, 2],
      [C4, 1], [D4, 1], [E4, 1], [F4, 1],
      [G4, 1], [G4, 1], [G4, 2],
      [A4, 1], [F4, 1], [A4, 1], [F4, 1],
      [G4, 1], [F4, 1], [E4, 2],
      [C4, 1], [E4, 1], [G4, 1], [E4, 1],
      [D4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'amazing-grace',
    title: { zh: '奇异恩典', en: 'Amazing Grace' },
    bpm: 80,
    sequence: [
      [C4, 1], [F4, 1.5], [A4, 0.5], [F4, 1],
      [A4, 2], [G4, 1], [F4, 2], [D4, 1],
      [C4, 2], [C4, 1], [F4, 1.5], [A4, 0.5],
      [F4, 1], [A4, 2], [G4, 1], [C5, 3],
      [C5, 1], [A4, 1.5], [F4, 0.5], [G4, 1],
      [F4, 1.5], [D4, 0.5], [C4, 2], [C4, 1],
      [F4, 1.5], [A4, 0.5], [F4, 1], [G4, 2],
      [F4, 1], [F4, 3],
    ],
  },
  {
    id: 'when-the-saints',
    title: { zh: '当圣徒前进时', en: 'When the Saints Go Marching In' },
    bpm: 116,
    sequence: [
      [C4, 1], [E4, 1], [F4, 1], [G4, 4], [0, 1],
      [C4, 1], [E4, 1], [F4, 1], [G4, 4], [0, 1],
      [C4, 1], [E4, 1], [F4, 1], [G4, 2], [E4, 2],
      [C4, 2], [E4, 2], [D4, 4],
      [E4, 2], [E4, 1], [D4, 1], [C4, 2], [C4, 1], [E4, 1],
      [G4, 2], [G4, 2], [F4, 4],
      [E4, 1], [F4, 1], [G4, 2], [E4, 2], [C4, 2],
      [D4, 2], [C4, 4], [0, 2],
    ],
  },
  {
    id: 'oh-susanna',
    title: { zh: '噢！苏珊娜', en: 'Oh! Susanna' },
    bpm: 120,
    sequence: [
      [C4, 0.5], [D4, 0.5], [E4, 1], [G4, 1], [G4, 0.5], [A4, 0.5], [G4, 1], [E4, 1],
      [C4, 0.5], [D4, 0.5], [E4, 1], [E4, 1], [D4, 1], [C4, 1], [D4, 2],
      [C4, 0.5], [D4, 0.5], [E4, 1], [G4, 1], [G4, 0.5], [A4, 0.5], [G4, 1], [E4, 1],
      [C4, 0.5], [D4, 0.5], [E4, 1], [E4, 1], [D4, 1], [D4, 1], [C4, 2],
      [F4, 2], [A4, 2], [A4, 1], [G4, 1], [E4, 1], [G4, 1],
      [C4, 0.5], [D4, 0.5], [E4, 1], [G4, 1], [G4, 0.5], [A4, 0.5], [G4, 1], [E4, 1],
      [C4, 0.5], [D4, 0.5], [E4, 1], [E4, 1], [D4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'yankee-doodle',
    title: { zh: '扬基歌', en: 'Yankee Doodle' },
    bpm: 120,
    sequence: [
      [C5, 1], [C5, 1], [D5, 1], [E5, 1],
      [C5, 1], [E5, 1], [D5, 2],
      [C5, 1], [C5, 1], [D5, 1], [E5, 1],
      [C5, 2], [B4, 2],
      [A4, 1], [B4, 1], [C5, 1], [D5, 1],
      [E5, 1], [D5, 1], [C5, 1], [B4, 1],
      [A4, 1], [G4, 1], [A4, 1], [B4, 1],
      [C5, 2], [C5, 2],
    ],
  },
  {
    id: 'old-macdonald',
    title: { zh: '老麦克唐纳有个农场', en: 'Old MacDonald Had a Farm' },
    bpm: 120,
    sequence: [
      [G4, 1], [G4, 1], [G4, 1], [D4, 1],
      [E4, 1], [E4, 1], [D4, 2],
      [B4, 1], [B4, 1], [A4, 1], [A4, 1],
      [G4, 2], [0, 2],
      [D4, 0.5], [D4, 0.5], [G4, 1], [G4, 1], [G4, 1],
      [D4, 1], [E4, 1], [E4, 1], [D4, 2],
      [B4, 1], [B4, 1], [A4, 1], [A4, 1],
      [G4, 2], [0, 2],
    ],
  },
  {
    id: 'scarborough-fair',
    title: { zh: '斯卡布罗集市', en: 'Scarborough Fair' },
    bpm: 88,
    sequence: [
      [D4, 2], [D4, 1], [D4, 2], [A4, 1],
      [A4, 2], [E4, 1], [F4, 1.5], [E4, 0.5], [D4, 1],
      [A4, 1], [C5, 1], [D5, 1], [D5, 2], [C5, 1],
      [A4, 2], [A4, 1], [B4, 2], [G4, 1],
      [A4, 3], [D4, 2], [D4, 1],
      [F4, 1], [E4, 1], [C4, 1], [D4, 3],
    ],
  },
  {
    id: 'brahms-lullaby',
    title: { zh: '摇篮曲', en: "Brahms' Lullaby" },
    bpm: 76,
    sequence: [
      [E4, 0.5], [E4, 0.5], [G4, 1.5], [E4, 0.5], [G4, 2],
      [E4, 0.5], [E4, 0.5], [G4, 1.5], [A4, 0.5], [G4, 2],
      [C5, 2], [A4, 1.5], [C5, 0.5],
      [B4, 1.5], [G4, 0.5], [A4, 1], [G4, 1],
      [E4, 0.5], [E4, 0.5], [G4, 1.5], [E4, 0.5], [G4, 2],
      [E4, 0.5], [E4, 0.5], [G4, 1.5], [A4, 0.5], [G4, 2],
      [C5, 2], [B4, 1], [A4, 1],
      [G4, 1], [D4, 1], [C4, 2],
    ],
  },
  {
    id: 'sakura',
    title: { zh: '樱花', en: 'Sakura Sakura' },
    bpm: 80,
    sequence: [
      [A4, 1], [A4, 1], [B4, 2],
      [A4, 1], [A4, 1], [B4, 2],
      [A4, 1], [B4, 1], [C5, 1], [B4, 1],
      [A4, 1], [B4, 1], [A4, 1], [Gs4, 1],
      [E4, 2], [Gs4, 2],
      [A4, 1], [B4, 1], [C5, 1], [B4, 1],
      [A4, 1], [B4, 1], [A4, 1], [Gs4, 1],
      [E4, 2], [Gs4, 2],
      [A4, 1], [A4, 1], [B4, 2],
      [A4, 2], [Gs4, 2],
      [E4, 4],
    ],
  },
  {
    id: 'la-cucaracha',
    title: { zh: '蟑螂之歌', en: 'La Cucaracha' },
    bpm: 140,
    sequence: [
      [C4, 0.5], [C4, 0.5], [C4, 0.5], [F4, 0.5], [A4, 2],
      [C4, 0.5], [C4, 0.5], [C4, 0.5], [F4, 0.5], [A4, 2],
      [F4, 1], [F4, 1], [E4, 1], [E4, 1],
      [D4, 1], [D4, 1], [C4, 2],
      [C4, 0.5], [C4, 0.5], [C4, 0.5], [E4, 0.5], [G4, 2],
      [C4, 0.5], [C4, 0.5], [C4, 0.5], [E4, 0.5], [G4, 2],
      [E4, 1], [E4, 1], [D4, 1], [D4, 1],
      [C4, 1], [C4, 1], [F4, 2],
    ],
  },
  {
    id: 'o-christmas-tree',
    title: { zh: '圣诞树', en: 'O Christmas Tree' },
    bpm: 100,
    sequence: [
      [C4, 1], [F4, 1.5], [F4, 0.5], [F4, 1],
      [G4, 1], [A4, 1.5], [A4, 0.5], [A4, 1],
      [G4, 1], [A4, 1], [Bb4, 1],
      [E4, 1.5], [G4, 0.5], [F4, 2],
      [C5, 1], [C5, 1], [C5, 1.5], [A4, 0.5],
      [D5, 2], [C5, 2],
      [C5, 1], [Bb4, 1], [A4, 1], [G4, 1],
      [C4, 1], [F4, 1.5], [F4, 0.5], [F4, 1],
      [G4, 1], [A4, 1.5], [A4, 0.5], [A4, 1],
      [G4, 1], [A4, 1], [Bb4, 1],
      [E4, 1.5], [G4, 0.5], [F4, 2],
    ],
  },
  {
    id: 'pop-goes-the-weasel',
    title: { zh: '黄鼠狼跳出来', en: 'Pop Goes the Weasel' },
    bpm: 120,
    sequence: [
      [C4, 0.5], [E4, 0.5], [E4, 0.5], [G4, 0.5], [E4, 0.5], [G4, 0.5],
      [A4, 1], [A4, 0.5], [F4, 0.5], [D4, 0.5], [F4, 0.5],
      [E4, 0.5], [G4, 0.5], [E4, 0.5], [G4, 0.5],
      [A4, 1], [G4, 1], [E4, 1], [C4, 1],
      [C4, 0.5], [E4, 0.5], [E4, 0.5], [G4, 0.5], [E4, 0.5], [G4, 0.5],
      [A4, 1], [A4, 0.5], [F4, 0.5], [A4, 0.5], [G4, 0.5],
      [C5, 2], [C4, 2],
    ],
  },
  {
    id: 'aura-lee',
    title: { zh: '奥拉·李', en: 'Aura Lee' },
    bpm: 100,
    sequence: [
      [C4, 1], [D4, 1], [E4, 1], [F4, 1],
      [G4, 2], [A4, 2],
      [G4, 1], [E4, 1], [D4, 1], [F4, 1],
      [E4, 4],
      [C4, 1], [D4, 1], [E4, 1], [F4, 1],
      [G4, 2], [A4, 2],
      [G4, 1], [E4, 1], [D4, 1], [F4, 1],
      [C4, 4],
      [G4, 2], [G4, 2],
      [G4, 1], [A4, 1], [B4, 1], [G4, 1],
      [A4, 4],
      [C4, 1], [D4, 1], [E4, 1], [F4, 1],
      [G4, 2], [A4, 2],
      [G4, 1], [E4, 1], [D4, 1], [F4, 1],
      [C4, 4],
    ],
  },
  {
    id: 'danny-boy',
    title: { zh: '丹尼男孩', en: 'Danny Boy' },
    bpm: 80,
    sequence: [
      [C4, 1], [F4, 1.5], [G4, 0.5], [A4, 1.5], [G4, 0.5],
      [A4, 1], [C5, 1], [D5, 1], [C5, 1],
      [A4, 1.5], [G4, 0.5], [F4, 1.5], [G4, 0.5],
      [A4, 2], [F4, 1], [D4, 1],
      [C4, 1], [F4, 1.5], [G4, 0.5], [A4, 1.5], [G4, 0.5],
      [A4, 1], [C5, 1], [D5, 2],
      [F5, 2], [E5, 1.5], [D5, 0.5],
      [C5, 2], [A4, 1], [C5, 1],
      [D5, 2], [C5, 1], [A4, 1],
      [F4, 1.5], [G4, 0.5], [F4, 1.5], [D4, 0.5],
      [C4, 2], [F4, 2],
    ],
  },
  {
    id: 'camptown-races',
    title: { zh: '坎伯顿赛马', en: 'Camptown Races' },
    bpm: 120,
    sequence: [
      [G4, 1], [G4, 1], [E4, 1], [G4, 1],
      [A4, 1], [G4, 1], [E4, 2],
      [E4, 1], [D4, 1], [E4, 1], [D4, 1],
      [C4, 2], [0, 2],
      [G4, 1], [G4, 1], [E4, 1], [G4, 1],
      [A4, 1], [G4, 1], [E4, 2],
      [E4, 1], [D4, 1], [E4, 1], [D4, 1],
      [C4, 2], [0, 2],
      [C4, 2], [C4, 2],
      [D4, 1], [E4, 1], [F4, 1], [D4, 1],
      [G4, 1], [G4, 1], [E4, 1], [G4, 1],
      [A4, 1], [G4, 1], [E4, 2],
      [E4, 1], [D4, 1], [E4, 1], [D4, 1],
      [C4, 2], [0, 2],
    ],
  },
  {
    id: 'drunken-sailor',
    title: { zh: '醉水手', en: 'Drunken Sailor' },
    bpm: 130,
    sequence: [
      [E4, 0.5], [E4, 0.5], [E4, 0.5], [E4, 0.5], [E4, 0.5], [F4, 0.5], [G4, 0.5], [A4, 0.5],
      [G4, 0.5], [G4, 0.5], [G4, 0.5], [G4, 0.5], [G4, 0.5], [A4, 0.5], [B4, 0.5], [C5, 0.5],
      [E4, 0.5], [E4, 0.5], [E4, 0.5], [E4, 0.5], [E4, 0.5], [F4, 0.5], [G4, 0.5], [A4, 0.5],
      [G4, 1], [F4, 1], [E4, 1], [D4, 1],
      [E4, 2], [0, 2],
      [A4, 2], [A4, 2],
      [A4, 1], [G4, 1], [F4, 1], [E4, 1],
      [G4, 1], [F4, 1], [E4, 1], [D4, 1],
      [E4, 2], [0, 2],
    ],
  },
  {
    id: 'korobushka',
    title: { zh: '货郎', en: 'Korobushka' },
    bpm: 130,
    sequence: [
      [E5, 1], [B4, 0.5], [C5, 0.5], [D5, 1], [C5, 0.5], [B4, 0.5],
      [A4, 1], [A4, 0.5], [C5, 0.5], [E5, 1], [D5, 0.5], [C5, 0.5],
      [B4, 1], [0, 0.5], [C5, 0.5], [D5, 1], [E5, 1],
      [C5, 1], [A4, 1], [A4, 2],
      [0, 0.5], [D5, 1], [0, 0.5], [F5, 1], [A4, 0.5], [G5, 0.5],
      [F5, 1], [0, 0.5], [C5, 0.5], [E5, 1], [D5, 0.5], [C5, 0.5],
      [B4, 1], [0, 0.5], [C5, 0.5], [D5, 1], [E5, 1],
      [C5, 1], [A4, 1], [A4, 2],
    ],
  },
  {
    id: 'home-on-the-range',
    title: { zh: '牧场是我家', en: 'Home on the Range' },
    bpm: 100,
    sequence: [
      [C4, 1], [C4, 1], [E4, 2],
      [G4, 1.5], [A4, 0.5], [G4, 1.5], [E4, 0.5],
      [F4, 1], [A4, 1], [C5, 2],
      [C5, 1], [B4, 1], [A4, 1], [G4, 1],
      [A4, 2], [G4, 2],
      [C4, 1], [C4, 1], [E4, 2],
      [G4, 1.5], [A4, 0.5], [G4, 1.5], [E4, 0.5],
      [F4, 1], [A4, 1], [C5, 2],
      [C5, 1], [B4, 1], [A4, 1], [B4, 1],
      [C5, 4],
    ],
  },
  {
    id: 'minuet-in-g',
    title: { zh: 'G大调小步舞曲', en: 'Minuet in G' },
    bpm: 108,
    sequence: [
      [D5, 1], [G4, 0.5], [A4, 0.5], [B4, 0.5], [C5, 0.5],
      [D5, 1], [G4, 1], [G4, 2],
      [E5, 1], [C5, 0.5], [D5, 0.5], [E5, 0.5], [Fs4, 0.5],
      [G5, 1], [G4, 1], [G4, 2],
      [C5, 1], [D5, 0.5], [C5, 0.5], [B4, 0.5], [A4, 0.5],
      [B4, 1], [C5, 0.5], [B4, 0.5], [A4, 0.5], [G4, 0.5],
      [Fs4, 1], [G4, 0.5], [A4, 0.5], [B4, 0.5], [G4, 0.5],
      [A4, 2], [D4, 2],
    ],
  },
  {
    id: 'can-can',
    title: { zh: '康康舞曲', en: 'Can Can' },
    bpm: 140,
    sequence: [
      [E4, 0.5], [E4, 0.5], [F4, 0.5], [G4, 0.5], [G4, 0.5], [F4, 0.5], [E4, 0.5], [D4, 0.5],
      [E4, 0.5], [F4, 0.5], [G4, 0.5], [E4, 0.5], [C4, 2],
      [B4, 0.5], [C5, 0.5], [B4, 0.5], [A4, 0.5], [G4, 0.5], [A4, 0.5], [G4, 0.5], [F4, 0.5],
      [E4, 0.5], [F4, 0.5], [G4, 0.5], [E4, 0.5], [C4, 2],
      [E4, 0.5], [E4, 0.5], [F4, 0.5], [G4, 0.5], [G4, 0.5], [F4, 0.5], [E4, 0.5], [D4, 0.5],
      [E4, 0.5], [F4, 0.5], [G4, 0.5], [E4, 0.5], [C5, 2],
      [B4, 0.5], [C5, 0.5], [B4, 0.5], [A4, 0.5], [G4, 1], [E4, 1],
      [D4, 1], [G4, 1], [C4, 2],
    ],
  },
  {
    id: 'lavenders-blue',
    title: { zh: '薰衣草蓝', en: "Lavender's Blue" },
    bpm: 100,
    sequence: [
      [C4, 1], [E4, 1], [E4, 1], [D4, 1],
      [C4, 1], [E4, 1], [G4, 2],
      [A4, 1], [G4, 1], [F4, 1], [E4, 1],
      [D4, 2], [D4, 2],
      [E4, 1], [F4, 1], [E4, 1], [D4, 1],
      [C4, 1], [E4, 1], [G4, 2],
      [A4, 1], [G4, 1], [F4, 1], [D4, 1],
      [C4, 2], [0, 2],
    ],
  },
];

export const PUBLIC_DOMAIN_ABC_SONGS: TutorialSong[] = SONG_DEFINITIONS.map(buildTutorialSongFromSequence);
