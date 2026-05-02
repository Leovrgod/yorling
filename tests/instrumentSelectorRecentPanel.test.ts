import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('renders the recent instruments panel as grouped categories without recency copy or per-item category text', () => {
  const selectorSource = readFileSync('src/components/music/InstrumentSelector.tsx', 'utf8');

  assert.ok(selectorSource.includes('instrument-selector-body'));
  assert.ok(selectorSource.includes('instrument-recent-panel'));
  assert.ok(selectorSource.includes('groupedRecentCategories'));
  assert.ok(selectorSource.includes('instrument-recent-group'));
  assert.ok(selectorSource.includes('instrument-recent-grid'));
  assert.ok(selectorSource.includes('instrument-recent-button'));
  assert.ok(selectorSource.includes('instrument-recent-remove'));
  assert.strictEqual(selectorSource.includes('按最近选择排序'), false);
  assert.strictEqual(selectorSource.includes('instrument-recent-meta'), false);
});

test('renders an empty-state message when there are no recent instruments yet', () => {
  const source = readFileSync('src/components/music/InstrumentSelector.tsx', 'utf8');

  assert.ok(source.includes('还没有最近使用的音色'));
});

test('keeps the recent instruments panel internally scrollable while hiding the scrollbar', () => {
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(stylesSource.includes('.instrument-recent-scroll'));
  assert.ok(stylesSource.includes('overflow-y: auto;'));
  assert.ok(stylesSource.includes('scrollbar-width: none;'));
  assert.ok(stylesSource.includes('.instrument-recent-scroll::-webkit-scrollbar'));
  assert.ok(stylesSource.includes('display: none;'));
});

test('widens the recent instruments panel and lays recent items out in two columns', () => {
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(stylesSource.includes('.instrument-recent-panel'));
  assert.ok(stylesSource.includes('width: min(440px, 42vw);'));
  assert.ok(stylesSource.includes('.instrument-recent-grid'));
  assert.ok(stylesSource.includes('display: grid;'));
  assert.ok(stylesSource.includes('grid-template-columns: repeat(2, minmax(0, 1fr));'));
});
