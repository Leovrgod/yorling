import type { MascotProps } from './types';

/**
 * Codex "Dex" mascot — cloud blob with terminal prompt face.
 */
export function CodexMascot({ state, size }: MascotProps) {
  const s = size / 15;
  const y0 = 4;
  const r = (x: number, y: number, w: number, h: number) => ({
    x: x * s, y: (y - y0) * s, width: w * s, height: h * s,
  });

  const cloud = 'rgb(235,235,237)';
  const cloudDk = 'rgb(179,179,184)';
  const prompt = '#000000';

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      {/* Shadow */}
      <rect {...r(3, 15, 9, 1)} rx={s * 0.5} fill="black" opacity={0.25} />
      {/* Cloud body */}
      <rect {...r(4, 5, 7, 1)} rx={s * 0.5} fill={cloud} />
      <rect {...r(3, 6, 9, 1)} fill={cloud} />
      <rect {...r(2, 7, 11, 2)} fill={cloud} />
      <rect {...r(1, 9, 13, 4)} fill={cloud} />
      <rect {...r(2, 13, 11, 1)} fill={cloud} />
      <rect {...r(3, 14, 9, 1)} rx={s * 0.5} fill={cloud} />
      {/* Cloud bumps */}
      <rect {...r(3, 5.5, 3, 1.5)} rx={s * 0.7} fill={cloud} />
      <rect {...r(6, 5, 3, 2)} rx={s * 0.7} fill={cloud} />
      <rect {...r(9, 5.5, 3, 1.5)} rx={s * 0.7} fill={cloud} />
      {/* Legs */}
      <rect {...r(5, 14.5, 1, 1.5)} fill={cloudDk} />
      <rect {...r(9, 14.5, 1, 1.5)} fill={cloudDk} />
      {/* Terminal prompt face >_ */}
      {state !== 'idle' ? (
        <>
          <rect {...r(3, 10, 1, 1)} fill={prompt} />
          <rect {...r(4, 11, 1, 1)} fill={prompt} />
          <rect {...r(3, 12, 1, 1)} fill={prompt} />
          <rect {...r(6, 12, 3, 1)} fill={prompt}>
            {state === 'working' && (
              <animate attributeName="opacity" values="1;0;1" dur="1s" repeatCount="indefinite" />
            )}
          </rect>
        </>
      ) : (
        <>
          {/* Sleeping: horizontal eye lines */}
          <rect {...r(3, 11, 3, 0.8)} rx={s * 0.3} fill={prompt} />
          <rect {...r(9, 11, 3, 0.8)} rx={s * 0.3} fill={prompt} />
        </>
      )}
      {/* Alert */}
      {state === 'alert' && (
        <g className="mascot-bounce">
          <rect {...r(13.5, 4, 1.5, 3)} rx={s * 0.3} fill="rgb(255,140,0)" />
          <rect {...r(13.5, 7.5, 1.5, 1.2)} rx={s * 0.3} fill="rgb(255,140,0)" />
        </g>
      )}
      {/* Sleep Z's */}
      {state === 'idle' && (
        <g className="mascot-float" opacity={0.5}>
          <text x={12 * s} y={5 * s} fontSize={s * 2} fill="white" fontFamily="monospace">z</text>
        </g>
      )}
    </svg>
  );
}
