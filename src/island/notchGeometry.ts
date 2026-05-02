import type { IslandScreenInfo } from '../types';

export type IslandSurfaceOpenReason =
  | 'click'
  | 'hover'
  | 'attention'
  | 'shortcut'
  | 'boot'
  | null;

export type IslandPanelMode = 'sessions' | 'chatview' | 'rules';

export const DEFAULT_MAX_PANEL_HEIGHT = 2000;
export const ISLAND_EXPANDED_LIST_MAX_WIDTH = 520;
export const ISLAND_EXPANDED_WIDE_MAX_WIDTH = 680;
export const ISLAND_FALLBACK_CLOSED_WIDTH = 266;
export const ISLAND_FALLBACK_CLOSED_HEIGHT = 32;
export const CLOSED_TOP_RADIUS = 6;
export const CLOSED_BOTTOM_RADIUS = 14;
export const OPEN_TOP_RADIUS = 19;
export const OPEN_BOTTOM_RADIUS = 24;
const HORIZONTAL_SCREEN_MARGIN = 64;
const VERTICAL_SCREEN_MARGIN = 48;
const DEFAULT_FALLBACK_HEIGHT = 200;
const DEFAULT_CHATVIEW_HEIGHT = 340;
const MIN_OPENED_HEIGHT_DELTA = 24;
const EXPANDED_CONTENT_BUFFER = 0;

interface IslandSurfaceMetricsInput {
  progress: number;
  sessionCount: number;
  screenInfo: IslandScreenInfo | null;
  openReason: IslandSurfaceOpenReason;
  panelMode: IslandPanelMode;
  measuredExpandedHeight: number | null;
}

interface IslandOpenedSizeInput {
  screenInfo: IslandScreenInfo | null;
  openReason: IslandSurfaceOpenReason;
  panelMode: IslandPanelMode;
  measuredExpandedHeight: number | null;
}

export interface IslandSurfaceMetrics {
  progress: number;
  width: number;
  height: number;
  topRadius: number;
  bottomRadius: number;
}

export function getIslandSurfaceMetrics({
  progress,
  screenInfo,
  openReason,
  panelMode,
  measuredExpandedHeight,
}: IslandSurfaceMetricsInput): IslandSurfaceMetrics {
  const clampedProgress = clamp(progress, 0, 1);
  const closedSize = getClosedIslandSize(screenInfo);
  const openedSize = getOpenedIslandSize({
    screenInfo,
    openReason,
    panelMode,
    measuredExpandedHeight,
  });

  return {
    progress: clampedProgress,
    width: lerp(closedSize.width, openedSize.width, clampedProgress),
    height: lerp(closedSize.height, openedSize.height, clampedProgress),
    topRadius: lerp(CLOSED_TOP_RADIUS, OPEN_TOP_RADIUS, clampedProgress),
    bottomRadius: lerp(CLOSED_BOTTOM_RADIUS, OPEN_BOTTOM_RADIUS, clampedProgress),
  };
}

export function getClosedIslandSize(screenInfo: IslandScreenInfo | null) {
  return {
    width: Math.max(
      ISLAND_FALLBACK_CLOSED_WIDTH,
      Math.round(screenInfo?.closed_width ?? ISLAND_FALLBACK_CLOSED_WIDTH),
    ),
    height: Math.max(
      ISLAND_FALLBACK_CLOSED_HEIGHT,
      Math.round(
        screenInfo?.closed_height ??
          screenInfo?.top_inset ??
          ISLAND_FALLBACK_CLOSED_HEIGHT,
      ),
    ),
  };
}

export function getOpenedIslandSize({
  screenInfo,
  openReason,
  panelMode,
  measuredExpandedHeight,
}: IslandOpenedSizeInput) {
  const closedSize = getClosedIslandSize(screenInfo);
  const screenWidth = screenInfo?.screen_width ?? 1440;
  const screenHeight = screenInfo?.screen_height ?? 900;
  const maximumOpenedHeight = Math.max(
    closedSize.height + MIN_OPENED_HEIGHT_DELTA,
    Math.min(screenHeight - VERTICAL_SCREEN_MARGIN, DEFAULT_MAX_PANEL_HEIGHT),
  );
  const wideWidth = Math.min(
    Math.max(closedSize.width + 80, screenWidth - HORIZONTAL_SCREEN_MARGIN),
    ISLAND_EXPANDED_WIDE_MAX_WIDTH,
  );
  const listWidth = Math.min(
    Math.max(closedSize.width + 80, screenWidth * 0.4),
    ISLAND_EXPANDED_LIST_MAX_WIDTH,
  );

  if (panelMode === 'chatview') {
    const resolvedMeasuredHeight =
      measuredExpandedHeight !== null
      && measuredExpandedHeight >= closedSize.height + 80
        ? measuredExpandedHeight
        : null;
    const resolvedHeight = Math.min(
      maximumOpenedHeight,
      Math.max(
        closedSize.height + MIN_OPENED_HEIGHT_DELTA,
        (resolvedMeasuredHeight ?? DEFAULT_CHATVIEW_HEIGHT) + EXPANDED_CONTENT_BUFFER,
      ),
    );

    return {
      width: Math.round(wideWidth),
      height: Math.round(resolvedHeight),
    };
  }

  const fallbackHeight = DEFAULT_FALLBACK_HEIGHT;
  const resolvedMeasuredHeight =
    measuredExpandedHeight !== null && measuredExpandedHeight >= fallbackHeight
      ? measuredExpandedHeight
      : null;
  const resolvedHeight = Math.min(
    maximumOpenedHeight,
    Math.max(
      closedSize.height + MIN_OPENED_HEIGHT_DELTA,
      (resolvedMeasuredHeight ?? fallbackHeight) + EXPANDED_CONTENT_BUFFER,
    ),
  );
  const shouldUseWideSessionsWidth =
    openReason === 'hover' ||
    openReason === 'attention' ||
    openReason === 'click' ||
    openReason === 'shortcut';

  return {
    width: Math.round(shouldUseWideSessionsWidth ? wideWidth : listWidth),
    height: Math.round(resolvedHeight),
  };
}

export function lerp(start: number, end: number, progress: number) {
  return start + (end - start) * progress;
}

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}
