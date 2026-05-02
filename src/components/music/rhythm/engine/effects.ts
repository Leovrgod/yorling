/**
 * Hit-feedback effects: particle burst pool used by the WebGL renderer.
 *
 * No DOM dependency; lifecycle is driven by `tick(timeSec)` per frame.
 */

export interface ParticleSnapshot {
  origin: [number, number];
  velocity: [number, number];
  color: [number, number, number];
  birth: number;
  lifetime: number;
  size: number;
}

const PARTICLE_CAP = 512;

export class ParticlePool {
  private origin = new Float32Array(PARTICLE_CAP * 2);
  private velocity = new Float32Array(PARTICLE_CAP * 2);
  private color = new Float32Array(PARTICLE_CAP * 3);
  private birth = new Float32Array(PARTICLE_CAP);
  private lifetime = new Float32Array(PARTICLE_CAP);
  private size = new Float32Array(PARTICLE_CAP);
  private slot = 0;
  private filled = 0;

  emitBurst(opts: {
    x: number;
    y: number;
    color: [number, number, number];
    nowSec: number;
    count: number;
    speed: number;
    spreadDeg?: number;
    sizePx?: number;
    lifetime?: number;
  }): void {
    const { x, y, color, nowSec, count, speed } = opts;
    const spread = (opts.spreadDeg ?? 360) * Math.PI / 180;
    const baseAng = -Math.PI / 2; // upward
    for (let i = 0; i < count; i++) {
      const idx = this.slot;
      this.slot = (this.slot + 1) % PARTICLE_CAP;
      if (this.filled < PARTICLE_CAP) this.filled++;
      const ang = baseAng + (Math.random() - 0.5) * spread;
      const sp = speed * (0.55 + Math.random() * 0.6);
      this.origin[idx * 2] = x;
      this.origin[idx * 2 + 1] = y;
      this.velocity[idx * 2] = Math.cos(ang) * sp;
      this.velocity[idx * 2 + 1] = Math.sin(ang) * sp;
      this.color[idx * 3] = color[0];
      this.color[idx * 3 + 1] = color[1];
      this.color[idx * 3 + 2] = color[2];
      this.birth[idx] = nowSec;
      this.lifetime[idx] = opts.lifetime ?? 0.7;
      this.size[idx] = opts.sizePx ?? 18;
    }
  }

  clear(): void {
    this.slot = 0;
    this.filled = 0;
  }

  size_(): number { return this.filled; }

  /** Pack alive particles into typed arrays for instancing.
   *  Returns the count of alive instances. */
  collect(
    nowSec: number,
    outOrigin: Float32Array,
    outVelocity: Float32Array,
    outColor: Float32Array,
    outBirth: Float32Array,
    outLifetime: Float32Array,
    outSize: Float32Array,
  ): number {
    let n = 0;
    for (let i = 0; i < this.filled; i++) {
      const age = nowSec - this.birth[i];
      if (age >= this.lifetime[i]) continue;
      outOrigin[n * 2] = this.origin[i * 2];
      outOrigin[n * 2 + 1] = this.origin[i * 2 + 1];
      outVelocity[n * 2] = this.velocity[i * 2];
      outVelocity[n * 2 + 1] = this.velocity[i * 2 + 1];
      outColor[n * 3] = this.color[i * 3];
      outColor[n * 3 + 1] = this.color[i * 3 + 1];
      outColor[n * 3 + 2] = this.color[i * 3 + 2];
      outBirth[n] = this.birth[i];
      outLifetime[n] = this.lifetime[i];
      outSize[n] = this.size[i];
      n++;
    }
    return n;
  }

  capacity(): number { return PARTICLE_CAP; }
}
