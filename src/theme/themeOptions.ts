import type { AppThemeId } from '../types';

export interface ThemeOption {
  id: AppThemeId;
  label: string;
  icon: string;
}

export const THEME_OPTIONS: ThemeOption[] = [
  { id: 'light', label: 'Light', icon: '☀' },
  { id: 'dark', label: 'Dark', icon: '☾' },
];

export const DEFAULT_THEME: AppThemeId = 'light';
export const THEME_STORAGE_KEY = 'yorling.theme';

/** Migration map: old theme ids → new ids. */
const LEGACY_THEME_MAP: Record<string, AppThemeId> = {
  graphite: 'dark',
  aurora: 'dark',
  dawn: 'light',
};

export function isThemeId(value: string | null | undefined): value is AppThemeId {
  return THEME_OPTIONS.some((theme) => theme.id === value);
}

export function getStoredTheme(storage?: Pick<Storage, 'getItem' | 'setItem'> | null): AppThemeId {
  if (!storage) {
    return DEFAULT_THEME;
  }

  const storedTheme = storage.getItem(THEME_STORAGE_KEY);

  if (isThemeId(storedTheme)) {
    return storedTheme;
  }

  // Migrate legacy theme ids from previous versions
  if (storedTheme && storedTheme in LEGACY_THEME_MAP) {
    const migrated = LEGACY_THEME_MAP[storedTheme];
    storage.setItem?.(THEME_STORAGE_KEY, migrated);
    return migrated;
  }

  return DEFAULT_THEME;
}

export function persistTheme(theme: AppThemeId, storage?: Pick<Storage, 'setItem'> | null): void {
  storage?.setItem(THEME_STORAGE_KEY, theme);
}
