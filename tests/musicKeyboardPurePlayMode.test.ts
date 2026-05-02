import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import * as musicMappingModule from '../src/data/musicKeyMapping.ts';

type MusicEventResolution = {
  shouldSwallow: boolean;
  midi: number | null;
  controlAction: string | null;
};

type PianoLayoutState = {
  mainOctave: number;
  lowerOctave: number;
  numberOctave: number;
};

type KeyboardInterceptorSnapshot = {
  running: boolean;
  enabled: boolean;
};

const musicMapping = musicMappingModule as Record<string, unknown>;
const resolveMusicKeyboardEvent = musicMapping.resolveMusicKeyboardEvent as
  | ((code: string, purePlayActive: boolean, layoutId?: string, pianoLayoutState?: number | PianoLayoutState) => MusicEventResolution)
  | undefined;
const shouldPauseNativeKeyboardInterceptor = musicMapping.shouldPauseNativeKeyboardInterceptor as
  | ((purePlayActive: boolean, status: KeyboardInterceptorSnapshot) => boolean)
  | undefined;
const shouldEnableMusicNativeKeySuppression = musicMapping.shouldEnableMusicNativeKeySuppression as
  | ((layoutId: string, purePlayActive: boolean, audioUnlocked?: boolean) => boolean)
  | undefined;
const DEFAULT_PIANO_LAYOUT_STATE: PianoLayoutState = {
  mainOctave: 4,
  lowerOctave: 3,
  numberOctave: 5,
};

test('swallows every key while the music module pure-play mode is active', () => {
  assert.strictEqual(typeof resolveMusicKeyboardEvent, 'function');

  assert.deepEqual(resolveMusicKeyboardEvent?.('KeyA', true), {
    shouldSwallow: true,
    midi: 53,
    controlAction: null,
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Escape', true), {
    shouldSwallow: true,
    midi: null,
    controlAction: null,
  });
});

test('lets keyboard events pass through when music pure-play mode is inactive', () => {
  assert.strictEqual(typeof resolveMusicKeyboardEvent, 'function');

  assert.deepEqual(resolveMusicKeyboardEvent?.('KeyA', false), {
    shouldSwallow: false,
    midi: null,
    controlAction: null,
  });
});

test('uses octave controls in the alternate piano layout while keeping H inert', () => {
  assert.strictEqual(typeof resolveMusicKeyboardEvent, 'function');

  assert.deepEqual(resolveMusicKeyboardEvent?.('CapsLock', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'mainOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Enter', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'mainOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('ShiftLeft', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'lowerOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('ShiftRight', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'lowerOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Tab', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'numberOctaveDown',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('Backslash', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: 'numberOctaveUp',
  });
  assert.deepEqual(resolveMusicKeyboardEvent?.('KeyH', true, 'piano', DEFAULT_PIANO_LAYOUT_STATE), {
    shouldSwallow: true,
    midi: null,
    controlAction: null,
  });
});

test('enables native music-key suppression throughout music pure-play mode while frontmost', () => {
  assert.strictEqual(typeof shouldEnableMusicNativeKeySuppression, 'function');

  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('piano', true, true), true);
  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('classic', true, true), true);
  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('piano', true, false), false);
  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('classic', true, false), false);
  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('piano', false, true), false);
  assert.strictEqual(shouldEnableMusicNativeKeySuppression?.('classic', false, true), false);
});

test('pauses the native interceptor only while pure-play mode would conflict with it', () => {
  assert.strictEqual(typeof shouldPauseNativeKeyboardInterceptor, 'function');

  assert.strictEqual(
    shouldPauseNativeKeyboardInterceptor?.(true, { running: true, enabled: true }),
    true,
  );
  assert.strictEqual(
    shouldPauseNativeKeyboardInterceptor?.(true, { running: true, enabled: false }),
    false,
  );
  assert.strictEqual(
    shouldPauseNativeKeyboardInterceptor?.(false, { running: true, enabled: true }),
    false,
  );
});

test('wires the music page to swallow key events in the capture phase', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');

  assert.ok(/window\.addEventListener\('keydown', handleKeyDown, \{ capture: true \}\);/.test(musicSource));
  assert.ok(/window\.addEventListener\('keyup', handleKeyUp, \{ capture: true \}\);/.test(musicSource));
  assert.strictEqual(musicSource.includes('e.stopPropagation();'), true);
});

test('primes web audio from trusted DOM input before relying on native music-key events', () => {
  const musicSource = readFileSync('src/components/music/MusicKeyboard.tsx', 'utf8');
  const audioSource = readFileSync('src/hooks/useMusicAudio.ts', 'utf8');

  assert.ok(/const \{[^}]*unlock[^}]*unlocked[^}]*\} = useMusicAudio\(/s.test(musicSource));
  assert.ok(musicSource.includes('const handleTrustedAudioUnlock = useCallback(() => {'));
  assert.ok(musicSource.includes('void unlock();'));
  assert.ok(musicSource.includes('onPointerDownCapture={handleTrustedAudioUnlock}'));
  assert.ok(audioSource.includes('unlock: () => Promise<void>;'));
  assert.ok(audioSource.includes('unlocked: boolean;'));
});
