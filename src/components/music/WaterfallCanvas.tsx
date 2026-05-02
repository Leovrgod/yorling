import { memo, useCallback, useEffect, useMemo, useRef } from 'react';
import type { TutorialSong } from '../../data/tutorialSongTypes';
import {
  getLatestActiveSuccessPulseIdsByMidi,
  useScoringStore,
  type HitFeedback,
} from '../../stores/scoringStore';

// ── Note color palette (matches music.css chromatic system) ─────────

const NOTE_HSL: Record<number, [number, number]> = {
  0:  [0, 72],    // C
  1:  [25, 80],   // C#
  2:  [35, 85],   // D
  3:  [50, 80],   // D#
  4:  [80, 60],   // E
  5:  [145, 55],  // F
  6:  [170, 55],  // F#
  7:  [190, 70],  // G
  8:  [220, 65],  // G#
  9:  [240, 55],  // A
  10: [265, 55],  // A#
  11: [280, 55],  // B
};

const BLACK_SEMITONES = new Set([1, 3, 6, 8, 10]);

function isBlackKey(midi: number): boolean {
  return BLACK_SEMITONES.has(((midi % 12) + 12) % 12);
}

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

function midiNoteName(midi: number): string {
  return NOTE_NAMES[((midi % 12) + 12) % 12] + String(Math.floor(midi / 12) - 1);
}

// ── Piano key position computation ──────────────────────────────────

interface KeyPos {
  midi: number;
  x: number;
  w: number;
  black: boolean;
}

interface PreparedNote {
  index: number;
  midi: number;
  time: number;
  duration: number;
  endTime: number;
  hue: number;
  sat: number;
}

interface CachedKeyLayout {
  width: number;
  minMidi: number;
  maxMidi: number;
  keys: KeyPos[];
  keyMap: Map<number, KeyPos>;
}

function computeKeyLayout(minMidi: number, maxMidi: number, width: number): KeyPos[] {
  // Expand to white-key boundaries
  while (isBlackKey(minMidi) && minMidi > 0) minMidi--;
  while (isBlackKey(maxMidi) && maxMidi < 127) maxMidi++;

  let whiteCount = 0;
  for (let m = minMidi; m <= maxMidi; m++) {
    if (!isBlackKey(m)) whiteCount++;
  }
  if (whiteCount === 0) return [];

  const wkW = width / whiteCount;
  const bkW = wkW * 0.62;
  const keys: KeyPos[] = [];
  let wi = 0;

  for (let m = minMidi; m <= maxMidi; m++) {
    if (isBlackKey(m)) {
      keys.push({ midi: m, x: wi * wkW - bkW / 2, w: bkW, black: true });
    } else {
      keys.push({ midi: m, x: wi * wkW, w: wkW, black: false });
      wi++;
    }
  }
  return keys;
}

function lowerBoundPreparedNotes(notes: readonly PreparedNote[], targetTime: number): number {
  let low = 0;
  let high = notes.length;

  while (low < high) {
    const mid = (low + high) >> 1;
    if (notes[mid].time < targetTime) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

function upperBoundPreparedNotes(notes: readonly PreparedNote[], targetTime: number): number {
  let low = 0;
  let high = notes.length;

  while (low < high) {
    const mid = (low + high) >> 1;
    if (notes[mid].time <= targetTime) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low;
}

// ── Rounded-rect helper ─────────────────────────────────────────────

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number, y: number, w: number, h: number, r: number,
) {
  r = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.arcTo(x + w, y, x + w, y + r, r);
  ctx.lineTo(x + w, y + h - r);
  ctx.arcTo(x + w, y + h, x + w - r, y + h, r);
  ctx.lineTo(x + r, y + h);
  ctx.arcTo(x, y + h, x, y + h - r, r);
  ctx.lineTo(x, y + r);
  ctx.arcTo(x, y, x + r, y, r);
  ctx.closePath();
}

// ── Component ───────────────────────────────────────────────────────

const HIT_ZONE_HEIGHT = 48;
const LOOK_AHEAD_SEC = 4;
const KEY_STRIP_HEIGHT = 28;
const NOTE_GAP_PX = 2;
const NOTE_RADIUS = 5;
const SUCCESS_PULSE_LIGHT = 'hsla(282, 92%, 58%, 0.76)';
const SUCCESS_PULSE_DARK = 'hsla(282, 100%, 74%, 0.86)';

const GRADE_COLORS: Record<string, { color: string; darkColor: string }> = {
  perfect: { color: 'hsl(45, 95%, 55%)', darkColor: 'hsl(45, 95%, 70%)' },
  good:    { color: 'hsl(145, 65%, 42%)', darkColor: 'hsl(145, 65%, 60%)' },
  miss:    { color: 'hsl(0, 70%, 50%)', darkColor: 'hsl(0, 70%, 65%)' },
};

const GRADE_LABELS: Record<string, string> = {
  perfect: 'PERFECT',
  good: 'GOOD',
  miss: 'MISS',
};

function drawHitFeedback(
  ctx: CanvasRenderingContext2D,
  fb: HitFeedback,
  perfNow: number,
  keyMap: Map<number, KeyPos>,
  playableH: number,
  isDark: boolean,
) {
  const age = perfNow - fb.createdAt;
  const ttl = 900;
  if (age > ttl) return;

  const progress = age / ttl;
  const alpha = 1 - progress * progress; // Ease out
  const yOffset = progress * 40; // Float upward

  const kp = keyMap.get(fb.midi);
  const x = kp ? kp.x + kp.w / 2 : ctx.canvas.width / 4;
  const y = playableH - 20 - yOffset;

  const gc = GRADE_COLORS[fb.grade];
  const color = isDark ? gc.darkColor : gc.color;

  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.font = `bold ${fb.grade === 'perfect' ? 14 : 12}px ui-sans-serif, system-ui, sans-serif`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  // Shadow for readability
  ctx.shadowColor = isDark ? 'rgba(0,0,0,0.6)' : 'rgba(255,255,255,0.8)';
  ctx.shadowBlur = 4;

  ctx.fillStyle = color;
  ctx.fillText(GRADE_LABELS[fb.grade], x, y);
  ctx.restore();
}

function drawCombo(
  ctx: CanvasRenderingContext2D,
  combo: number,
  canvasW: number,
  hitZoneY: number,
  isDark: boolean,
) {
  ctx.save();
  const x = canvasW - 16;
  const y = hitZoneY - 8;

  ctx.font = 'bold 18px ui-sans-serif, system-ui, sans-serif';
  ctx.textAlign = 'right';
  ctx.textBaseline = 'bottom';

  // Glow
  ctx.shadowColor = isDark ? 'rgba(255,200,50,0.4)' : 'rgba(200,150,0,0.3)';
  ctx.shadowBlur = 8;

  ctx.fillStyle = isDark ? 'rgba(255,220,100,0.85)' : 'rgba(180,130,0,0.8)';
  ctx.fillText(`${combo}×`, x, y);

  ctx.font = '10px ui-sans-serif, system-ui, sans-serif';
  ctx.fillStyle = isDark ? 'rgba(255,220,100,0.5)' : 'rgba(180,130,0,0.5)';
  ctx.fillText('COMBO', x, y + 13);
  ctx.restore();
}

function drawConfirmedHitKeyBadge(
  ctx: CanvasRenderingContext2D,
  keyLabel: string,
  x: number,
  width: number,
  centerY: number,
  hue: number,
  sat: number,
  isDark: boolean,
) {
  ctx.save();
  ctx.font = 'bold 10px ui-monospace, SFMono-Regular, monospace';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';

  const textWidth = ctx.measureText(keyLabel).width;
  const badgeWidth = Math.max(24, textWidth + 14);
  const badgeHeight = 18;
  const badgeX = x + width / 2 - badgeWidth / 2;
  const badgeY = centerY - badgeHeight / 2;

  ctx.shadowColor = isDark
    ? `hsla(${hue}, ${sat}%, 78%, 0.5)`
    : `hsla(${hue}, ${sat}%, 52%, 0.35)`;
  ctx.shadowBlur = 12;
  roundRect(ctx, badgeX, badgeY, badgeWidth, badgeHeight, 9);
  ctx.fillStyle = isDark
    ? `hsla(${hue}, ${sat}%, 72%, 0.96)`
    : `hsla(${hue}, ${sat}%, 48%, 0.92)`;
  ctx.fill();
  ctx.strokeStyle = isDark
    ? 'hsla(0, 0%, 100%, 0.18)'
    : 'hsla(0, 0%, 100%, 0.28)';
  ctx.lineWidth = 1;
  ctx.stroke();

  ctx.shadowBlur = 0;
  ctx.fillStyle = isDark ? 'rgba(16, 16, 20, 0.96)' : 'rgba(255, 255, 255, 0.96)';
  ctx.fillText(keyLabel, badgeX + badgeWidth / 2, badgeY + badgeHeight / 2 + 0.5);
  ctx.restore();
}

interface WaterfallCanvasProps {
  song: TutorialSong;
  getCurrentTime: () => number;
  isPlaying: boolean;
  isDark: boolean;
  keyLabelByMidi: ReadonlyMap<number, string>;
}

const WaterfallCanvasComponent = ({
  song,
  getCurrentTime,
  isPlaying,
  isDark,
  keyLabelByMidi,
}: WaterfallCanvasProps) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const stoppedRef = useRef(false);
  const keyLayoutCacheRef = useRef<CachedKeyLayout | null>(null);
  const viewportRef = useRef({ width: 0, height: 0, dpr: 1 });
  const scoringMaintenanceRef = useRef({
    lastSweepSongTime: Number.NEGATIVE_INFINITY,
    lastCleanupPerfTime: Number.NEGATIVE_INFINITY,
  });

  const {
    preparedNotes,
    maxNoteDuration,
    minMidi,
    maxMidi,
  } = useMemo(() => {
    if (song.notes.length === 0) {
      return {
        preparedNotes: [] as PreparedNote[],
        maxNoteDuration: 0,
        minMidi: 60,
        maxMidi: 72,
      };
    }

    let lo = 127;
    let hi = 0;
    let maxDuration = 0;
    const nextPreparedNotes = song.notes
      .map((note, index) => {
        lo = Math.min(lo, note.midi);
        hi = Math.max(hi, note.midi);
        maxDuration = Math.max(maxDuration, note.duration);

        const [hue, sat] = NOTE_HSL[((note.midi % 12) + 12) % 12] ?? [0, 0];
        return {
          index,
          midi: note.midi,
          time: note.time,
          duration: note.duration,
          endTime: note.time + note.duration,
          hue,
          sat,
        };
      })
      .sort((left, right) => left.time - right.time || left.midi - right.midi || left.index - right.index);

    return {
      preparedNotes: nextPreparedNotes,
      maxNoteDuration: maxDuration,
      minMidi: Math.max(0, lo - 2),
      maxMidi: Math.min(127, hi + 2),
    };
  }, [song]);

  const syncCanvasViewport = useCallback(() => {
    const canvas = canvasRef.current;
    const container = containerRef.current;
    if (!canvas || !container) {
      return;
    }

    const rect = container.getBoundingClientRect();
    const width = Math.round(rect.width);
    const height = Math.round(rect.height);
    const dpr = window.devicePixelRatio || 1;
    const pixelWidth = Math.max(1, Math.round(width * dpr));
    const pixelHeight = Math.max(1, Math.round(height * dpr));

    if (viewportRef.current.width === width
      && viewportRef.current.height === height
      && viewportRef.current.dpr === dpr
      && canvas.width === pixelWidth
      && canvas.height === pixelHeight
    ) {
      return;
    }

    viewportRef.current = { width, height, dpr };
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    keyLayoutCacheRef.current = null;
  }, []);

  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const { width: W, height: H, dpr } = viewportRef.current;
    if (W <= 0 || H <= 0) {
      return;
    }

    const ctx = canvas.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    // ── Background ──
    ctx.fillStyle = isDark ? 'rgba(18,18,20,0.95)' : 'rgba(248,247,245,0.95)';
    ctx.fillRect(0, 0, W, H);

    const playableH = H - KEY_STRIP_HEIGHT;
    const hitZoneY = playableH - HIT_ZONE_HEIGHT;
    const pxPerSec = playableH / LOOK_AHEAD_SEC;
    const now = getCurrentTime();

    let cachedKeyLayout = keyLayoutCacheRef.current;
    if (
      !cachedKeyLayout
      || cachedKeyLayout.width !== W
      || cachedKeyLayout.minMidi !== minMidi
      || cachedKeyLayout.maxMidi !== maxMidi
    ) {
      const keys = computeKeyLayout(minMidi, maxMidi, W);
      cachedKeyLayout = {
        width: W,
        minMidi,
        maxMidi,
        keys,
        keyMap: new Map(keys.map((key) => [key.midi, key])),
      };
      keyLayoutCacheRef.current = cachedKeyLayout;
    }

    const { keys, keyMap } = cachedKeyLayout;

    // ── Vertical gridlines ──
    ctx.lineWidth = 1;
    for (const k of keys) {
      if (k.black) continue;
      ctx.strokeStyle = isDark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.04)';
      ctx.beginPath();
      ctx.moveTo(k.x, 0);
      ctx.lineTo(k.x, playableH);
      ctx.stroke();
    }

    // ── Hit zone ──
    const hitGrad = ctx.createLinearGradient(0, hitZoneY, 0, playableH);
    if (isDark) {
      hitGrad.addColorStop(0, 'rgba(255,255,255,0)');
      hitGrad.addColorStop(0.4, 'rgba(255,255,255,0.03)');
      hitGrad.addColorStop(1, 'rgba(255,255,255,0.06)');
    } else {
      hitGrad.addColorStop(0, 'rgba(0,0,0,0)');
      hitGrad.addColorStop(0.4, 'rgba(0,0,0,0.02)');
      hitGrad.addColorStop(1, 'rgba(0,0,0,0.05)');
    }
    ctx.fillStyle = hitGrad;
    ctx.fillRect(0, hitZoneY, W, HIT_ZONE_HEIGHT);

    // Hit zone line
    ctx.strokeStyle = isDark ? 'rgba(255,255,255,0.12)' : 'rgba(0,0,0,0.08)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(0, playableH);
    ctx.lineTo(W, playableH);
    ctx.stroke();

    // ── Draw notes ──
    const activeMidis = new Set<number>();
    const hitWindow = HIT_ZONE_HEIGHT / pxPerSec;
    let scoringState = useScoringStore.getState();
    const judgedIndices = scoringState.judgedIndices;
    const successPulseIdsByMidi = getLatestActiveSuccessPulseIdsByMidi(scoringState.successPulses);
    const activeHoldIndices = new Set(scoringState.activeHolds.map((h) => h.noteIndex));
    const heldMidis = scoringState.heldMidis;
    const visiblePastWindowSec = Math.max(maxNoteDuration + 0.25, LOOK_AHEAD_SEC + 0.5);
    const visibleStartIndex = lowerBoundPreparedNotes(preparedNotes, now - visiblePastWindowSec);
    const visibleEndIndex = upperBoundPreparedNotes(preparedNotes, now + LOOK_AHEAD_SEC + hitWindow + 0.25);

    for (let preparedIndex = visibleStartIndex; preparedIndex < visibleEndIndex; preparedIndex++) {
      const note = preparedNotes[preparedIndex];
      const kp = keyMap.get(note.midi);
      if (!kp) continue;

      const startY = playableH - (note.time - now) * pxPerSec;
      const endY = playableH - (note.endTime - now) * pxPerSec;
      const noteH = startY - endY;

      // Cull off-screen
      if (startY < -noteH || endY > H + 10) continue;

      const { hue, sat } = note;
      const inHitZone = now >= note.time - 0.05 && now <= note.time + note.duration;
      const approaching = now >= note.time - hitWindow && now < note.time;
      const judged = judgedIndices.has(note.index);
      const pastNote = now > note.endTime;
      const isBeingHeld = activeHoldIndices.has(note.index);
      const hitSuccess = successPulseIdsByMidi.has(note.midi);
      const confirmedKeyLabel = keyLabelByMidi.get(note.midi);

      if (inHitZone) activeMidis.add(note.midi);

      const x = kp.x + NOTE_GAP_PX;
      const w = kp.w - NOTE_GAP_PX * 2;
      const y = endY;
      const h = Math.max(noteH - NOTE_GAP_PX, 4);

      // Judged notes that have passed: dim them
      if (judged && pastNote) {
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat * 0.3}%, 35%, 0.2)`
          : `hsla(${hue}, ${sat * 0.3}%, 60%, 0.15)`;
        ctx.fill();
        continue;
      }

      // Missed notes that have passed: show as red/grey
      if (!judged && !isBeingHeld && pastNote) {
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.fillStyle = isDark ? 'rgba(180,50,50,0.25)' : 'rgba(200,60,60,0.18)';
        ctx.fill();
        ctx.strokeStyle = isDark ? 'rgba(180,50,50,0.15)' : 'rgba(200,60,60,0.1)';
        ctx.lineWidth = 1;
        ctx.stroke();
        continue;
      }

      // ── Active hold: show fill progress + glow ──
      if (isBeingHeld) {
        // Draw base (unfilled part) dimmer
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat}%, 40%, 0.3)`
          : `hsla(${hue}, ${sat}%, 65%, 0.25)`;
        ctx.fill();

        // Compute filled portion (from judgment line upward to current time)
        const holdProgress = Math.min(1, Math.max(0,
          (now - note.time) / note.duration));
        const filledH = h * holdProgress;
        const filledY = y + h - filledH;

        // Bright fill for the held portion
        ctx.save();
        ctx.beginPath();
        ctx.rect(x, filledY, w, filledH);
        ctx.clip();
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat}%, 68%, 0.92)`
          : `hsla(${hue}, ${sat}%, 52%, 0.88)`;
        ctx.fill();
        ctx.restore();

        // Hold glow
        ctx.save();
        ctx.shadowColor = isDark
          ? `hsla(${hue}, ${sat}%, 70%, 0.7)`
          : `hsla(${hue}, ${sat}%, 50%, 0.6)`;
        ctx.shadowBlur = 22;
        roundRect(ctx, x, filledY, w, filledH, NOTE_RADIUS);
        ctx.fillStyle = 'transparent';
        ctx.fill();
        ctx.restore();

        // Border
        ctx.strokeStyle = isDark
          ? `hsla(${hue}, ${sat}%, 75%, 0.6)`
          : `hsla(${hue}, ${sat}%, 45%, 0.5)`;
        ctx.lineWidth = 1.5;
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.stroke();

        if (hitSuccess && inHitZone && confirmedKeyLabel) {
          drawConfirmedHitKeyBadge(
            ctx,
            confirmedKeyLabel,
            x,
            w,
            Math.max(y + 14, playableH - 18),
            hue,
            sat,
            isDark,
          );
        }
        continue;
      }

      // Glow for approaching/active notes
      if (inHitZone || approaching) {
        ctx.save();
        ctx.shadowColor = isDark
          ? `hsla(${hue}, ${sat}%, 65%, 0.6)`
          : `hsla(${hue}, ${sat}%, 50%, 0.5)`;
        ctx.shadowBlur = inHitZone ? 18 : 8;
        ctx.shadowOffsetX = 0;
        ctx.shadowOffsetY = 0;
        roundRect(ctx, x, y, w, h, NOTE_RADIUS);
        ctx.fillStyle = 'transparent';
        ctx.fill();
        ctx.restore();
      }

      // Note body
      const lightness = isDark ? (inHitZone ? 65 : 55) : (inHitZone ? 55 : 48);
      const alpha = inHitZone ? 0.9 : (approaching ? 0.75 : 0.55);
      roundRect(ctx, x, y, w, h, NOTE_RADIUS);
      ctx.fillStyle = `hsla(${hue}, ${sat}%, ${lightness}%, ${alpha})`;
      ctx.fill();

      // Subtle border
      ctx.strokeStyle = `hsla(${hue}, ${sat}%, ${lightness - 10}%, ${alpha * 0.6})`;
      ctx.lineWidth = 1;
      ctx.stroke();

      if (hitSuccess && inHitZone) {
        const bandHeight = Math.min(h, 30);
        const bandY = Math.max(y, playableH - bandHeight - 4);

        ctx.save();
        roundRect(ctx, x, bandY, w, bandHeight, NOTE_RADIUS);
        ctx.fillStyle = isDark
          ? 'hsla(286, 100%, 76%, 0.22)'
          : 'hsla(282, 92%, 58%, 0.18)';
        ctx.fill();
        ctx.restore();

        if (confirmedKeyLabel) {
          drawConfirmedHitKeyBadge(
            ctx,
            confirmedKeyLabel,
            x,
            w,
            Math.max(y + 14, playableH - 18),
            hue,
            sat,
            isDark,
          );
        }
      }
    }

    // ── Key strip at bottom ──
    for (const k of keys) {
      const isActive = activeMidis.has(k.midi);
      const hitSuccess = successPulseIdsByMidi.has(k.midi);
      const isHeld = heldMidis.has(k.midi);
      const [hue, sat] = NOTE_HSL[((k.midi % 12) + 12) % 12] ?? [0, 0];

      // Sustained hold glow (stronger, persistent while held)
      if (isHeld) {
        ctx.save();
        ctx.shadowColor = isDark
          ? `hsla(${hue}, ${sat}%, 70%, 0.8)`
          : `hsla(${hue}, ${sat}%, 50%, 0.7)`;
        ctx.shadowBlur = 22;
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat}%, 65%, 0.25)`
          : `hsla(${hue}, ${sat}%, 50%, 0.15)`;
        ctx.fillRect(k.x + 1, playableH - 10, k.w - 2, KEY_STRIP_HEIGHT + 12);
        ctx.restore();
      } else if (hitSuccess) {
        ctx.save();
        ctx.shadowColor = isDark ? SUCCESS_PULSE_DARK : SUCCESS_PULSE_LIGHT;
        ctx.shadowBlur = 18;
        ctx.fillStyle = isDark
          ? 'hsla(282, 100%, 70%, 0.28)'
          : 'hsla(282, 92%, 58%, 0.18)';
        ctx.fillRect(k.x + 1, playableH - 8, k.w - 2, KEY_STRIP_HEIGHT + 10);
        ctx.restore();
      }

      // Key background
      if (isHeld) {
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat}%, 45%, 0.92)`
          : `hsla(${hue}, ${sat}%, 58%, 0.85)`;
      } else if (hitSuccess) {
        ctx.fillStyle = isDark
          ? 'hsla(282, 78%, 44%, 0.94)'
          : 'hsla(282, 88%, 62%, 0.88)';
      } else if (k.black) {
        ctx.fillStyle = isDark
          ? (isActive ? `hsla(${hue}, ${sat}%, 40%, 0.8)` : 'rgba(40,40,44,0.9)')
          : (isActive ? `hsla(${hue}, ${sat}%, 45%, 0.7)` : 'rgba(50,50,55,0.85)');
      } else {
        ctx.fillStyle = isDark
          ? (isActive ? `hsla(${hue}, ${sat}%, 25%, 0.6)` : 'rgba(30,30,34,0.7)')
          : (isActive ? `hsla(${hue}, ${sat}%, 85%, 0.8)` : 'rgba(240,238,235,0.9)');
      }
      ctx.fillRect(k.x + 0.5, playableH + 1, k.w - 1, KEY_STRIP_HEIGHT - 2);

      // Key border
      const showKeyGlow = isHeld || hitSuccess;
      ctx.strokeStyle = showKeyGlow
        ? (isHeld
          ? (isDark ? `hsla(${hue}, ${sat}%, 80%, 0.7)` : `hsla(${hue}, ${sat}%, 40%, 0.5)`)
          : (isDark ? 'hsla(282, 100%, 82%, 0.7)' : 'hsla(282, 88%, 46%, 0.45)'))
        : (isDark ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.08)');
      ctx.lineWidth = isHeld ? 1 : 0.5;
      ctx.strokeRect(k.x + 0.5, playableH + 1, k.w - 1, KEY_STRIP_HEIGHT - 2);

      if (isHeld) {
        // Sustained hold beam above the key
        ctx.fillStyle = isDark
          ? `hsla(${hue}, ${sat}%, 72%, 0.85)`
          : `hsla(${hue}, ${sat}%, 48%, 0.7)`;
        ctx.fillRect(k.x + 1.5, playableH - 3, k.w - 3, 4);
      } else if (hitSuccess) {
        ctx.fillStyle = isDark ? SUCCESS_PULSE_DARK : SUCCESS_PULSE_LIGHT;
        ctx.fillRect(k.x + 1.5, playableH - 3, k.w - 3, 4);
      }

      // Key label
      if (!k.black) {
        const label = midiNoteName(k.midi);
        ctx.font = '9px ui-monospace, SFMono-Regular, monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillStyle = isHeld
          ? (isDark ? 'rgba(255,255,255,0.96)' : `hsl(${hue}, ${sat}%, 20%)`)
          : hitSuccess
            ? (isDark ? 'rgba(255,255,255,0.96)' : 'hsl(282, 88%, 24%)')
            : isDark
              ? (isActive ? `hsl(${hue}, ${sat}%, 80%)` : 'rgba(255,255,255,0.35)')
              : (isActive ? `hsl(${hue}, ${sat}%, 35%)` : 'rgba(0,0,0,0.3)');
        ctx.fillText(label, k.x + k.w / 2, playableH + KEY_STRIP_HEIGHT / 2);
      }
    }

    // ── Scoring: sweep missed notes ──
    if (scoringState.enabled) {
      if (now - scoringMaintenanceRef.current.lastSweepSongTime >= 1 / 30) {
        scoringState.sweepMisses(now, song.notes);
        scoringMaintenanceRef.current.lastSweepSongTime = now;
        scoringState = useScoringStore.getState();
      }

      const perfNow = performance.now();
      if (perfNow - scoringMaintenanceRef.current.lastCleanupPerfTime >= 70) {
        scoringState.cleanFeedbacks();
        scoringMaintenanceRef.current.lastCleanupPerfTime = perfNow;
        scoringState = useScoringStore.getState();
      }
    }

    // ── Draw scoring feedback popups ──
    const feedbacks = scoringState.feedbacks;
    if (feedbacks.length > 0) {
      const perfNow = performance.now();
      for (const fb of feedbacks) {
        drawHitFeedback(ctx, fb, perfNow, keyMap, playableH, isDark);
      }
    }

    // ── Draw combo ──
    const currentCombo = scoringState.combo;
    if (currentCombo > 2) {
      drawCombo(ctx, currentCombo, W, hitZoneY, isDark);
    }

    // Auto-stop when song ends
    if (now > song.duration + 1 && !stoppedRef.current) {
      stoppedRef.current = true;
    }
  }, [song, getCurrentTime, isDark, keyLabelByMidi, minMidi, maxMidi]);

  useEffect(() => {
    stoppedRef.current = false;
    scoringMaintenanceRef.current = {
      lastSweepSongTime: Number.NEGATIVE_INFINITY,
      lastCleanupPerfTime: Number.NEGATIVE_INFINITY,
    };
  }, [song.id]);

  useEffect(() => {
    syncCanvasViewport();

    const container = containerRef.current;
    if (!container) {
      return;
    }

    let frameId: number | null = null;
    const queueViewportSync = () => {
      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }

      frameId = window.requestAnimationFrame(() => {
        syncCanvasViewport();
        draw();
        frameId = null;
      });
    };

    const resizeObserver = typeof ResizeObserver === 'undefined'
      ? null
      : new ResizeObserver(() => {
          queueViewportSync();
        });

    resizeObserver?.observe(container);
    window.addEventListener('resize', queueViewportSync);

    return () => {
      resizeObserver?.disconnect();
      window.removeEventListener('resize', queueViewportSync);

      if (frameId !== null) {
        window.cancelAnimationFrame(frameId);
      }
    };
  }, [draw, syncCanvasViewport]);

  useEffect(() => {
    let running = true;

    function loop() {
      if (!running) return;
      draw();

      const scoringState = useScoringStore.getState();
      const shouldKeepAnimating = isPlaying
        || scoringState.activeHolds.length > 0
        || scoringState.feedbacks.length > 0
        || scoringState.successPulses.length > 0;

      if (!shouldKeepAnimating) {
        rafRef.current = 0;
        return;
      }

      rafRef.current = requestAnimationFrame(loop);
    }

    syncCanvasViewport();
    loop();
    return () => {
      running = false;
      if (rafRef.current !== 0) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = 0;
      }
    };
  }, [draw, isPlaying, syncCanvasViewport]);

  return (
    <div ref={containerRef} className="waterfall-container">
      <canvas ref={canvasRef} className="waterfall-canvas" />
    </div>
  );
};

export const WaterfallCanvas = memo(WaterfallCanvasComponent);
WaterfallCanvas.displayName = 'WaterfallCanvas';
