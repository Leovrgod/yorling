import type { MascotProps } from './types';

/**
 * Cursor mascot — hexagonal gem with faceted faces.
 */
export function CursorMascot({ state, size }: MascotProps) {
  const s = size / 15;
  const y0 = 4;
  const r = (x: number, y: number, w: number, h: number) => ({
    x: x * s, y: (y - y0) * s, width: w * s, height: h * s,
  });

  // Hex vertices (flat-top)
  const cx = 7.5, cy = 10;
  const pts = (coords: [number, number][]) =>
    coords.map(([x, y]) => `${x * s},${(y - y0) * s}`).join(' ');

  const top: [number, number] = [cx, 5.5];
  const topR: [number, number] = [12.5, 7.975];
  const botR: [number, number] = [12.5, 12.025];
  const bot: [number, number] = [cx, 14.5];
  const botL: [number, number] = [2.5, 12.025];
  const topL: [number, number] = [2.5, 7.975];
  const center: [number, number] = [cx, cy];

  return (
    <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
      {/* Shadow */}
      <rect {...r(3, 15, 9, 1)} rx={s * 0.5} fill="black" opacity={0.25} />
      {/* Left dark facet */}
      <polygon points={pts([topL, top, center, botL])} fill="#14120B" />
      {/* Right facet */}
      <polygon points={pts([top, topR, botR, center])} fill="#26251E" />
      {/* Bottom facet */}
      <polygon points={pts([botL, center, botR, bot])} fill="rgb(77,71,61)" />
      {/* Highlight */}
      <polygon points={pts([[8.5, 6], [12, 8.275], [8, 10.5]])} fill="#EDECEC" opacity={0.5} />
      {/* Outline */}
      <polygon
        points={pts([top, topR, botR, bot, botL, topL])}
        fill="none" stroke="#EDECEC" strokeOpacity={0.35}
        strokeWidth={0.5 * s}
      />
      {/* Eyes */}
      {state === 'idle' ? (
        <>
          <rect {...r(5, 9.5, 1.5, 0.7)} rx={s * 0.3} fill="#EDECEC" />
          <rect {...r(8.5, 9.5, 1.5, 0.7)} rx={s * 0.3} fill="#EDECEC" />
        </>
      ) : (
        <>
          <rect {...r(4.5, 9, 1.3, 1.3)} rx={s * 0.4} fill="#EDECEC" />
          <rect {...r(7, 9, 1.3, 1.3)} rx={s * 0.4} fill="#EDECEC" />
        </>
      )}
      {/* Legs */}
      <rect {...r(5.5, 14.5, 1, 1.5)} fill="rgb(77,71,61)" />
      <rect {...r(8.5, 14.5, 1, 1.5)} fill="rgb(77,71,61)" />
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
