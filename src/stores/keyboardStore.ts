import { create } from 'zustand';
import type { EngineStatus, MappingRule } from '../types';
import { defaultRules } from '../components/keyboard/keyboardMappingData.ts';

const KEYBOARD_AUTO_START_STORAGE_KEY = 'yorling.keyboardAutoStart';
const KEYBOARD_AUTO_ENABLE_STORAGE_KEY = 'yorling.keyboardAutoEnable';

export function parseStoredKeyboardAutoStart(raw: string | null): boolean {
  return raw === 'true';
}

export function parseStoredKeyboardAutoEnable(raw: string | null): boolean {
  return raw === null || raw.trim() === '' ? true : raw === 'true';
}

function readStoredKeyboardAutoStart(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  return parseStoredKeyboardAutoStart(
    window.localStorage.getItem(KEYBOARD_AUTO_START_STORAGE_KEY),
  );
}

function readStoredKeyboardAutoEnable(): boolean {
  if (typeof window === 'undefined') {
    return true;
  }

  return parseStoredKeyboardAutoEnable(
    window.localStorage.getItem(KEYBOARD_AUTO_ENABLE_STORAGE_KEY),
  );
}

function persistBoolean(key: string, value: boolean): void {
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(key, String(value));
  }
}

interface KeyboardState {
  status: EngineStatus;
  rules: MappingRule[];
  error: string | null;
  autoStart: boolean;
  autoEnable: boolean;
  setStatus: (status: EngineStatus) => void;
  setRules: (rules: MappingRule[]) => void;
  setError: (error: string | null) => void;
  clearError: () => void;
  setAutoStart: (autoStart: boolean) => void;
  setAutoEnable: (autoEnable: boolean) => void;
}

export const useKeyboardStore = create<KeyboardState>((set) => ({
  status: {
    running: false,
    enabled: false,
    event_count: 0,
    has_accessibility: false,
    has_screen_recording: false,
  },
  rules: defaultRules,
  error: null,
  autoStart: readStoredKeyboardAutoStart(),
  autoEnable: readStoredKeyboardAutoEnable(),
  setStatus: (status) => set({ status }),
  setRules: (rules) => set({ rules }),
  setError: (error) => set({ error }),
  clearError: () => set({ error: null }),
  setAutoStart: (autoStart) => {
    persistBoolean(KEYBOARD_AUTO_START_STORAGE_KEY, autoStart);
    set({ autoStart });
  },
  setAutoEnable: (autoEnable) => {
    persistBoolean(KEYBOARD_AUTO_ENABLE_STORAGE_KEY, autoEnable);
    set({ autoEnable });
  },
}));
