import type { MascotProps } from './types';

/**
 * Gemini mascot — 4-pointed sparkle star with gradient fill.
 */
export function GeminiMascot({ state, size }: MascotProps) {
  const s = size / 15;
  const y0 = 4;
  const cx = 7.5 * s;
  const cy = (10 - y0) * s;
  const outer = 4.5 * s;
  const inner = 1.8 * s;

  // 8 vertices: alternating outer/inner
  const points: string[] = [];
  for (let i = 0; i < 8; i++) {
    const angle = (i * 45 - 90) * (Math.PI / 180);
    const rad = i % 2 === 0 ? outer : inner;
    points.push(`${cx + rad * Math.cos(angle)},${cy + rad * Math.sin(angle)}`);
  }

  const r = (x: number, y: number, w: number, h: number) => ({
    x: x * s, y: (y - y0) * s, width: w * s, height: h * s,
  });

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      <defs>
        <linearGradient id="gemini-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#4796E4" />
          <stop offset="50%" stopColor="#847ACE" />
          <stop offset="100%" stopColor="#C3677F" />
        </linearGradient>
      </defs>
      {/* Shadow */}
      <rect {...r(3, 15, 9, 1)} rx={s * 0.5} fill="black" opacity={0.25} />
      {/* Star body */}
      <polygon
        points={points.join(' ')}
        fill="url(#gemini-grad)"
        className={state === 'working' ? 'mascot-spin' : undefined}
      />
      {/* Eyes */}
      {state === 'idle' ? (
        <>
          <rect {...r(5.5, 9.5, 1.5, 0.7)} rx={s * 0.3} fill="white" />
          <rect {...r(8.3, 9.5, 1.5, 0.7)} rx={s * 0.3} fill="white" />
        </>
      ) : (
        <>
          <rect {...r(5.8, 9, 1.2, 1.5)} rx={s * 0.4} fill="white" />
          <rect {...r(8.3, 9, 1.2, 1.5)} rx={s * 0.4} fill="white" />
        </>
      )}
      {/* Legs */}
      <rect {...r(5.5, 14, 1, 2)} fill="#847ACE" opacity={0.7} />
      <rect {...r(8.5, 14, 1, 2)} fill="#847ACE" opacity={0.7} />
      {/* Alert */}
      {state === 'alert' && (
        <g className="mascot-bounce">
          <rect {...r(13.5, 4, 1.5, 3)} rx={s * 0.3} fill="#FF3D00" />
          <rect {...r(13.5, 7.5, 1.5, 1.2)} rx={s * 0.3} fill="#FF3D00" />
        </g>
      )}
      {/* Sleep Z's */}
      {state === 'idle' && (
        <g className="mascot-float" opacity={0.5}>
          <text x={12 * s} y={4 * s} fontSize={s * 2} fill="white" fontFamily="monospace">z</text>
        </g>
      )}
    </svg>
  );
}
