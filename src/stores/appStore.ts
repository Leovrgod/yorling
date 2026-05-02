import { create } from 'zustand';
import { DEFAULT_THEME, getStoredTheme, persistTheme } from '../theme/themeOptions';
import {
  DEFAULT_LANGUAGE,
  getStoredLanguage,
  persistLanguage,
} from '../i18n/copy';
import type { AppLanguageId, AppThemeId, ModuleName } from '../types';
import type { KeyboardLayerId } from '../components/keyboard/keyboardUiModel';

const SIDEBAR_COLLAPSED_KEY = 'yorling.sidebarCollapsed';
const SIDEBAR_WIDTH_KEY = 'yorling.sidebarWidth';
const DISABLED_LAYERS_KEY = 'yorling.disabledLayers';
const DISABLED_RULES_KEY = 'yorling.disabledRules';
const DEFAULT_SIDEBAR_WIDTH = 200;

interface AppStoreState {
  activeModule: ModuleName;
  theme: AppThemeId;
  language: AppLanguageId;
  sidebarCollapsed: boolean;
  sidebarWidth: number;
  disabledLayers: KeyboardLayerId[];
  disabledRules: string[];
  setActiveModule: (module: ModuleName) => void;
  setTheme: (theme: AppThemeId) => void;
  setLanguage: (language: AppLanguageId) => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  setSidebarWidth: (width: number, persist?: boolean) => void;
  toggleLayer: (layerId: KeyboardLayerId) => void;
  toggleRule: (ruleId: string) => void;
  setRulesDisabled: (ruleIds: string[], disabled: boolean) => void;
}

function readInitialTheme(): AppThemeId {
  if (typeof window === 'undefined') {
    return DEFAULT_THEME;
  }

  return getStoredTheme(window.localStorage);
}

function readInitialLanguage(): AppLanguageId {
  if (typeof window === 'undefined') {
    return DEFAULT_LANGUAGE;
  }

  return getStoredLanguage(window.localStorage);
}

function readBoolean(key: string, fallback: boolean): boolean {
  if (typeof window === 'undefined') return fallback;
  const v = window.localStorage.getItem(key);
  return v === null ? fallback : v === 'true';
}

function readStringArray(key: string): string[] {
  if (typeof window === 'undefined') return [];
  try {
    const v = window.localStorage.getItem(key);
    return v ? JSON.parse(v) : [];
  } catch {
    return [];
  }
}

function readNumber(key: string, fallback: number): number {
  if (typeof window === 'undefined') return fallback;

  const value = Number(window.localStorage.getItem(key));
  return Number.isFinite(value) ? value : fallback;
}

function persistJson(key: string, value: unknown): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(key, JSON.stringify(value));
  }
}

export const useAppStore = create<AppStoreState>((set, get) => ({
  activeModule: 'keyboard',
  theme: readInitialTheme(),
  language: readInitialLanguage(),
  sidebarCollapsed: readBoolean(SIDEBAR_COLLAPSED_KEY, false),
  sidebarWidth: readNumber(SIDEBAR_WIDTH_KEY, DEFAULT_SIDEBAR_WIDTH),
  disabledLayers: readStringArray(DISABLED_LAYERS_KEY) as KeyboardLayerId[],
  disabledRules: readStringArray(DISABLED_RULES_KEY),
  setActiveModule: (module) => set({ activeModule: module }),
  setTheme: (theme) => {
    if (typeof window !== 'undefined') {
      persistTheme(theme, window.localStorage);
    }

    set({ theme });
  },
  setLanguage: (language) => {
    if (typeof window !== 'undefined') {
      persistLanguage(language, window.localStorage);
    }

    set({ language });
  },
  setSidebarCollapsed: (collapsed) => {
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(collapsed));
    }
    set({ sidebarCollapsed: collapsed });
  },
  setSidebarWidth: (width, persist = true) => {
    if (persist && typeof window !== 'undefined') {
      window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(width));
    }
    set({ sidebarWidth: width });
  },
  toggleLayer: (layerId) => {
    const current = get().disabledLayers;
    const next = current.includes(layerId)
      ? current.filter((id) => id !== layerId)
      : [...current, layerId];
    persistJson(DISABLED_LAYERS_KEY, next);
    set({ disabledLayers: next });
  },
  toggleRule: (ruleId) => {
    const current = get().disabledRules;
    const next = current.includes(ruleId)
      ? current.filter((id) => id !== ruleId)
      : [...current, ruleId];
    persistJson(DISABLED_RULES_KEY, next);
    set({ disabledRules: next });
  },
  setRulesDisabled: (ruleIds, disabled) => {
    const scopedRuleIds = new Set(ruleIds);
    const next = disabled
      ? Array.from(new Set([...get().disabledRules.filter((id) => !scopedRuleIds.has(id)), ...ruleIds]))
      : get().disabledRules.filter((id) => !scopedRuleIds.has(id));

    persistJson(DISABLED_RULES_KEY, next);
    set({ disabledRules: next });
  },
}));
