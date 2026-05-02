import { useCallback, useEffect, useRef, useState } from 'react';
import { Soundfont } from 'smplr';
import { midiToFrequency } from '../data/musicKeyMapping';

/**
 * Unified music audio engine.
 *
 * Supports two backends:
 *   1. Built-in modeled synth piano (no network, instant)
 *   2. SoundFont instruments via `smplr` (loads samples from CDN)
 *
 * Design decisions (per rubber-duck review):
 *   - One AudioContext for the lifetime of the component
 *   - Old instrument stays playable until new one finishes loading
 *   - All notes are force-stopped before the engine swap completes
 *   - Per-note stop handles from smplr preserve ref-count semantics
 *   - Error state exposed so UI can show fallback / retry
 */

// ── Built-in synth constants ────────────────────────────────────────

const ATTACK = 0.008;
const DECAY = 0.22;
const SUSTAIN = 0.38;
const RELEASE = 0.42;
const MASTER_GAIN = 0.24;
const SOUNDFONT_PROBE_MIDI = 60;

interface SynthVoice {
  oscillators: OscillatorNode[];
  mix: GainNode;
  filter: BiquadFilterNode;
  gain: GainNode;
  refCount: number;
}

function createPianoWave(ctx: AudioContext): PeriodicWave {
  const harmonics = [1, 0.67, 0.42, 0.26, 0.17, 0.1, 0.07, 0.045];
  const real = new Float32Array(harmonics.length + 1);
  const imag = new Float32Array(harmonics.length + 1);
  harmonics.forEach((h, i) => { imag[i + 1] = h / (i + 1); });
  return ctx.createPeriodicWave(real, imag);
}

function disconnectSynthVoice(voice: SynthVoice) {
  for (const osc of voice.oscillators) osc.disconnect();
  voice.mix.disconnect();
  voice.filter.disconnect();
  voice.gain.disconnect();
}

function synthMasterGainFromVolume(volume: number): number {
  const clamped = Math.max(0, Math.min(127, volume));
  return MASTER_GAIN * (clamped / 100);
}

function probeSoundfontPlayback(sf: Soundfont): boolean {
  let playable = false;
  const stopProbe = sf.start({
    note: SOUNDFONT_PROBE_MIDI,
    velocity: 1,
    duration: 0.01,
    onStart: () => {
      playable = true;
    },
  });
  stopProbe();
  return playable;
}

// ── Types ───────────────────────────────────────────────────────────

export interface MusicAudioHandle {
  noteOn: (midi: number) => void;
  noteOff: (midi: number) => void;
  stopAll: () => void;
  unlock: () => Promise<void>;
  loading: boolean;
  ready: boolean;
  error: string | null;
  unlocked: boolean;
}

// ── Hook ────────────────────────────────────────────────────────────

export function useMusicAudio(
  instrumentId: string,
  volume: number,
): MusicAudioHandle {
  const isBuiltIn = instrumentId === 'synth-piano';

  // Shared AudioContext — single instance for module lifetime
  const ctxRef = useRef<AudioContext | null>(null);

  // Built-in synth refs
  const synthVoicesRef = useRef<Map<number, SynthVoice>>(new Map());
  const synthMasterRef = useRef<GainNode | null>(null);
  const pianoWaveRef = useRef<PeriodicWave | null>(null);

  // SoundFont refs — "active" is what's currently playing,
  // "pending" is loading in background
  const activeSfRef = useRef<Soundfont | null>(null);
  const activeIdRef = useRef<string | null>(null);
  // Per-note stop handles & ref counts for soundfont
  const sfStopHandlesRef = useRef<Map<number, (() => void)[]>>(new Map());
  const sfRefCountRef = useRef<Map<number, number>>(new Map());
  // Sequence counter to discard stale loads
  const switchSeqRef = useRef(0);

  // Track volume via ref so the loading effect doesn't re-run on volume changes
  const volumeRef = useRef(volume);

  const [loading, setLoading] = useState(false);
  const [ready, setReady] = useState(isBuiltIn);
  const [error, setError] = useState<string | null>(null);
  const [unlocked, setUnlocked] = useState(false);

  const ensureContext = useCallback(() => {
    if (!ctxRef.current) {
      ctxRef.current = new AudioContext();
    }
    if (ctxRef.current.state === 'suspended') {
      void ctxRef.current.resume();
    }
    return ctxRef.current;
  }, []);

  const unlock = useCallback(async () => {
    const ctx = ensureContext();

    try {
      if (ctx.state === 'suspended') {
        await ctx.resume();
      }

      if (ctx.state !== 'closed') {
        setUnlocked(true);
      }
    } catch (error) {
      console.warn('Music: failed to unlock audio context from a trusted DOM gesture', error);
    }
  }, [ensureContext]);

  // ── Built-in synth setup ────────────────────────────────────────

  const ensureSynthNodes = useCallback(() => {
    const ctx = ensureContext();
    if (!synthMasterRef.current) {
      const master = ctx.createGain();
      const compressor = ctx.createDynamicsCompressor();
      master.gain.value = synthMasterGainFromVolume(volumeRef.current);
      compressor.threshold.value = -26;
      compressor.knee.value = 20;
      compressor.ratio.value = 2.5;
      compressor.attack.value = 0.004;
      compressor.release.value = 0.18;
      master.connect(compressor);
      compressor.connect(ctx.destination);
      synthMasterRef.current = master;
      pianoWaveRef.current = createPianoWave(ctx);
    }
  }, [ensureContext]);

  // ── Stop all notes on the SoundFont engine ──────────────────────

  const stopSfNotes = useCallback(() => {
    if (activeSfRef.current) {
      activeSfRef.current.stop();
    }
    sfStopHandlesRef.current.clear();
    sfRefCountRef.current.clear();
  }, []);

  const stopSynthNotes = useCallback(() => {
    const ctx = ctxRef.current;
    if (!ctx) return;
    const voices = synthVoicesRef.current;
    const now = ctx.currentTime;
    for (const [, voice] of voices) {
      voice.gain.gain.cancelScheduledValues(now);
      voice.gain.gain.setValueAtTime(Math.max(voice.gain.gain.value, 0.0001), now);
      voice.gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.06);
      const stopAt = now + 0.08;
      let remaining = voice.oscillators.length;
      for (const osc of voice.oscillators) {
        osc.onended = () => {
          remaining--;
          if (remaining === 0) disconnectSynthVoice(voice);
        };
        osc.stop(stopAt);
      }
    }
    voices.clear();
  }, []);

  // ── SoundFont loading ───────────────────────────────────────────

  useEffect(() => {
    const seq = ++switchSeqRef.current;

    if (isBuiltIn) {
      // Force-stop any playing soundfont notes, then clear
      stopSfNotes();
      activeSfRef.current = null;
      activeIdRef.current = 'synth-piano';
      setLoading(false);
      setReady(true);
      setError(null);
      return;
    }

    if (activeIdRef.current === instrumentId && activeSfRef.current) {
      return; // Already loaded
    }

    setLoading(true);
    setError(null);
    // Keep ready=true if we have an old engine so input still works

    const ctx = ensureContext();
    const sf = new Soundfont(ctx, {
      instrument: instrumentId as any,
      volume: 0,
    });

    sf.load.then(() => {
      if (switchSeqRef.current !== seq) {
        sf.stop();
        sf.disconnect();
        return;
      }

      if (!probeSoundfontPlayback(sf)) {
        sf.stop();
        sf.disconnect();
        throw new Error(
          `SoundFont loaded for "${instrumentId}" but no playable samples decoded`,
        );
      }

      // Force-stop notes on old engine before swap
      stopSfNotes();
      stopSynthNotes();

      activeSfRef.current = sf;
      activeIdRef.current = instrumentId;
      sf.output.setVolume(volumeRef.current);

      setLoading(false);
      setReady(true);
      setError(null);
    }).catch((err: unknown) => {
      if (switchSeqRef.current !== seq) return;

      const message = err instanceof Error
        ? err.message
        : 'Failed to load instrument';
      console.warn(`Music: failed to activate SoundFont "${instrumentId}"`, err);
      setLoading(false);
      setError(message);
      // Don't clear ready — old engine may still be usable
    });
  }, [instrumentId, isBuiltIn, ensureContext, stopSfNotes, stopSynthNotes]);

  // ── Volume sync ─────────────────────────────────────────────────

  useEffect(() => {
    volumeRef.current = volume;
    if (synthMasterRef.current) {
      synthMasterRef.current.gain.value = synthMasterGainFromVolume(volume);
    }
    if (activeSfRef.current) {
      activeSfRef.current.output.setVolume(volume);
    }
  }, [volume]);

  // ── Determine which engine to use for note events ───────────────
  // If we're built-in, always use synth. Otherwise use soundfont if
  // the correct instrument is loaded; fall back to synth otherwise.

  const usesSynth = useCallback(() => {
    if (isBuiltIn) return true;
    return !activeSfRef.current || activeIdRef.current !== instrumentId;
  }, [isBuiltIn, instrumentId]);

  // ── Note on ─────────────────────────────────────────────────────

  const noteOn = useCallback((midi: number) => {
    // The music page primes Web Audio from trusted DOM gestures before native
    // music-key events take over; keep trying to resume in case the browser
    // suspended the context again.
    ensureContext();

    if (usesSynth()) {
      ensureSynthNodes();
      const ctx = ctxRef.current!;
      const master = synthMasterRef.current!;
      const pianoWave = pianoWaveRef.current!;
      const voices = synthVoicesRef.current;
      const existing = voices.get(midi);

      if (existing) {
        existing.refCount++;
        return;
      }

      const freq = midiToFrequency(midi);
      const now = ctx.currentTime;

      const primary = ctx.createOscillator();
      primary.setPeriodicWave(pianoWave);
      primary.frequency.setValueAtTime(freq, now);

      const detuned = ctx.createOscillator();
      detuned.setPeriodicWave(pianoWave);
      detuned.frequency.setValueAtTime(freq, now);
      detuned.detune.setValueAtTime(3.5, now);

      const mix = ctx.createGain();
      mix.gain.value = 0.7;

      const filter = ctx.createBiquadFilter();
      filter.type = 'lowpass';
      filter.Q.value = 0.85;
      filter.frequency.setValueAtTime(Math.max(2200, freq * 7), now);
      filter.frequency.exponentialRampToValueAtTime(Math.max(700, freq * 2.2), now + 0.18);

      const gain = ctx.createGain();
      gain.gain.setValueAtTime(0.0001, now);
      gain.gain.exponentialRampToValueAtTime(0.9, now + ATTACK);
      gain.gain.exponentialRampToValueAtTime(SUSTAIN, now + ATTACK + DECAY);

      primary.connect(mix);
      detuned.connect(mix);
      mix.connect(filter);
      filter.connect(gain);
      gain.connect(master);
      primary.start(now);
      detuned.start(now);

      voices.set(midi, { oscillators: [primary, detuned], mix, filter, gain, refCount: 1 });
    } else {
      const sf = activeSfRef.current;
      if (!sf) return;

      const rc = sfRefCountRef.current;
      const currentCount = rc.get(midi) ?? 0;
      if (currentCount > 0) {
        rc.set(midi, currentCount + 1);
        return;
      }

      rc.set(midi, 1);
      const vel = Math.max(30, Math.min(127, Math.round((volume / 127) * 100 + 27)));
      const stopFn = sf.start({ note: midi, velocity: vel });
      const handles = sfStopHandlesRef.current.get(midi) ?? [];
      handles.push(stopFn);
      sfStopHandlesRef.current.set(midi, handles);
    }
  }, [usesSynth, ensureContext, ensureSynthNodes, volume]);

  // ── Note off ────────────────────────────────────────────────────

  const noteOff = useCallback((midi: number) => {
    // Try both engines — a note may have started on one before a switch
    const voices = synthVoicesRef.current;
    const synthVoice = voices.get(midi);
    if (synthVoice) {
      synthVoice.refCount--;
      if (synthVoice.refCount <= 0) {
        const ctx = ctxRef.current;
        if (ctx) {
          const now = ctx.currentTime;
          synthVoice.gain.gain.cancelScheduledValues(now);
          synthVoice.gain.gain.setValueAtTime(Math.max(synthVoice.gain.gain.value, 0.0001), now);
          synthVoice.gain.gain.exponentialRampToValueAtTime(0.0001, now + RELEASE);
          synthVoice.filter.frequency.cancelScheduledValues(now);
          synthVoice.filter.frequency.setValueAtTime(Math.max(synthVoice.filter.frequency.value, 300), now);
          synthVoice.filter.frequency.exponentialRampToValueAtTime(280, now + RELEASE);
          const stopAt = now + RELEASE + 0.05;
          let remaining = synthVoice.oscillators.length;
          for (const osc of synthVoice.oscillators) {
            osc.onended = () => {
              remaining--;
              if (remaining === 0) disconnectSynthVoice(synthVoice);
            };
            osc.stop(stopAt);
          }
        }
        voices.delete(midi);
      }
    }

    // SoundFont
    const rc = sfRefCountRef.current;
    const currentCount = rc.get(midi) ?? 0;
    if (currentCount > 0) {
      const next = currentCount - 1;
      if (next > 0) {
        rc.set(midi, next);
      } else {
        rc.delete(midi);
        const handles = sfStopHandlesRef.current.get(midi);
        if (handles) {
          for (const stop of handles) stop();
          sfStopHandlesRef.current.delete(midi);
        }
      }
    }
  }, []);

  // ── Stop all ────────────────────────────────────────────────────

  const stopAll = useCallback(() => {
    stopSynthNotes();
    stopSfNotes();
  }, [stopSynthNotes, stopSfNotes]);

  // ── Cleanup on unmount ──────────────────────────────────────────

  useEffect(() => {
    return () => {
      stopAll();
      synthVoicesRef.current.clear();
      if (activeSfRef.current) {
        activeSfRef.current.stop();
        activeSfRef.current = null;
      }
      if (ctxRef.current) {
        void ctxRef.current.close();
        ctxRef.current = null;
      }
    };
  }, [stopAll]);

  return { noteOn, noteOff, stopAll, unlock, loading, ready, error, unlocked };
}
