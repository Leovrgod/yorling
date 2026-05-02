import { useCallback, useEffect, useRef } from 'react';
import { midiToFrequency } from '../data/musicKeyMapping.ts';

/**
 * Piano-oriented Web Audio engine.
 *
 * This is still a modeled synth, not a sampled concert piano. The goal is:
 * - better attack/body than a bare triangle oscillator
 * - honest metadata about what the engine is
 * - a clean path to swap in sample playback later
 */

interface Voice {
  oscillators: OscillatorNode[];
  mix: GainNode;
  filter: BiquadFilterNode;
  gain: GainNode;
  refCount: number;
}

interface PianoEngineProfile {
  kind: 'modeled-synth';
  isRealPiano: false;
  supportsSamplePlayback: true;
  summary: string;
}

export const PIANO_ENGINE_PROFILE: PianoEngineProfile = {
  kind: 'modeled-synth',
  isRealPiano: false,
  supportsSamplePlayback: true,
  summary: 'Modeled piano-like synth voice with harmonic wave shaping and tone filtering.',
};

const ATTACK = 0.008;
const DECAY = 0.22;
const SUSTAIN = 0.38;
const RELEASE = 0.42;
const MASTER_GAIN = 0.24;

function createPianoWave(ctx: AudioContext): PeriodicWave {
  const harmonics = [1, 0.67, 0.42, 0.26, 0.17, 0.1, 0.07, 0.045];
  const real = new Float32Array(harmonics.length + 1);
  const imag = new Float32Array(harmonics.length + 1);

  harmonics.forEach((harmonic, index) => {
    imag[index + 1] = harmonic / (index + 1);
  });

  return ctx.createPeriodicWave(real, imag);
}

function disconnectVoice(voice: Voice) {
  for (const oscillator of voice.oscillators) {
    oscillator.disconnect();
  }
  voice.mix.disconnect();
  voice.filter.disconnect();
  voice.gain.disconnect();
}

export function usePianoAudio() {
  const ctxRef = useRef<AudioContext | null>(null);
  const voicesRef = useRef<Map<number, Voice>>(new Map());
  const masterGainRef = useRef<GainNode | null>(null);
  const pianoWaveRef = useRef<PeriodicWave | null>(null);

  const ensureContext = useCallback(() => {
    if (!ctxRef.current) {
      const ctx = new AudioContext();
      const master = ctx.createGain();
      const compressor = ctx.createDynamicsCompressor();

      master.gain.value = MASTER_GAIN;
      compressor.threshold.value = -26;
      compressor.knee.value = 20;
      compressor.ratio.value = 2.5;
      compressor.attack.value = 0.004;
      compressor.release.value = 0.18;

      master.connect(compressor);
      compressor.connect(ctx.destination);

      ctxRef.current = ctx;
      masterGainRef.current = master;
      pianoWaveRef.current = createPianoWave(ctx);
    }

    if (ctxRef.current.state === 'suspended') {
      void ctxRef.current.resume();
    }

    return ctxRef.current;
  }, []);

  const noteOn = useCallback((midi: number) => {
    const ctx = ensureContext();
    const master = masterGainRef.current;
    const pianoWave = pianoWaveRef.current;

    if (!master || !pianoWave) {
      return;
    }

    const voices = voicesRef.current;
    const existing = voices.get(midi);

    if (existing) {
      existing.refCount++;
      return;
    }

    const frequency = midiToFrequency(midi);
    const now = ctx.currentTime;

    const primary = ctx.createOscillator();
    primary.setPeriodicWave(pianoWave);
    primary.frequency.setValueAtTime(frequency, now);

    const detuned = ctx.createOscillator();
    detuned.setPeriodicWave(pianoWave);
    detuned.frequency.setValueAtTime(frequency, now);
    detuned.detune.setValueAtTime(3.5, now);

    const mix = ctx.createGain();
    mix.gain.value = 0.7;

    const filter = ctx.createBiquadFilter();
    filter.type = 'lowpass';
    filter.Q.value = 0.85;
    filter.frequency.setValueAtTime(Math.max(2200, frequency * 7), now);
    filter.frequency.exponentialRampToValueAtTime(Math.max(700, frequency * 2.2), now + 0.18);

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

    voices.set(midi, {
      oscillators: [primary, detuned],
      mix,
      filter,
      gain,
      refCount: 1,
    });
  }, [ensureContext]);

  const noteOff = useCallback((midi: number) => {
    const ctx = ctxRef.current;
    if (!ctx) {
      return;
    }

    const voices = voicesRef.current;
    const voice = voices.get(midi);
    if (!voice) {
      return;
    }

    voice.refCount--;
    if (voice.refCount > 0) {
      return;
    }

    const now = ctx.currentTime;
    voice.gain.gain.cancelScheduledValues(now);
    voice.gain.gain.setValueAtTime(Math.max(voice.gain.gain.value, 0.0001), now);
    voice.gain.gain.exponentialRampToValueAtTime(0.0001, now + RELEASE);
    voice.filter.frequency.cancelScheduledValues(now);
    voice.filter.frequency.setValueAtTime(Math.max(voice.filter.frequency.value, 300), now);
    voice.filter.frequency.exponentialRampToValueAtTime(280, now + RELEASE);

    const stopAt = now + RELEASE + 0.05;
    let remainingOscillators = voice.oscillators.length;

    for (const oscillator of voice.oscillators) {
      oscillator.onended = () => {
        remainingOscillators -= 1;
        if (remainingOscillators === 0) {
          disconnectVoice(voice);
        }
      };
      oscillator.stop(stopAt);
    }

    voices.delete(midi);
  }, []);

  const stopAll = useCallback(() => {
    const ctx = ctxRef.current;
    if (!ctx) {
      return;
    }

    const voices = voicesRef.current;
    const now = ctx.currentTime;

    for (const [midi, voice] of voices) {
      voice.gain.gain.cancelScheduledValues(now);
      voice.gain.gain.setValueAtTime(Math.max(voice.gain.gain.value, 0.0001), now);
      voice.gain.gain.exponentialRampToValueAtTime(0.0001, now + 0.06);

      const stopAt = now + 0.08;
      let remainingOscillators = voice.oscillators.length;

      for (const oscillator of voice.oscillators) {
        oscillator.onended = () => {
          remainingOscillators -= 1;
          if (remainingOscillators === 0) {
            disconnectVoice(voice);
          }
        };
        oscillator.stop(stopAt);
      }

      voices.delete(midi);
    }
  }, []);

  useEffect(() => {
    return () => {
      stopAll();
      voicesRef.current.clear();
      if (ctxRef.current) {
        void ctxRef.current.close();
      }
    };
  }, [stopAll]);

  return { noteOn, noteOff, stopAll };
}
