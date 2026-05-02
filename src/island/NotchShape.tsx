import { forwardRef, type CSSProperties, type MouseEventHandler, type ReactNode } from 'react';
import type { IslandSurfaceMetrics } from './notchGeometry';

type IslandActivityLevel = 'idle' | 'busy' | 'attention';

interface NotchShapeProps {
  surfaceMetrics: IslandSurfaceMetrics;
  isAnimating: boolean;
  isBouncing?: boolean;
  activityLevel: IslandActivityLevel;
  hasActivity: boolean;
  placementMode?: 'notch' | 'top_bar';
  children: ReactNode;
  onMouseEnter?: () => void;
  onMouseLeave?: () => void;
  onMouseDownCapture?: MouseEventHandler<HTMLDivElement>;
}

/**
 * The pill/notch surface with inverted (concave) top corners and convex bottom
 * corners, matching the real Dynamic Island shape. Driven by spring progress
 * from the animation hook and screen geometry from the native island window.
 */
export const NotchShape = forwardRef<HTMLDivElement, NotchShapeProps>(function NotchShape(
  {
    surfaceMetrics,
    isAnimating,
    isBouncing,
    activityLevel,
    hasActivity,
    placementMode = 'top_bar',
    children,
    onMouseEnter,
    onMouseLeave,
    onMouseDownCapture,
  },
  ref,
) {
  const metrics = surfaceMetrics;
  const clampedProgress = metrics.progress;
  const isExpanded = clampedProgress > 0.02;
  const width = metrics.width;
  const height = metrics.height;
  const topRadius = metrics.topRadius;
  const bottomRadius = metrics.bottomRadius;
  const clipPath = createNotchClipPath(width, height, topRadius, bottomRadius);

  return (
    <div
      ref={ref}
      className={
        'island-notch' +
        ` island-notch--${activityLevel}` +
        ` island-notch--${placementMode}` +
        (isExpanded ? ' island-notch--expanded' : '') +
        (isAnimating ? ' island-notch--animating' : '') +
        (hasActivity ? ' island-notch--active' : '') +
        (isBouncing ? ' island-notch--bouncing' : '')
      }
      style={
        {
          width: `${width}px`,
          height: `${height}px`,
          clipPath,
          '--island-top-radius': `${Math.min(topRadius, height / 2)}px`,
          '--island-bottom-radius': `${Math.min(bottomRadius, height / 2)}px`,
        } as CSSProperties
      }
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      onMouseDownCapture={onMouseDownCapture}
    >
      <div className="island-notch__inner">{children}</div>
    </div>
  );
});

function createNotchClipPath(
  width: number,
  height: number,
  topRadius: number,
  bottomRadius: number,
) {
  return `path('${generateNotchPath(width, height, topRadius, bottomRadius)}')`;
}

/**
 * Generates an SVG path with inverted (concave) top corners and standard
 * convex bottom corners.
 *
 * Shape anatomy:
 * - Top edge spans full width (0 to width)
 * - Top-left corner curves concavely inward from (0,0) to (topR, topR)
 * - Left side runs vertically at x = topR
 * - Bottom-left corner curves convexly from (topR, h-botR) to (topR+botR, h)
 * - Bottom edge
 * - Bottom-right corner curves convexly
 * - Right side runs vertically at x = width-topR
 * - Top-right corner curves concavely inward from (width-topR, topR) to (width, 0)
 */
export function generateNotchPath(
  width: number,
  height: number,
  requestedTopRadius: number,
  requestedBottomRadius: number,
) {
  const topR = Math.max(0, Math.min(requestedTopRadius, width / 4, height / 4));
  const botR = Math.max(0, Math.min(requestedBottomRadius, width / 4, height / 2));

  const bottom = height;
  const right = width;

  return [
    // Start at top-left corner
    moveTo(0, 0),
    // Top-left concave curve: curves inward from (0,0) to (topR, topR)
    // Control point at (topR, 0) pulls the curve inward (concave)
    quadTo(topR, 0, topR, topR),
    // Left side down to bottom-left corner
    lineTo(topR, bottom - botR),
    // Bottom-left convex curve
    quadTo(topR, bottom, topR + botR, bottom),
    // Bottom edge
    lineTo(right - topR - botR, bottom),
    // Bottom-right convex curve
    quadTo(right - topR, bottom, right - topR, bottom - botR),
    // Right side up to top-right corner
    lineTo(right - topR, topR),
    // Top-right concave curve: curves inward from (right-topR, topR) to (right, 0)
    // Control point at (right-topR, 0) pulls the curve inward (concave)
    quadTo(right - topR, 0, right, 0),
    // Top edge back to start
    'Z',
  ].join(' ');
}

function moveTo(x: number, y: number) {
  return `M ${fmt(x)} ${fmt(y)}`;
}

function lineTo(x: number, y: number) {
  return `L ${fmt(x)} ${fmt(y)}`;
}

function quadTo(cx: number, cy: number, x: number, y: number) {
  return `Q ${fmt(cx)} ${fmt(cy)} ${fmt(x)} ${fmt(y)}`;
}

function fmt(value: number) {
  return Number(value.toFixed(3)).toString();
}
