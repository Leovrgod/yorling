import { create } from 'zustand';
import type {
  IslandPluginInfo,
  IslandProviderInfo,
  IslandScreenInfo,
  IslandSession,
  IslandViewMode,
} from '../../types';

export type IslandSoundStyle = 'voice' | 'silent';

interface IslandSoundPrefs {
  enabled: boolean;
  volume: number; // 0-1
  style: IslandSoundStyle;
}

interface IslandStore {
  sessions: IslandSession[];
  providers: IslandProviderInfo[];
  plugins: IslandPluginInfo[];
  screenInfo: IslandScreenInfo | null;
  viewMode: IslandViewMode;
  selectedSessionId: string | null;
  notifications: IslandNotification[];
  soundPrefs: IslandSoundPrefs;

  setSessions: (sessions: IslandSession[]) => void;
  updateSession: (session: IslandSession) => void;
  removeSession: (id: string) => void;
  setProviders: (providers: IslandProviderInfo[]) => void;
  setPlugins: (plugins: IslandPluginInfo[]) => void;
  setScreenInfo: (screenInfo: IslandScreenInfo | null) => void;
  setViewMode: (mode: IslandViewMode) => void;
  setSelectedSessionId: (id: string | null) => void;
  addNotification: (notification: IslandNotification) => void;
  removeNotification: (id: string) => void;
  setSoundPrefs: (prefs: Partial<IslandSoundPrefs>) => void;
}

export interface IslandNotification {
  id: string;
  title: string;
  body: string;
  timestamp: number;
  sessionId: string;
}

export const useIslandStore = create<IslandStore>((set) => ({
  sessions: [],
  providers: [],
  plugins: [],
  screenInfo: null,
  viewMode: 'collapsed',
  selectedSessionId: null,
  notifications: [],
  soundPrefs: { enabled: true, volume: 0.5, style: 'voice' },

  setSessions: (sessions) => set({ sessions }),

  updateSession: (session) =>
    set((state) => {
      const idx = state.sessions.findIndex((s) => s.id === session.id);
      if (idx >= 0) {
        const next = [...state.sessions];
        next[idx] = session;
        return { sessions: next };
      }
      return { sessions: [...state.sessions, session] };
    }),

  removeSession: (id) =>
    set((state) => ({
      sessions: state.sessions.filter((s) => s.id !== id),
      selectedSessionId:
        state.selectedSessionId === id ? null : state.selectedSessionId,
    })),

  setProviders: (providers) => set({ providers }),
  setPlugins: (plugins) => set({ plugins }),
  setScreenInfo: (screenInfo) => set({ screenInfo }),
  setViewMode: (viewMode) => set({ viewMode }),
  setSelectedSessionId: (selectedSessionId) => set({ selectedSessionId }),

  addNotification: (notification) =>
    set((state) => ({
      notifications: [...state.notifications, notification].slice(-10),
    })),

  removeNotification: (id) =>
    set((state) => ({
      notifications: state.notifications.filter((n) => n.id !== id),
    })),

  setSoundPrefs: (prefs) =>
    set((state) => ({
      soundPrefs: { ...state.soundPrefs, ...prefs },
    })),
}));
