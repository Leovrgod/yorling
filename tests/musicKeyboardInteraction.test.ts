import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('wires pointer handlers so on-screen music keys can be played directly', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');

  assert.ok(/onPointerDown=\{/.test(musicSource));
  assert.ok(/onPointerUp=\{/.test(musicSource));
  assert.ok(/onPointerCancel=\{/.test(musicSource));
  assert.ok(/window\.addEventListener\('pointerup', handleWindowPointerUp, \{ capture: true \}\);/.test(musicSource));
  assert.ok(/window\.addEventListener\('pointercancel', handleWindowPointerUp, \{ capture: true \}\);/.test(musicSource));
});

test('renders a selectable music layout switcher with multi-zone piano octave controls', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(musicSource.includes('music-layout-switcher'));
  assert.ok(musicSource.includes('setKeyboardLayout'));
  assert.ok(musicSource.includes('shiftPianoLayoutOctave'));
  assert.ok(musicSource.includes('shiftPianoLowerLayoutOctave'));
  assert.ok(musicSource.includes('shiftPianoNumberLayoutOctave'));
  assert.ok(musicSource.includes('pianoLowerLayoutOctave'));
  assert.ok(musicSource.includes('pianoNumberLayoutOctave'));
  assert.ok(musicSource.includes('music-octave-groups'));
  assert.ok(stylesSource.includes('.music-layout-switcher'));
  assert.ok(stylesSource.includes('.music-octave-groups'));
  assert.ok(stylesSource.includes('.music-octave-group'));
});

test('does not render a dedicated F-key row for the music keyboard', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const stylesSource = readFileSync('src/styles/nothing/music.css', 'utf8');

  assert.ok(!musicSource.includes('MUSIC_KEYBOARD_TOP_ROW_GROUPS'));
  assert.ok(!musicSource.includes("id: 'F1'"));
  assert.ok(!musicSource.includes("id: 'F12'"));
  assert.ok(!musicSource.includes('music-keyboard-top-row'));
  assert.ok(!musicSource.includes('music-keyboard-top-row-group'));
  assert.ok(!stylesSource.includes('.music-keyboard-top-row'));
  assert.ok(!stylesSource.includes('.music-keyboard-top-row-group'));
});

test('wires music mode to native key suppression so music keys do not fall through to the system', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const tauriHookSource = readFileSync('src/hooks/useTauriCommand.ts', 'utf8');
  const tauriCommandSource = readFileSync('src-tauri/src/commands/keyboard.rs', 'utf8');
  const tauriLibSource = readFileSync('src-tauri/src/lib.rs', 'utf8');
  const interceptorSource = readFileSync('crates/yorling-platform-macos/src/interceptor.rs', 'utf8');

  assert.ok(musicSource.includes('setMusicNativeKeySuppression'));
  assert.ok(tauriHookSource.includes('setMusicNativeKeySuppression'));
  assert.ok(tauriHookSource.includes("invoke('set_music_native_keys_suppressed'"));
  assert.ok(tauriCommandSource.includes('set_music_native_keys_suppressed'));
  assert.ok(tauriLibSource.includes('commands::keyboard::set_music_native_keys_suppressed'));
  assert.ok(interceptorSource.includes('K_CG_EVENT_SYSTEM_DEFINED'));
  assert.ok(interceptorSource.includes('K_CG_EVENT_DATA1'));
  assert.ok(interceptorSource.includes('decode_music_system_defined_data1'));
  assert.ok(!interceptorSource.includes('eventWithEventRef'));
});
