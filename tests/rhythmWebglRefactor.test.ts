import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('rewires MusicKeyboard back onto the legacy waterfall tutorial stack', () => {
  const source = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const panelSource = readFileSync('src/components/music/TutorialPanel.tsx', 'utf8');
  const stageSource = readFileSync('src/components/music/WaterfallCanvas.tsx', 'utf8');

  assert.ok(source.includes("../../stores/tutorialStore"));
  assert.ok(source.includes("../../stores/scoringStore"));
  assert.ok(source.includes("./TutorialPanel"));
  assert.ok(source.includes('keyLabelByMidi={keyLabelByMidi}'));
  assert.ok(panelSource.includes("from '../../stores/tutorialStore'"));
  assert.ok(panelSource.includes("from '../../stores/scoringStore'"));
  assert.ok(panelSource.includes("from './WaterfallCanvas'"));
  assert.ok(stageSource.includes('className="waterfall-canvas"'));
});

test('implements the rebuilt stage with native WebGL2 rendering primitives', () => {
  const rendererSource = readFileSync('src/components/music/rhythm/engine/renderer.ts', 'utf8');
  const stageSource = readFileSync('src/components/music/rhythm/RhythmStage.tsx', 'utf8');

  assert.ok(rendererSource.includes("getContext('webgl2'"));
  assert.ok(rendererSource.includes('drawArraysInstanced'));
  assert.ok(rendererSource.includes('ParticlePool'));
  assert.ok(stageSource.includes('new RhythmRenderer(canvas)'));
  assert.ok(stageSource.includes('requestAnimationFrame(tick)'));
});

test('keeps the rhythm header compact so the waterfall has more room', () => {
  const panelSource = readFileSync('src/components/music/RhythmGamePanel.tsx', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(!panelSource.includes('WebGL 原生音游模式'));
  assert.ok(panelSource.includes('className="rhythm-hero-metrics"'));
  assert.ok(panelSource.includes('className="rhythm-inline-metric"'));
  assert.ok(!panelSource.includes('selectedDifficulty.label[language]'));
  assert.ok(!panelSource.includes('selectedCategory.label[language]'));
  assert.ok(stylesSource.includes('.rhythm-hero-metrics'));
  assert.ok(stylesSource.includes('.rhythm-inline-metric'));
  assert.ok(stylesSource.includes('.rhythm-upcoming {'));
});
