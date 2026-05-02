export type TutorialSongSource = 'builtin-abc';
export type TutorialSongCategoryId =
  | 'featured-favorites'
  | 'reel'
  | 'hornpipe'
  | 'jig'
  | 'strathspey'
  | 'clog'
  | 'highland-fling'
  | 'slip-jig'
  | 'other-dance';

export interface TutorialSongCategoryDefinition {
  id: TutorialSongCategoryId;
  label: { zh: string; en: string };
}

export type TutorialSongDifficultyId =
  | 'stage-01'
  | 'stage-02'
  | 'stage-03'
  | 'stage-04'
  | 'stage-05'
  | 'stage-06'
  | 'stage-07'
  | 'stage-08'
  | 'stage-09'
  | 'stage-10';

export interface TutorialSongDifficultyDefinition {
  id: TutorialSongDifficultyId;
  label: { zh: string; en: string };
}

export interface TutorialNote {
  midi: number;
  time: number;
  duration: number;
}

export type TutorialNoteTuple = [number, number, number];

export interface GeneratedTutorialSong {
  id: string;
  title: { zh: string; en: string };
  bpm: number;
  notes: TutorialNoteTuple[];
  duration: number;
  source: Extract<TutorialSongSource, 'builtin-abc'>;
  categoryId: TutorialSongCategoryId;
  difficultyId?: TutorialSongDifficultyId;
}

export interface TutorialSong {
  id: string;
  title: { zh: string; en: string };
  bpm: number;
  notes: TutorialNote[];
  duration: number;
  source: TutorialSongSource;
  categoryId: TutorialSongCategoryId;
  difficultyId: TutorialSongDifficultyId;
  audioUrl?: string;
  audioFileName?: string;
}
