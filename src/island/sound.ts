import voiceUrl from '../assets/sounds/voice.wav';
import type { IslandEventPayload } from '../types';
import type { IslandSoundStyle } from './store/islandStore';

const MIN_SOUND_INTERVAL_MS = 320;

let sharedAudio: HTMLAudioElement | null = null;
let lastPlayedAt = 0;

function getSharedAudio(): HTMLAudioElement | null {
  if (typeof Audio === 'undefined') {
    return null;
  }

  if (!sharedAudio) {
    sharedAudio = new Audio(voiceUrl);
    sharedAudio.preload = 'auto';
  }

  return sharedAudio;
}

function clampVolume(volume: number): number {
  return Math.max(0, Math.min(1, volume));
}

export function preloadIslandSound(style: IslandSoundStyle) {
  if (style === 'silent') {
    return;
  }

  const audio = getSharedAudio();
  if (!audio || audio.readyState > 0) {
    return;
  }

  audio.load();
}

export function playIslandSound(style: IslandSoundStyle, volume: number) {
  if (style === 'silent') {
    return;
  }

  const now = Date.now();
  if (now - lastPlayedAt < MIN_SOUND_INTERVAL_MS) {
    return;
  }
  lastPlayedAt = now;

  const audio = getSharedAudio();
  if (!audio) {
    return;
  }

  audio.pause();
  audio.currentTime = 0;
  audio.volume = clampVolume(volume);
  void audio.play().catch(() => {});
}

export function shouldPlayIslandEventSound(
  eventType: IslandEventPayload['event_type'],
): boolean {
  switch (eventType.type) {
    case 'SessionStart':
    case 'SessionEnd':
    case 'UserPrompt':
    case 'PermissionRequest':
    case 'AskQuestion':
    case 'Stop':
    case 'SubagentStop':
    case 'Notification':
      return true;
    default:
      return false;
  }
}

export function playIslandEventSound(
  eventType: IslandEventPayload['event_type'],
  style: IslandSoundStyle,
  volume: number,
) {
  if (!shouldPlayIslandEventSound(eventType)) {
    return;
  }

  playIslandSound(style, volume);
}
