import type {
  TutorialNote,
  TutorialSong,
  TutorialSongDifficultyId,
} from './tutorialSongTypes.js';

interface DifficultyMetricsInput {
  notes: readonly TutorialNote[];
  duration: number;
  bpm: number;
}

interface DifficultyProfile {
  score: number;
  density: number;
  chordPeak: number;
  pitchSpan: number;
  hasBlackKeys: boolean;
  hasOutsideComfortRange: boolean;
}

type SongWithOptionalDifficulty = Omit<TutorialSong, 'difficultyId'> & {
  difficultyId?: TutorialSongDifficultyId;
};

const BLACK_KEY_PITCH_CLASSES = new Set([1, 3, 6, 8, 10]);
const COMFORT_RANGE_MIN_MIDI = 60; // C4
const COMFORT_RANGE_MAX_MIDI = 71; // B4

function isBlackKey(midi: number): boolean {
  const pitchClass = ((midi % 12) + 12) % 12;
  return BLACK_KEY_PITCH_CLASSES.has(pitchClass);
}

function computeDifficultyProfile({ notes, duration, bpm }: DifficultyMetricsInput): DifficultyProfile {
  if (notes.length === 0) {
    return {
      score: 0,
      density: 0,
      chordPeak: 1,
      pitchSpan: 0,
      hasBlackKeys: false,
      hasOutsideComfortRange: false,
    };
  }

  let minMidi = Number.POSITIVE_INFINITY;
  let maxMidi = Number.NEGATIVE_INFINITY;
  let shortNotes = 0;
  let sustainedNotes = 0;
  let blackKeyCount = 0;
  let outsideComfortCount = 0;
  const uniqueMidis = new Set<number>();
  const simultaneousStarts = new Map<string, number>();

  for (const note of notes) {
    minMidi = Math.min(minMidi, note.midi);
    maxMidi = Math.max(maxMidi, note.midi);
    uniqueMidis.add(note.midi);

    if (note.duration <= 0.2) {
      shortNotes += 1;
    }

    if (note.duration >= 0.6) {
      sustainedNotes += 1;
    }

    if (isBlackKey(note.midi)) {
      blackKeyCount += 1;
    }

    if (note.midi < COMFORT_RANGE_MIN_MIDI || note.midi > COMFORT_RANGE_MAX_MIDI) {
      outsideComfortCount += 1;
    }

    const timeBucket = note.time.toFixed(3);
    simultaneousStarts.set(timeBucket, (simultaneousStarts.get(timeBucket) ?? 0) + 1);
  }

  const chordPeak = Math.max(1, ...simultaneousStarts.values());
  const density = notes.length / Math.max(duration, 1);
  const pitchSpan = maxMidi - minMidi;
  const shortNoteRatio = shortNotes / Math.max(notes.length, 1);
  const sustainRatio = sustainedNotes / Math.max(notes.length, 1);
  const blackKeyRatio = blackKeyCount / Math.max(notes.length, 1);
  const outsideComfortRatio = outsideComfortCount / Math.max(notes.length, 1);

  return {
    score:
      density * 12
      + chordPeak * 8
      + pitchSpan * 0.34
      + uniqueMidis.size * 0.5
      + shortNoteRatio * 18
      + sustainRatio * 5
      + Math.log2(notes.length + 1) * 4
      + Math.max(0, bpm - 88) * 0.06
      + blackKeyRatio * 14
      + outsideComfortRatio * 18
      + (blackKeyCount > 0 ? 6 : 0)
      + (outsideComfortCount > 0 ? 7 : 0)
      + (blackKeyCount > 0 && outsideComfortCount > 0 ? 6 : 0),
    density,
    chordPeak,
    pitchSpan,
    hasBlackKeys: blackKeyCount > 0,
    hasOutsideComfortRange: outsideComfortCount > 0,
  };
}

function isPrimerFriendlySong(
  profile: DifficultyProfile,
  { bpm }: DifficultyMetricsInput,
): boolean {
  return !profile.hasBlackKeys
    && !profile.hasOutsideComfortRange
    && profile.chordPeak === 1
    && profile.pitchSpan <= 9
    && profile.density <= 2.2
    && bpm <= 120;
}

function scoreToDifficultyId(score: number): TutorialSongDifficultyId {
  if (score < 46) {
    return 'stage-01';
  }

  if (score < 52) {
    return 'stage-02';
  }

  if (score < 60) {
    return 'stage-03';
  }

  if (score < 70) {
    return 'stage-04';
  }

  if (score < 80) {
    return 'stage-05';
  }

  if (score < 90) {
    return 'stage-06';
  }

  if (score < 100) {
    return 'stage-07';
  }

  if (score < 110) {
    return 'stage-08';
  }

  if (score < 122) {
    return 'stage-09';
  }

  return 'stage-10';
}

export function estimateTutorialSongDifficultyId(
  input: DifficultyMetricsInput,
): TutorialSongDifficultyId {
  const profile = computeDifficultyProfile(input);
  if (isPrimerFriendlySong(profile, input)) {
    return 'stage-01';
  }

  if (profile.chordPeak >= 4 || (profile.density >= 8 && profile.pitchSpan >= 18)) {
    return 'stage-10';
  }

  return scoreToDifficultyId(profile.score);
}

export function withTutorialSongDifficulty<T extends SongWithOptionalDifficulty>(
  song: T,
): T & { difficultyId: TutorialSongDifficultyId } {
  return {
    ...song,
    difficultyId: song.difficultyId ?? estimateTutorialSongDifficultyId(song),
  };
}
