import type { MascotProps } from './types';

/**
 * Claude "Clawd" mascot — warm terracotta blob with 4 legs.
 */
export function ClaudeMascot({ state, size }: MascotProps) {
  const s = size / 15;
  const y0 = 4;
  const r = (x: number, y: number, w: number, h: number) => ({
    x: x * s, y: (y - y0) * s, width: w * s, height: h * s,
  });

  const body = '#DE886D';
  const eye = '#000000';
  const alert = '#FF3D00';

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      {/* Shadow */}
      <rect {...r(2, 15, 11, 1)} rx={s * 0.5} fill="black" opacity={0.3} />
      {/* Legs */}
      <rect {...r(3, 13, 1, 2)} fill={body} />
      <rect {...r(5, 13, 1, 2)} fill={body} />
      <rect {...r(9, 13, 1, 2)} fill={body} />
      <rect {...r(11, 13, 1, 2)} fill={body} />
      {/* Torso */}
      <rect {...r(2, 6, 11, 7)} rx={s * 1.5} fill={body} />
      {/* Eyes */}
      {state === 'idle' ? (
        <>
          <rect {...r(4, 10, 2, 0.8)} rx={s * 0.3} fill={eye} />
          <rect {...r(9, 10, 2, 0.8)} rx={s * 0.3} fill={eye} />
        </>
      ) : (
        <>
          <rect {...r(4.5, 8.5, 1, 2)} rx={s * 0.3} fill={eye} />
          <rect {...r(9.5, 8.5, 1, 2)} rx={s * 0.3} fill={eye} />
        </>
      )}
      {/* Arms */}
      {state === 'working' ? (
        <>
          <rect {...r(0, 9, 2, 2)} rx={s * 0.5} fill={body}>
            <animateTransform attributeName="transform" type="rotate"
              values="-10,{1*s},{10*s};-45,{1*s},{10*s};-10,{1*s},{10*s}"
              dur="0.6s" repeatCount="indefinite" />
          </rect>
          <rect {...r(13, 9, 2, 2)} rx={s * 0.5} fill={body}>
            <animateTransform attributeName="transform" type="rotate"
              values="10,{14*s},{10*s};45,{14*s},{10*s};10,{14*s},{10*s}"
              dur="0.6s" repeatCount="indefinite" />
          </rect>
        </>
      ) : (
        <>
          <rect {...r(0, 10, 2, 2)} rx={s * 0.5} fill={body} />
          <rect {...r(13, 10, 2, 2)} rx={s * 0.5} fill={body} />
        </>
      )}
      {/* Alert exclamation */}
      {state === 'alert' && (
        <g className="mascot-bounce">
          <rect {...r(13.5, 4, 1.5, 3)} rx={s * 0.3} fill={alert} />
          <rect {...r(13.5, 7.5, 1.5, 1.2)} rx={s * 0.3} fill={alert} />
        </g>
      )}
      {/* Sleep Z's */}
      {state === 'idle' && (
        <g className="mascot-float" opacity={0.5}>
          <text x={12 * s} y={5 * s} fontSize={s * 2} fill="white" fontFamily="monospace">z</text>
          <text x={13 * s} y={3 * s} fontSize={s * 1.5} fill="white" fontFamily="monospace" opacity={0.6}>z</text>
        </g>
      )}
    </svg>
  );
}
