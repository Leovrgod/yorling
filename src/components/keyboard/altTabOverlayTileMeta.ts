import type { AltTabWindow } from '../../types';

const MIN_TITLE_BAR_HEIGHT = 22;
const MAX_TITLE_BAR_HEIGHT = 30;
const TITLE_BAR_HEIGHT_RATIO = 0.11;
const MIN_TITLE_BAR_FONT_SIZE = 12;
const MAX_TITLE_BAR_FONT_SIZE = 16;
const TITLE_BAR_FONT_SIZE_RATIO = 0.55;

export function getAltTabAppLabel(windowInfo: Pick<AltTabWindow, 'app_name' | 'title'>): string {
  const appName = windowInfo.app_name.trim();
  if (appName) {
    return appName;
  }

  const title = windowInfo.title.trim();
  if (title) {
    return title;
  }

  return 'Window';
}

export function getAltTabTitleBarHeight(tileHeight: number): number {
  if (!Number.isFinite(tileHeight) || tileHeight <= 0) {
    return MIN_TITLE_BAR_HEIGHT;
  }

  return Math.max(
    MIN_TITLE_BAR_HEIGHT,
    Math.min(MAX_TITLE_BAR_HEIGHT, Math.round(tileHeight * TITLE_BAR_HEIGHT_RATIO)),
  );
}

export function getAltTabTitleBarFontSize(tileHeight: number): number {
  const titleBarHeight = getAltTabTitleBarHeight(tileHeight);

  return Math.max(
    MIN_TITLE_BAR_FONT_SIZE,
    Math.min(MAX_TITLE_BAR_FONT_SIZE, Math.round(titleBarHeight * TITLE_BAR_FONT_SIZE_RATIO)),
  );
}
