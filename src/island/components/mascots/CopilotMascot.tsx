import type { MascotProps } from './types';

/**
 * GitHub Copilot mascot — robot head with ear loops and rose frame.
 */
export function CopilotMascot({ state, size }: MascotProps) {
  const s = size / 15;
  const y0 = 4;
  const r = (x: number, y: number, w: number, h: number) => ({
    x: x * s, y: (y - y0) * s, width: w * s, height: h * s,
  });

  const ear = '#333333';
  const body = '#CC3366';
  const face = 'rgb(33,33,41)';
  const eyeC = '#FFD700';

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      {/* Shadow */}
      <rect {...r(3, 15.5, 9, 1)} rx={s * 0.5} fill="black" opacity={0.25} />
      {/* Left ear loop */}
      <rect {...r(3, 5, 3, 1)} fill={ear} />
      <rect {...r(3, 6, 1, 1)} fill={ear} />
      <rect {...r(5, 6, 1, 1)} fill={ear} />
      <rect {...r(3, 7, 3, 1)} fill={ear} />
      {/* Right ear loop */}
      <rect {...r(9, 5, 3, 1)} fill={ear} />
      <rect {...r(9, 6, 1, 1)} fill={ear} />
      <rect {...r(11, 6, 1, 1)} fill={ear} />
      <rect {...r(9, 7, 3, 1)} fill={ear} />
      {/* Ear stems */}
      <rect {...r(4, 8, 1, 1)} fill={ear} />
      <rect {...r(10, 8, 1, 1)} fill={ear} />
      {/* Body frame */}
      <rect {...r(2, 9, 11, 1)} fill={body} />
      <rect {...r(2, 10, 2, 3)} fill={body} />
      <rect {...r(11, 10, 2, 3)} fill={body} />
      <rect {...r(2, 13, 11, 1)} fill={body} />
      <rect {...r(4, 14, 7, 1)} fill={body} />
      {/* Face screen */}
      <rect {...r(4, 10, 7, 3)} fill={face} />
      {/* Eyes */}
      {state === 'idle' ? (
        <>
          <rect {...r(5, 11, 2, 0.7)} rx={s * 0.3} fill={eyeC} />
          <rect {...r(8, 11, 2, 0.7)} rx={s * 0.3} fill={eyeC} />
        </>
      ) : (
        <>
          <rect {...r(5.5, 10.5, 1, 1)} rx={s * 0.3} fill={eyeC} />
          <rect {...r(8.5, 10.5, 1, 1)} rx={s * 0.3} fill={eyeC} />
        </>
      )}
      {/* Legs */}
      <rect {...r(6, 14.5, 1, 1.5)} fill={body} opacity={0.6} />
      <rect {...r(8, 14.5, 1, 1.5)} fill={body} opacity={0.6} />
      {/* Alert */}
      {state === 'alert' && (
        <g className="mascot-bounce">
          <rect {...r(13.5, 4, 1.5, 3)} rx={s * 0.3} fill="#FE4C25" />
          <rect {...r(13.5, 7.5, 1.5, 1.2)} rx={s * 0.3} fill="#FE4C25" />
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
