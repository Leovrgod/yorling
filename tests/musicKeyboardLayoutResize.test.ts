import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('uses the volume row as a draggable separator between the instrument and keyboard panes', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(musicSource.includes('music-layout-shell'));
  assert.ok(musicSource.includes('music-layout-top'));
  assert.ok(musicSource.includes('music-layout-bottom'));
  assert.ok(musicSource.includes('music-pane-divider'));
  assert.ok(musicSource.includes('role="separator"'));
  assert.ok(musicSource.includes('aria-orientation="horizontal"'));
  assert.ok(musicSource.includes("window.addEventListener('pointermove', handleDividerPointerMove);"));
  assert.ok(musicSource.includes("window.addEventListener('pointerup', stopDividerDrag);"));
  assert.ok(musicSource.includes("window.addEventListener('pointercancel', stopDividerDrag);"));
  assert.ok(musicSource.includes('requestAnimationFrame'));
  assert.ok(musicSource.includes("document.body.classList.add('music-layout-resizing');"));
  assert.ok(musicSource.includes("document.body.classList.remove('music-layout-resizing');"));
  assert.ok(stylesSource.includes('.music-pane-divider'));
  assert.ok(stylesSource.includes('cursor: row-resize;'));
  assert.ok(stylesSource.includes('body.music-layout-resizing'));
});

test('keeps the volume slider interactive while the same row also acts as the resize affordance', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');

  assert.ok(musicSource.includes('data-music-resize-exempt'));
  assert.ok(musicSource.includes("closest('[data-music-resize-exempt]')"));
});
