import { useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useIslandStore } from '../store/islandStore';
import { playIslandEventSound, preloadIslandSound } from '../sound';
import type {
  IslandEventPayload,
  IslandPluginInfo,
  IslandProviderInfo,
  IslandScreenInfo,
  IslandScreenListItem,
  IslandSession,
} from '../../types';

/**
 * Hook that connects the frontend island store to the Rust backend.
 * Listens for real-time events and polls for initial state.
 */
export function useIslandState() {
  const {
    setSessions,
    updateSession,
    removeSession,
    addNotification,
    setProviders,
    setPlugins,
    setScreenInfo,
  } = useIslandStore();
  const soundPrefs = useIslandStore((s) => s.soundPrefs);

  // Fetch initial session list
  const fetchSessions = useCallback(async () => {
    try {
      const sessions = await invoke<IslandSession[]>('get_island_sessions');
      setSessions(sessions);
    } catch {
      // Island server may not be ready yet
    }
  }, [setSessions]);

  // Fetch provider status
  const fetchProviders = useCallback(async () => {
    try {
      const providers = await invoke<IslandProviderInfo[]>('get_provider_status');
      setProviders(providers);
    } catch {
      // Not critical
    }
  }, [setProviders]);

  const fetchPlugins = useCallback(async () => {
    try {
      const plugins = await invoke<IslandPluginInfo[]>('get_island_plugins');
      setPlugins(plugins);
    } catch {
      // Plugin platform is optional in the overlay.
    }
  }, [setPlugins]);

  const fetchScreenInfo = useCallback(async () => {
    try {
      const screenInfo = await invoke<IslandScreenInfo>('get_island_screen_info');
      setScreenInfo(screenInfo);
    } catch {
      // Island geometry is optional; keep the fallback dimensions.
    }
  }, [setScreenInfo]);

  // Listen for real-time events from Rust
  useEffect(() => {
    preloadIslandSound(soundPrefs.style);

    // Initial fetch
    fetchSessions();
    fetchProviders();
    fetchPlugins();
    fetchScreenInfo();

    // Poll sessions periodically (backup for missed events)
    const interval = setInterval(fetchSessions, 15000);

    // Listen for island events
    const unlisten = listen<IslandEventPayload>('island-event', (event) => {
      const payload = event.payload;

      if (soundPrefs.enabled) {
        playIslandEventSound(payload.event_type, soundPrefs.style, soundPrefs.volume);
      }

      // Handle notifications specially
      if (payload.event_type.type === 'Notification') {
        const data = payload.event_type.data as { title: string; body: string } | undefined;
        if (data) {
          addNotification({
            id: `${payload.session_id}-${payload.timestamp}`,
            title: data.title,
            body: data.body,
            timestamp: payload.timestamp,
            sessionId: payload.session_id,
          });
        }
      }
    });

    const unlistenSessionSync = listen<IslandSession>('island-session-sync', (event) => {
      updateSession(event.payload);
    });

    const unlistenSessionRemove = listen<string>('island-session-remove', (event) => {
      removeSession(event.payload);
    });

    // Listen for global shortcut events
    const unlistenShortcut = listen<string>('island-shortcut', (event) => {
      const action = event.payload;
      if (action === 'toggle') {
        window.dispatchEvent(new CustomEvent('yorling:island-toggle'));
      } else if (action === 'approve') {
        window.dispatchEvent(new CustomEvent('yorling:island-approve'));
      } else if (action === 'deny') {
        window.dispatchEvent(new CustomEvent('yorling:island-deny'));
      }
    });

    // Listen for screen configuration changes (multi-monitor)
    const unlistenScreenChange = listen<IslandScreenInfo>('island-screen-changed', (event) => {
      setScreenInfo(event.payload);
    });

    return () => {
      clearInterval(interval);
      unlisten.then((fn) => fn());
      unlistenSessionSync.then((fn) => fn());
      unlistenSessionRemove.then((fn) => fn());
      unlistenShortcut.then((fn) => fn());
      unlistenScreenChange.then((fn) => fn());
    };
  }, [
    fetchSessions,
    fetchProviders,
    fetchPlugins,
    fetchScreenInfo,
    updateSession,
    removeSession,
    addNotification,
    soundPrefs.enabled,
    soundPrefs.style,
    soundPrefs.volume,
  ]);

  return { fetchSessions, fetchProviders, fetchPlugins, fetchScreenInfo };
}

/** Actions for interacting with the island backend */
export function useIslandActions() {
  return {
    approvePermission: async (requestId: string, decision: 'allow' | 'deny' | 'allow_always') => {
      await invoke('approve_permission', { requestId, decision });
    },
    answerQuestion: async (requestId: string, answer: string) => {
      await invoke('answer_question', { requestId, answer });
    },
    installHooks: async (providerId: string) => {
      await invoke('install_provider_hooks', { providerId });
    },
    uninstallHooks: async (providerId: string) => {
      await invoke('uninstall_provider_hooks', { providerId });
    },
    setMousePassthrough: async (passthrough: boolean) => {
      await invoke('set_island_mouse_passthrough', { passthrough });
    },
    jumpToTerminal: async (sessionId: string) => {
      try {
        await invoke('jump_to_terminal', { sessionId });
        // Collapse the island after a successful jump so the target app is visible
        window.dispatchEvent(new CustomEvent('yorling:island-collapse'));
      } catch (error) {
        console.error('[Island] jump_to_terminal failed:', error);
      }
    },
    getAllScreens: async () => {
      return invoke<IslandScreenListItem[]>('get_all_screens');
    },
    setPreferredScreen: async (screenName: string | null) => {
      await invoke('set_preferred_screen', { screenName });
    },
  };
}
