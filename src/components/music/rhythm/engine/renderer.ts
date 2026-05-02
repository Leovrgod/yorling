import { ParticlePool } from './effects';
import type { HitGrade } from './judge';
import {
  HIT_LINE_FRAGMENT,
  HIT_LINE_VERTEX,
  LANES_FRAGMENT,
  LANES_VERTEX,
  NOTES_FRAGMENT,
  NOTES_VERTEX,
  PARTICLES_FRAGMENT,
  PARTICLES_VERTEX,
} from './shaders';
import {
  firstVisibleIndex,
  isBlackKey,
  lastVisibleIndex,
  type PreparedChart,
} from './chart';

const LOOK_AHEAD_SEC = 4.35;
const KEY_STRIP_HEIGHT = 34;
const HIT_LINE_OFFSET = 18;
const TOP_PADDING = 18;
const NOTE_SIDE_INSET = 7;
const BLACK_NOTE_SIDE_INSET = 5;
const MIN_NOTE_HEIGHT = 12;
const MAX_NOTE_INSTANCES = 4096;

interface LaneGeometry {
  midi: number;
  x: number;
  width: number;
  center: number;
  black: boolean;
  color: [number, number, number];
}

interface LayoutCache {
  width: number;
  minMidi: number;
  maxMidi: number;
  isDark: boolean;
  lanes: LaneGeometry[];
  laneByMidi: Map<number, LaneGeometry>;
}

export interface RhythmLaneGuide {
  midi: number;
  center: number;
  width: number;
  black: boolean;
}

interface LanesProgram {
  program: WebGLProgram;
  vao: WebGLVertexArrayObject;
  quad: WebGLBuffer;
  instances: WebGLBuffer;
  uViewport: WebGLUniformLocation;
  uHitLineY01: WebGLUniformLocation;
  uIntensity: WebGLUniformLocation;
}

interface NotesProgram {
  program: WebGLProgram;
  vao: WebGLVertexArrayObject;
  quad: WebGLBuffer;
  instances: WebGLBuffer;
  uViewport: WebGLUniformLocation;
}

interface ParticlesProgram {
  program: WebGLProgram;
  vao: WebGLVertexArrayObject;
  quad: WebGLBuffer;
  instances: WebGLBuffer;
  uViewport: WebGLUniformLocation;
  uTime: WebGLUniformLocation;
}

interface HitLineProgram {
  program: WebGLProgram;
  vao: WebGLVertexArrayObject;
  quad: WebGLBuffer;
  uViewport: WebGLUniformLocation;
  uHitLineY: WebGLUniformLocation;
  uHeight: WebGLUniformLocation;
  uColor: WebGLUniformLocation;
  uPulse: WebGLUniformLocation;
}

export interface RhythmRenderFrame {
  chart: PreparedChart;
  songTime: number;
  judgedIndices: ReadonlySet<number>;
  activeHoldIndices: ReadonlySet<number>;
  heldMidis: ReadonlySet<number>;
  latestPulseIdsByMidi: ReadonlyMap<number, number>;
  latestPulseGradesByMidi?: ReadonlyMap<number, HitGrade>;
  combo: number;
  isDark: boolean;
}

const NOTE_HUE_SAT: ReadonlyArray<readonly [number, number]> = [
  [0, 72],
  [25, 80],
  [35, 85],
  [50, 80],
  [80, 60],
  [145, 55],
  [170, 55],
  [190, 70],
  [220, 65],
  [240, 55],
  [265, 55],
  [280, 55],
];

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function mixRgb(
  left: readonly [number, number, number],
  right: readonly [number, number, number],
  amount: number,
): [number, number, number] {
  const t = clamp01(amount);
  return [
    left[0] + (right[0] - left[0]) * t,
    left[1] + (right[1] - left[1]) * t,
    left[2] + (right[2] - left[2]) * t,
  ];
}

function hslToRgb(hue: number, satPercent: number, lightPercent: number): [number, number, number] {
  const h = (((hue % 360) + 360) % 360) / 360;
  const s = clamp01(satPercent / 100);
  const l = clamp01(lightPercent / 100);

  if (s === 0) {
    return [l, l, l];
  }

  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const toChannel = (tRaw: number) => {
    let t = tRaw;
    if (t < 0) t += 1;
    if (t > 1) t -= 1;
    if (t < 1 / 6) return p + (q - p) * 6 * t;
    if (t < 1 / 2) return q;
    if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
    return p;
  };

  return [
    toChannel(h + 1 / 3),
    toChannel(h),
    toChannel(h - 1 / 3),
  ];
}

function laneColorForMidi(midi: number, isDark: boolean): [number, number, number] {
  const [hue, sat] = NOTE_HUE_SAT[((midi % 12) + 12) % 12];
  return hslToRgb(hue, sat, isDark ? 38 : 60);
}

function hitColorForGrade(grade: HitGrade | undefined, fallback: readonly [number, number, number]): [number, number, number] {
  if (grade === 'perfect') return [1, 0.82, 0.26];
  if (grade === 'great') return [0.44, 0.88, 0.98];
  if (grade === 'good') return [0.58, 0.92, 0.6];
  return [fallback[0], fallback[1], fallback[2]];
}

function createShader(
  gl: WebGL2RenderingContext,
  type: number,
  source: string,
): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) {
    throw new Error('Failed to allocate WebGL shader.');
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const info = gl.getShaderInfoLog(shader) ?? 'Unknown shader compile error';
    gl.deleteShader(shader);
    throw new Error(info);
  }
  return shader;
}

function createProgram(
  gl: WebGL2RenderingContext,
  vertexSource: string,
  fragmentSource: string,
): WebGLProgram {
  const vertexShader = createShader(gl, gl.VERTEX_SHADER, vertexSource);
  const fragmentShader = createShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
  const program = gl.createProgram();
  if (!program) {
    gl.deleteShader(vertexShader);
    gl.deleteShader(fragmentShader);
    throw new Error('Failed to allocate WebGL program.');
  }

  gl.attachShader(program, vertexShader);
  gl.attachShader(program, fragmentShader);
  gl.linkProgram(program);
  gl.deleteShader(vertexShader);
  gl.deleteShader(fragmentShader);

  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const info = gl.getProgramInfoLog(program) ?? 'Unknown program link error';
    gl.deleteProgram(program);
    throw new Error(info);
  }
  return program;
}

function getAttribLocation(gl: WebGL2RenderingContext, program: WebGLProgram, name: string): number {
  const location = gl.getAttribLocation(program, name);
  if (location < 0) {
    throw new Error(`Missing WebGL attribute "${name}".`);
  }
  return location;
}

function getUniformLocation(gl: WebGL2RenderingContext, program: WebGLProgram, name: string): WebGLUniformLocation {
  const location = gl.getUniformLocation(program, name);
  if (!location) {
    throw new Error(`Missing WebGL uniform "${name}".`);
  }
  return location;
}

function createBuffer(gl: WebGL2RenderingContext): WebGLBuffer {
  const buffer = gl.createBuffer();
  if (!buffer) {
    throw new Error('Failed to allocate WebGL buffer.');
  }
  return buffer;
}

function createVertexArray(gl: WebGL2RenderingContext): WebGLVertexArrayObject {
  const vao = gl.createVertexArray();
  if (!vao) {
    throw new Error('Failed to allocate WebGL vertex array.');
  }
  return vao;
}

function expandLaneRange(minMidi: number, maxMidi: number): [number, number] {
  let expandedMin = minMidi;
  let expandedMax = maxMidi;
  while (expandedMin > 0 && isBlackKey(expandedMin)) expandedMin--;
  while (expandedMax < 127 && isBlackKey(expandedMax)) expandedMax++;
  return [expandedMin, expandedMax];
}

function buildLaneLayout(minMidi: number, maxMidi: number, width: number, isDark: boolean): LayoutCache {
  const [expandedMin, expandedMax] = expandLaneRange(minMidi, maxMidi);
  let whiteCount = 0;
  for (let midi = expandedMin; midi <= expandedMax; midi++) {
    if (!isBlackKey(midi)) whiteCount++;
  }

  const laneByMidi = new Map<number, LaneGeometry>();
  if (whiteCount === 0 || width <= 0) {
    return {
      width,
      minMidi,
      maxMidi,
      isDark,
      lanes: [],
      laneByMidi,
    };
  }

  const whiteWidth = width / whiteCount;
  const blackWidth = whiteWidth * 0.62;
  const lanes: LaneGeometry[] = [];
  let whiteIndex = 0;

  for (let midi = expandedMin; midi <= expandedMax; midi++) {
    if (isBlackKey(midi)) {
      const x = whiteIndex * whiteWidth - blackWidth * 0.5;
      const lane: LaneGeometry = {
        midi,
        x,
        width: blackWidth,
        center: x + blackWidth * 0.5,
        black: true,
        color: laneColorForMidi(midi, isDark),
      };
      lanes.push(lane);
      laneByMidi.set(midi, lane);
      continue;
    }

    const x = whiteIndex * whiteWidth;
    const lane: LaneGeometry = {
      midi,
      x,
      width: whiteWidth,
      center: x + whiteWidth * 0.5,
      black: false,
      color: laneColorForMidi(midi, isDark),
    };
    lanes.push(lane);
    laneByMidi.set(midi, lane);
    whiteIndex++;
  }

  return {
    width,
    minMidi,
    maxMidi,
    isDark,
    lanes,
    laneByMidi,
  };
}

export function buildRhythmLaneGuides(
  minMidi: number,
  maxMidi: number,
  width: number,
  isDark: boolean,
): RhythmLaneGuide[] {
  if (width <= 0) {
    return [];
  }

  return buildLaneLayout(minMidi, maxMidi, width, isDark).lanes.map((lane) => ({
    midi: lane.midi,
    center: lane.center,
    width: lane.width,
    black: lane.black,
  }));
}

export class RhythmRenderer {
  private readonly gl: WebGL2RenderingContext;
  private readonly lanesProgram: LanesProgram;
  private readonly notesProgram: NotesProgram;
  private readonly particlesProgram: ParticlesProgram;
  private readonly hitLineProgram: HitLineProgram;
  private readonly particles = new ParticlePool();
  private layoutCache: LayoutCache | null = null;
  private width = 1;
  private height = 1;
  private noteInstanceData = new Float32Array(MAX_NOTE_INSTANCES * 10);
  private laneInstanceData = new Float32Array(0);
  private particleTempOrigin = new Float32Array(this.particles.capacity() * 2);
  private particleTempVelocity = new Float32Array(this.particles.capacity() * 2);
  private particleTempColor = new Float32Array(this.particles.capacity() * 3);
  private particleTempBirth = new Float32Array(this.particles.capacity());
  private particleTempLifetime = new Float32Array(this.particles.capacity());
  private particleTempSize = new Float32Array(this.particles.capacity());
  private particleInstanceData = new Float32Array(this.particles.capacity() * 10);
  private latestPulseIdsByMidi = new Map<number, number>();
  private lastPulseAtSec = 0;
  private disposed = false;

  constructor(private readonly canvas: HTMLCanvasElement) {
    const gl = canvas.getContext('webgl2', {
      alpha: true,
      antialias: true,
      powerPreference: 'high-performance',
      premultipliedAlpha: true,
    });
    if (!gl) {
      throw new Error('WebGL2 is unavailable.');
    }
    this.gl = gl;
    this.lanesProgram = this.createLanesProgram();
    this.notesProgram = this.createNotesProgram();
    this.particlesProgram = this.createParticlesProgram();
    this.hitLineProgram = this.createHitLineProgram();

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  resize(width: number, height: number, dpr = window.devicePixelRatio || 1): void {
    const nextWidth = Math.max(1, Math.round(width));
    const nextHeight = Math.max(1, Math.round(height));
    const pixelWidth = Math.max(1, Math.round(nextWidth * dpr));
    const pixelHeight = Math.max(1, Math.round(nextHeight * dpr));

    if (this.canvas.width !== pixelWidth) this.canvas.width = pixelWidth;
    if (this.canvas.height !== pixelHeight) this.canvas.height = pixelHeight;
    this.canvas.style.width = `${nextWidth}px`;
    this.canvas.style.height = `${nextHeight}px`;

    this.width = nextWidth;
    this.height = nextHeight;
    this.gl.viewport(0, 0, pixelWidth, pixelHeight);
    this.layoutCache = null;
  }

  resetTransientState(): void {
    this.latestPulseIdsByMidi.clear();
    this.lastPulseAtSec = 0;
    this.particles.clear();
  }

  render(frame: RhythmRenderFrame): void {
    if (this.disposed) return;
    const gl = this.gl;
    const layout = this.getLayout(frame.chart, frame.isDark);
    const hitLineY = this.height - KEY_STRIP_HEIGHT - HIT_LINE_OFFSET;
    const nowSec = performance.now() / 1000;

    this.emitNewHitBursts(layout, frame, hitLineY, nowSec);

    const clear = frame.isDark
      ? [0.09, 0.09, 0.11, 1]
      : [0.97, 0.96, 0.94, 1];
    gl.clearColor(clear[0], clear[1], clear[2], clear[3]);
    gl.clear(gl.COLOR_BUFFER_BIT);

    this.drawLanes(layout, frame, hitLineY);
    this.drawNotes(layout, frame, hitLineY);
    this.drawHitLine(frame, hitLineY, nowSec);
    this.drawParticles(nowSec);
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;

    const gl = this.gl;
    gl.deleteProgram(this.lanesProgram.program);
    gl.deleteProgram(this.notesProgram.program);
    gl.deleteProgram(this.particlesProgram.program);
    gl.deleteProgram(this.hitLineProgram.program);

    gl.deleteBuffer(this.lanesProgram.quad);
    gl.deleteBuffer(this.lanesProgram.instances);
    gl.deleteBuffer(this.notesProgram.quad);
    gl.deleteBuffer(this.notesProgram.instances);
    gl.deleteBuffer(this.particlesProgram.quad);
    gl.deleteBuffer(this.particlesProgram.instances);
    gl.deleteBuffer(this.hitLineProgram.quad);

    gl.deleteVertexArray(this.lanesProgram.vao);
    gl.deleteVertexArray(this.notesProgram.vao);
    gl.deleteVertexArray(this.particlesProgram.vao);
    gl.deleteVertexArray(this.hitLineProgram.vao);
  }

  private createLanesProgram(): LanesProgram {
    const gl = this.gl;
    const program = createProgram(gl, LANES_VERTEX, LANES_FRAGMENT);
    const vao = createVertexArray(gl);
    const quad = createBuffer(gl);
    const instances = createBuffer(gl);

    gl.bindVertexArray(vao);

    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -1, 0,
      1, 0,
      -1, 1,
      1, 1,
    ]), gl.STATIC_DRAW);
    const aPos = getAttribLocation(gl, program, 'aPos');
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, instances);
    const aLane = getAttribLocation(gl, program, 'aLane');
    const aColor = getAttribLocation(gl, program, 'aColor');
    const aIsBlack = getAttribLocation(gl, program, 'aIsBlack');
    const stride = 6 * 4;
    gl.enableVertexAttribArray(aLane);
    gl.vertexAttribPointer(aLane, 2, gl.FLOAT, false, stride, 0);
    gl.vertexAttribDivisor(aLane, 1);
    gl.enableVertexAttribArray(aColor);
    gl.vertexAttribPointer(aColor, 3, gl.FLOAT, false, stride, 2 * 4);
    gl.vertexAttribDivisor(aColor, 1);
    gl.enableVertexAttribArray(aIsBlack);
    gl.vertexAttribPointer(aIsBlack, 1, gl.FLOAT, false, stride, 5 * 4);
    gl.vertexAttribDivisor(aIsBlack, 1);

    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

    return {
      program,
      vao,
      quad,
      instances,
      uViewport: getUniformLocation(gl, program, 'uViewport'),
      uHitLineY01: getUniformLocation(gl, program, 'uHitLineY01'),
      uIntensity: getUniformLocation(gl, program, 'uIntensity'),
    };
  }

  private createNotesProgram(): NotesProgram {
    const gl = this.gl;
    const program = createProgram(gl, NOTES_VERTEX, NOTES_FRAGMENT);
    const vao = createVertexArray(gl);
    const quad = createBuffer(gl);
    const instances = createBuffer(gl);

    gl.bindVertexArray(vao);

    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -0.5, -0.5,
      0.5, -0.5,
      -0.5, 0.5,
      0.5, 0.5,
    ]), gl.STATIC_DRAW);
    const aQuad = getAttribLocation(gl, program, 'aQuad');
    gl.enableVertexAttribArray(aQuad);
    gl.vertexAttribPointer(aQuad, 2, gl.FLOAT, false, 0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, instances);
    const stride = 10 * 4;
    const iCenter = getAttribLocation(gl, program, 'iCenter');
    const iSize = getAttribLocation(gl, program, 'iSize');
    const iColor = getAttribLocation(gl, program, 'iColor');
    const iGlow = getAttribLocation(gl, program, 'iGlow');
    const iIsHold = getAttribLocation(gl, program, 'iIsHold');
    const iAlpha = getAttribLocation(gl, program, 'iAlpha');

    gl.enableVertexAttribArray(iCenter);
    gl.vertexAttribPointer(iCenter, 2, gl.FLOAT, false, stride, 0);
    gl.vertexAttribDivisor(iCenter, 1);

    gl.enableVertexAttribArray(iSize);
    gl.vertexAttribPointer(iSize, 2, gl.FLOAT, false, stride, 2 * 4);
    gl.vertexAttribDivisor(iSize, 1);

    gl.enableVertexAttribArray(iColor);
    gl.vertexAttribPointer(iColor, 3, gl.FLOAT, false, stride, 4 * 4);
    gl.vertexAttribDivisor(iColor, 1);

    gl.enableVertexAttribArray(iGlow);
    gl.vertexAttribPointer(iGlow, 1, gl.FLOAT, false, stride, 7 * 4);
    gl.vertexAttribDivisor(iGlow, 1);

    gl.enableVertexAttribArray(iIsHold);
    gl.vertexAttribPointer(iIsHold, 1, gl.FLOAT, false, stride, 8 * 4);
    gl.vertexAttribDivisor(iIsHold, 1);

    gl.enableVertexAttribArray(iAlpha);
    gl.vertexAttribPointer(iAlpha, 1, gl.FLOAT, false, stride, 9 * 4);
    gl.vertexAttribDivisor(iAlpha, 1);

    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

    return {
      program,
      vao,
      quad,
      instances,
      uViewport: getUniformLocation(gl, program, 'uViewport'),
    };
  }

  private createParticlesProgram(): ParticlesProgram {
    const gl = this.gl;
    const program = createProgram(gl, PARTICLES_VERTEX, PARTICLES_FRAGMENT);
    const vao = createVertexArray(gl);
    const quad = createBuffer(gl);
    const instances = createBuffer(gl);

    gl.bindVertexArray(vao);

    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -0.5, -0.5,
      0.5, -0.5,
      -0.5, 0.5,
      0.5, 0.5,
    ]), gl.STATIC_DRAW);
    const aQuad = getAttribLocation(gl, program, 'aQuad');
    gl.enableVertexAttribArray(aQuad);
    gl.vertexAttribPointer(aQuad, 2, gl.FLOAT, false, 0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, instances);
    const stride = 10 * 4;
    const iOrigin = getAttribLocation(gl, program, 'iOrigin');
    const iVelocity = getAttribLocation(gl, program, 'iVelocity');
    const iColor = getAttribLocation(gl, program, 'iColor');
    const iBirth = getAttribLocation(gl, program, 'iBirth');
    const iLifetime = getAttribLocation(gl, program, 'iLifetime');
    const iSize = getAttribLocation(gl, program, 'iSize');

    gl.enableVertexAttribArray(iOrigin);
    gl.vertexAttribPointer(iOrigin, 2, gl.FLOAT, false, stride, 0);
    gl.vertexAttribDivisor(iOrigin, 1);

    gl.enableVertexAttribArray(iVelocity);
    gl.vertexAttribPointer(iVelocity, 2, gl.FLOAT, false, stride, 2 * 4);
    gl.vertexAttribDivisor(iVelocity, 1);

    gl.enableVertexAttribArray(iColor);
    gl.vertexAttribPointer(iColor, 3, gl.FLOAT, false, stride, 4 * 4);
    gl.vertexAttribDivisor(iColor, 1);

    gl.enableVertexAttribArray(iBirth);
    gl.vertexAttribPointer(iBirth, 1, gl.FLOAT, false, stride, 7 * 4);
    gl.vertexAttribDivisor(iBirth, 1);

    gl.enableVertexAttribArray(iLifetime);
    gl.vertexAttribPointer(iLifetime, 1, gl.FLOAT, false, stride, 8 * 4);
    gl.vertexAttribDivisor(iLifetime, 1);

    gl.enableVertexAttribArray(iSize);
    gl.vertexAttribPointer(iSize, 1, gl.FLOAT, false, stride, 9 * 4);
    gl.vertexAttribDivisor(iSize, 1);

    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

    return {
      program,
      vao,
      quad,
      instances,
      uViewport: getUniformLocation(gl, program, 'uViewport'),
      uTime: getUniformLocation(gl, program, 'uTime'),
    };
  }

  private createHitLineProgram(): HitLineProgram {
    const gl = this.gl;
    const program = createProgram(gl, HIT_LINE_VERTEX, HIT_LINE_FRAGMENT);
    const vao = createVertexArray(gl);
    const quad = createBuffer(gl);

    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      -1, -1,
      1, -1,
      -1, 1,
      1, 1,
    ]), gl.STATIC_DRAW);

    const aPos = getAttribLocation(gl, program, 'aPos');
    gl.enableVertexAttribArray(aPos);
    gl.vertexAttribPointer(aPos, 2, gl.FLOAT, false, 0, 0);

    gl.bindVertexArray(null);
    gl.bindBuffer(gl.ARRAY_BUFFER, null);

    return {
      program,
      vao,
      quad,
      uViewport: getUniformLocation(gl, program, 'uViewport'),
      uHitLineY: getUniformLocation(gl, program, 'uHitLineY'),
      uHeight: getUniformLocation(gl, program, 'uHeight'),
      uColor: getUniformLocation(gl, program, 'uColor'),
      uPulse: getUniformLocation(gl, program, 'uPulse'),
    };
  }

  private getLayout(chart: PreparedChart, isDark: boolean): LayoutCache {
    const cached = this.layoutCache;
    if (
      cached
      && cached.width === this.width
      && cached.minMidi === chart.minMidi
      && cached.maxMidi === chart.maxMidi
      && cached.isDark === isDark
    ) {
      return cached;
    }
    const next = buildLaneLayout(chart.minMidi, chart.maxMidi, this.width, isDark);
    this.layoutCache = next;
    return next;
  }

  private drawLanes(layout: LayoutCache, frame: RhythmRenderFrame, hitLineY: number): void {
    const gl = this.gl;
    const laneCount = layout.lanes.length;
    if (laneCount === 0) return;

    if (this.laneInstanceData.length < laneCount * 6) {
      this.laneInstanceData = new Float32Array(laneCount * 6);
    }

    for (let index = 0; index < laneCount; index++) {
      const lane = layout.lanes[index];
      const base = index * 6;
      this.laneInstanceData[base] = lane.center;
      this.laneInstanceData[base + 1] = lane.width;
      this.laneInstanceData[base + 2] = lane.color[0];
      this.laneInstanceData[base + 3] = lane.color[1];
      this.laneInstanceData[base + 4] = lane.color[2];
      this.laneInstanceData[base + 5] = lane.black ? 1 : 0;
    }

    gl.useProgram(this.lanesProgram.program);
    gl.bindVertexArray(this.lanesProgram.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lanesProgram.instances);
    gl.bufferData(gl.ARRAY_BUFFER, this.laneInstanceData.subarray(0, laneCount * 6), gl.DYNAMIC_DRAW);
    gl.uniform2f(this.lanesProgram.uViewport, this.width, this.height);
    gl.uniform1f(this.lanesProgram.uHitLineY01, hitLineY / this.height);
    gl.uniform1f(this.lanesProgram.uIntensity, 0.95 + Math.min(frame.combo, 24) * 0.01);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, laneCount);
    gl.bindVertexArray(null);
  }

  private drawNotes(layout: LayoutCache, frame: RhythmRenderFrame, hitLineY: number): void {
    const gl = this.gl;
    const travelHeight = Math.max(1, hitLineY - TOP_PADDING);
    const pxPerSec = travelHeight / LOOK_AHEAD_SEC;
    const startTime = frame.songTime - frame.chart.maxDuration - 0.4;
    const endTime = frame.songTime + LOOK_AHEAD_SEC + 0.45;
    const startIndex = firstVisibleIndex(frame.chart.notes, startTime);
    const endIndex = lastVisibleIndex(frame.chart.notes, endTime);
    let instanceCount = 0;

    for (let noteIndex = startIndex; noteIndex < endIndex; noteIndex++) {
      if (instanceCount >= MAX_NOTE_INSTANCES) break;
      const note = frame.chart.notes[noteIndex];
      const lane = layout.laneByMidi.get(note.midi);
      if (!lane) continue;

      const headY = hitLineY - (note.time - frame.songTime) * pxPerSec;
      const tailY = hitLineY - (note.endTime - frame.songTime) * pxPerSec;
      const height = Math.max(MIN_NOTE_HEIGHT, headY - tailY);
      const centerY = tailY + height * 0.5;
      if (centerY + height * 0.5 < -48 || centerY - height * 0.5 > this.height + 48) continue;

      const judged = frame.judgedIndices.has(note.index);
      const activeHold = frame.activeHoldIndices.has(note.index);
      const holdingLane = frame.heldMidis.has(note.midi);
      const missed = !judged && note.endTime < frame.songTime - 0.03;
      const settled = judged && note.endTime < frame.songTime - 0.03;
      const inWindow = frame.songTime >= note.time - 0.1 && frame.songTime <= note.endTime + 0.12;
      const approaching = note.time > frame.songTime && note.time - frame.songTime <= 0.35;
      const sideInset = lane.black ? BLACK_NOTE_SIDE_INSET : NOTE_SIDE_INSET;
      const width = Math.max(6, lane.width - sideInset * 2);

      let color: [number, number, number] = lane.color;
      let glow = 0.18;
      let alpha = 0.88;

      if (activeHold) {
        color = mixRgb(lane.color, [1, 1, 1], frame.isDark ? 0.22 : 0.16);
        glow = 1;
        alpha = 1;
      } else if (missed) {
        color = frame.isDark ? [0.46, 0.16, 0.2] : [0.78, 0.36, 0.32];
        glow = 0.06;
        alpha = 0.3;
      } else if (settled) {
        color = mixRgb(
          lane.color,
          frame.isDark ? [0.12, 0.14, 0.16] : [0.96, 0.95, 0.94],
          0.72,
        );
        glow = 0.04;
        alpha = 0.26;
      } else if (inWindow) {
        color = mixRgb(lane.color, [1, 1, 1], frame.isDark ? 0.18 : 0.1);
        glow = 0.96;
        alpha = 1;
      } else if (approaching) {
        glow = 0.42;
        alpha = 0.94;
      } else if (holdingLane) {
        glow = 0.58;
        alpha = 0.96;
      }

      const base = instanceCount * 10;
      this.noteInstanceData[base] = lane.center;
      this.noteInstanceData[base + 1] = centerY;
      this.noteInstanceData[base + 2] = width;
      this.noteInstanceData[base + 3] = height;
      this.noteInstanceData[base + 4] = color[0];
      this.noteInstanceData[base + 5] = color[1];
      this.noteInstanceData[base + 6] = color[2];
      this.noteInstanceData[base + 7] = glow;
      this.noteInstanceData[base + 8] = note.duration >= 0.9 ? 1 : 0;
      this.noteInstanceData[base + 9] = alpha;
      instanceCount++;
    }

    if (instanceCount === 0) {
      return;
    }

    gl.useProgram(this.notesProgram.program);
    gl.bindVertexArray(this.notesProgram.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.notesProgram.instances);
    gl.bufferData(gl.ARRAY_BUFFER, this.noteInstanceData.subarray(0, instanceCount * 10), gl.DYNAMIC_DRAW);
    gl.uniform2f(this.notesProgram.uViewport, this.width, this.height);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, instanceCount);
    gl.bindVertexArray(null);
  }

  private drawHitLine(frame: RhythmRenderFrame, hitLineY: number, nowSec: number): void {
    const gl = this.gl;
    const pulseAge = nowSec - this.lastPulseAtSec;
    const pulse = pulseAge < 0.3 ? 1 - pulseAge / 0.3 : 0;
    const baseColor = frame.isDark ? [0.98, 0.9, 0.72] : [0.84, 0.62, 0.2];

    gl.useProgram(this.hitLineProgram.program);
    gl.bindVertexArray(this.hitLineProgram.vao);
    gl.uniform2f(this.hitLineProgram.uViewport, this.width, this.height);
    gl.uniform1f(this.hitLineProgram.uHitLineY, hitLineY);
    gl.uniform1f(this.hitLineProgram.uHeight, 18);
    gl.uniform3f(this.hitLineProgram.uColor, baseColor[0], baseColor[1], baseColor[2]);
    gl.uniform1f(this.hitLineProgram.uPulse, pulse);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.bindVertexArray(null);
  }

  private drawParticles(nowSec: number): void {
    const gl = this.gl;
    const aliveCount = this.particles.collect(
      nowSec,
      this.particleTempOrigin,
      this.particleTempVelocity,
      this.particleTempColor,
      this.particleTempBirth,
      this.particleTempLifetime,
      this.particleTempSize,
    );
    if (aliveCount === 0) return;

    for (let index = 0; index < aliveCount; index++) {
      const base = index * 10;
      this.particleInstanceData[base] = this.particleTempOrigin[index * 2];
      this.particleInstanceData[base + 1] = this.particleTempOrigin[index * 2 + 1];
      this.particleInstanceData[base + 2] = this.particleTempVelocity[index * 2];
      this.particleInstanceData[base + 3] = this.particleTempVelocity[index * 2 + 1];
      this.particleInstanceData[base + 4] = this.particleTempColor[index * 3];
      this.particleInstanceData[base + 5] = this.particleTempColor[index * 3 + 1];
      this.particleInstanceData[base + 6] = this.particleTempColor[index * 3 + 2];
      this.particleInstanceData[base + 7] = this.particleTempBirth[index];
      this.particleInstanceData[base + 8] = this.particleTempLifetime[index];
      this.particleInstanceData[base + 9] = this.particleTempSize[index];
    }

    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE);
    gl.useProgram(this.particlesProgram.program);
    gl.bindVertexArray(this.particlesProgram.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.particlesProgram.instances);
    gl.bufferData(gl.ARRAY_BUFFER, this.particleInstanceData.subarray(0, aliveCount * 10), gl.DYNAMIC_DRAW);
    gl.uniform2f(this.particlesProgram.uViewport, this.width, this.height);
    gl.uniform1f(this.particlesProgram.uTime, nowSec);
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, aliveCount);
    gl.bindVertexArray(null);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
  }

  private emitNewHitBursts(
    layout: LayoutCache,
    frame: RhythmRenderFrame,
    hitLineY: number,
    nowSec: number,
  ): void {
    for (const [midi, pulseId] of frame.latestPulseIdsByMidi.entries()) {
      if (this.latestPulseIdsByMidi.get(midi) === pulseId) continue;
      this.latestPulseIdsByMidi.set(midi, pulseId);

      const lane = layout.laneByMidi.get(midi);
      if (!lane) continue;

      this.lastPulseAtSec = nowSec;
      const color = hitColorForGrade(frame.latestPulseGradesByMidi?.get(midi), lane.color);
      this.particles.emitBurst({
        x: lane.center,
        y: hitLineY,
        color,
        nowSec,
        count: lane.black ? 12 : 16,
        speed: lane.black ? 180 : 210,
        spreadDeg: 96,
        sizePx: lane.black ? 16 : 20,
        lifetime: 0.62,
      });
    }
  }
}
