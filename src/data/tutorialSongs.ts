import { PUBLIC_DOMAIN_ABC_SONGS } from './abcSongLibrary.js';
import { GENERATED_PUBLIC_DOMAIN_SONGS } from './generatedPublicDomainSongs.js';
import { withTutorialSongDifficulty } from './tutorialSongDifficulty.js';
import type {
  GeneratedTutorialSong,
  TutorialSong,
  TutorialSongCategoryDefinition,
  TutorialSongCategoryId,
  TutorialSongDifficultyDefinition,
  TutorialSongDifficultyId,
} from './tutorialSongTypes.js';

export type {
  GeneratedTutorialSong,
  TutorialNote,
  TutorialSong,
  TutorialSongCategoryDefinition,
  TutorialSongCategoryId,
  TutorialSongDifficultyDefinition,
  TutorialSongDifficultyId,
  TutorialSongSource,
} from './tutorialSongTypes.js';

export const TUTORIAL_SONG_CATEGORIES: TutorialSongCategoryDefinition[] = [
  { id: 'featured-favorites', label: { zh: '精选公版', en: 'Featured public-domain' } },
  { id: 'reel', label: { zh: 'Reel', en: 'Reels' } },
  { id: 'hornpipe', label: { zh: 'Hornpipe', en: 'Hornpipes' } },
  { id: 'jig', label: { zh: 'Jig', en: 'Jigs' } },
  { id: 'strathspey', label: { zh: 'Strathspey', en: 'Strathspeys' } },
  { id: 'clog', label: { zh: 'Clog', en: 'Clogs' } },
  { id: 'highland-fling', label: { zh: 'Highland Fling', en: 'Highland Fling' } },
  { id: 'slip-jig', label: { zh: 'Slip Jig', en: 'Slip Jigs' } },
  { id: 'other-dance', label: { zh: '其他舞曲', en: 'Other dance tunes' } },
];

export const TUTORIAL_SONG_DIFFICULTIES: TutorialSongDifficultyDefinition[] = [
  { id: 'stage-01', label: { zh: '第01阶', en: 'Stage 01' } },
  { id: 'stage-02', label: { zh: '第02阶', en: 'Stage 02' } },
  { id: 'stage-03', label: { zh: '第03阶', en: 'Stage 03' } },
  { id: 'stage-04', label: { zh: '第04阶', en: 'Stage 04' } },
  { id: 'stage-05', label: { zh: '第05阶', en: 'Stage 05' } },
  { id: 'stage-06', label: { zh: '第06阶', en: 'Stage 06' } },
  { id: 'stage-07', label: { zh: '第07阶', en: 'Stage 07' } },
  { id: 'stage-08', label: { zh: '第08阶', en: 'Stage 08' } },
  { id: 'stage-09', label: { zh: '第09阶', en: 'Stage 09' } },
  { id: 'stage-10', label: { zh: '第10阶', en: 'Stage 10' } },
];

const TUTORIAL_SONG_CATEGORY_BY_ID = new Map(
  TUTORIAL_SONG_CATEGORIES.map((category, index) => [category.id, { category, index }] as const),
);

const TUTORIAL_SONG_DIFFICULTY_BY_ID = new Map(
  TUTORIAL_SONG_DIFFICULTIES.map((difficulty, index) => [difficulty.id, { difficulty, index }] as const),
);

function decodeGeneratedSong(song: GeneratedTutorialSong): TutorialSong {
  return withTutorialSongDifficulty({
    ...song,
    notes: song.notes.map(([midi, time, duration]) => ({
      midi,
      time,
      duration,
    })),
  });
}

export const TUTORIAL_SONGS: TutorialSong[] = [
  ...PUBLIC_DOMAIN_ABC_SONGS.map(withTutorialSongDifficulty),
  ...GENERATED_PUBLIC_DOMAIN_SONGS.map(decodeGeneratedSong),
];

const TUTORIAL_SONGS_BY_ID = new Map(TUTORIAL_SONGS.map((song) => [song.id, song] as const));

export function getTutorialSong(id: string): TutorialSong | undefined {
  return TUTORIAL_SONGS_BY_ID.get(id);
}

export function getTutorialSongCategory(
  categoryId: TutorialSongCategoryId,
): TutorialSongCategoryDefinition {
  const entry = TUTORIAL_SONG_CATEGORY_BY_ID.get(categoryId);
  if (!entry) {
    throw new Error(`Unknown tutorial song category: ${categoryId}`);
  }
  return entry.category;
}

export function getTutorialSongDifficulty(
  difficultyId: TutorialSongDifficultyId,
): TutorialSongDifficultyDefinition {
  const entry = TUTORIAL_SONG_DIFFICULTY_BY_ID.get(difficultyId);
  if (!entry) {
    throw new Error(`Unknown tutorial song difficulty: ${difficultyId}`);
  }
  return entry.difficulty;
}

export function groupTutorialSongsByCategory<T extends { categoryId: TutorialSongCategoryId }>(
  songs: readonly T[],
): Array<{ category: TutorialSongCategoryDefinition; songs: T[] }> {
  const groupedSongs = new Map<TutorialSongCategoryId, T[]>();

  for (const song of songs) {
    if (!TUTORIAL_SONG_CATEGORY_BY_ID.has(song.categoryId)) {
      throw new Error(`Unknown tutorial song category: ${song.categoryId}`);
    }

    const currentSongs = groupedSongs.get(song.categoryId);
    if (currentSongs) {
      currentSongs.push(song);
      continue;
    }

    groupedSongs.set(song.categoryId, [song]);
  }

  return [...groupedSongs.entries()]
    .sort(([leftId], [rightId]) => {
      const leftIndex = TUTORIAL_SONG_CATEGORY_BY_ID.get(leftId)?.index ?? Number.MAX_SAFE_INTEGER;
      const rightIndex = TUTORIAL_SONG_CATEGORY_BY_ID.get(rightId)?.index ?? Number.MAX_SAFE_INTEGER;
      return leftIndex - rightIndex;
    })
    .map(([categoryId, categorySongs]) => ({
      category: getTutorialSongCategory(categoryId),
      songs: categorySongs,
    }));
}

export function groupTutorialSongsByDifficulty<T extends { difficultyId: TutorialSongDifficultyId }>(
  songs: readonly T[],
): Array<{ difficulty: TutorialSongDifficultyDefinition; songs: T[] }> {
  const groupedSongs = new Map<TutorialSongDifficultyId, T[]>();

  for (const song of songs) {
    if (!TUTORIAL_SONG_DIFFICULTY_BY_ID.has(song.difficultyId)) {
      throw new Error(`Unknown tutorial song difficulty: ${song.difficultyId}`);
    }

    const currentSongs = groupedSongs.get(song.difficultyId);
    if (currentSongs) {
      currentSongs.push(song);
      continue;
    }

    groupedSongs.set(song.difficultyId, [song]);
  }

  return [...groupedSongs.entries()]
    .sort(([leftId], [rightId]) => {
      const leftIndex = TUTORIAL_SONG_DIFFICULTY_BY_ID.get(leftId)?.index ?? Number.MAX_SAFE_INTEGER;
      const rightIndex = TUTORIAL_SONG_DIFFICULTY_BY_ID.get(rightId)?.index ?? Number.MAX_SAFE_INTEGER;
      return leftIndex - rightIndex;
    })
    .map(([difficultyId, difficultySongs]) => ({
      difficulty: getTutorialSongDifficulty(difficultyId),
      songs: difficultySongs,
    }));
}
