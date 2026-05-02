import test from 'node:test';
import assert from 'node:assert/strict';
import { TUTORIAL_SONGS } from '../src/data/tutorialSongs.ts';
import { LEAD_IN_SEC, useTutorialStore } from '../src/stores/tutorialStore.ts';

const firstSongId = TUTORIAL_SONGS[0]?.id ?? '';
const secondSongId = TUTORIAL_SONGS[1]?.id ?? firstSongId;

function resetTutorialStore() {
  useTutorialStore.setState({
    active: false,
    songId: firstSongId,
    recentSongIds: [],
    isPlaying: false,
    isDemoPlaying: false,
    speed: 1,
    externalCurrentTime: null,
    playStartTs: 0,
    elapsedAtPause: -LEAD_IN_SEC,
  });
}

test('selecting a song does not add it to recent history until the player practices it', () => {
  resetTutorialStore();

  useTutorialStore.getState().selectSong(secondSongId);
  assert.deepEqual(useTutorialStore.getState().recentSongIds, []);

  useTutorialStore.getState().markCurrentSongPracticed();
  assert.deepEqual(useTutorialStore.getState().recentSongIds, [secondSongId]);
});

test('markCurrentSongPracticed records each song once while keeping first-seen order', () => {
  resetTutorialStore();

  useTutorialStore.getState().selectSong(firstSongId);
  useTutorialStore.getState().markCurrentSongPracticed();
  useTutorialStore.getState().markCurrentSongPracticed();
  useTutorialStore.getState().selectSong(secondSongId);
  useTutorialStore.getState().markCurrentSongPracticed();
  useTutorialStore.getState().selectSong(firstSongId);
  useTutorialStore.getState().markCurrentSongPracticed();

  assert.deepEqual(useTutorialStore.getState().recentSongIds, [firstSongId, secondSongId]);
});
