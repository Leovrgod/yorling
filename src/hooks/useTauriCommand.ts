import { useCallback, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useKeyboardStore } from '../stores/keyboardStore';
import type { EngineStatus } from '../types';

interface KeyboardPreferenceOptions {
  persistPreference?: boolean;
}

/** Polling hook — call from App root so status stays fresh across all pages. */
export function useKeyboardPolling() {
  const { setStatus } = useKeyboardStore();
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchStatus = useCallback(async () => {
    try {
      const status = await invoke<EngineStatus>('get_status');
      setStatus(status);
    } catch {
      // Backend not available yet
    }
  }, [setStatus]);

  useEffect(() => {
    fetchStatus();
    pollingRef.current = setInterval(fetchStatus, 2000);
    return () => {
      if (pollingRef.current) clearInterval(pollingRef.current);
    };
  }, [fetchStatus]);

  return { fetchStatus };
}

/** Action hook — used by KeyboardMapping page for start/stop/enable/permission. */
export function useKeyboardService() {
  const {
    setStatus,
    setError,
    clearError,
    setAutoStart,
    setAutoEnable,
  } = useKeyboardStore();

  const fetchStatus = useCallback(async () => {
    try {
      const status = await invoke<EngineStatus>('get_status');
      setStatus(status);
    } catch {
      // Ignore
    }
  }, [setStatus]);

  const startInterceptor = useCallback(async (
    options?: KeyboardPreferenceOptions,
  ) => {
    try {
      clearError();
      await invoke('start_interceptor');
      if (options?.persistPreference !== false) {
        setAutoStart(true);
      }
      await fetchStatus();
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e as Error).message ?? 'Unknown error';
      setError(msg);
      await fetchStatus();
      throw e;
    }
  }, [clearError, fetchStatus, setAutoStart, setError]);

  const stopInterceptor = useCallback(async (
    options?: KeyboardPreferenceOptions,
  ) => {
    try {
      clearError();
      await invoke('stop_interceptor');
      if (options?.persistPreference !== false) {
        setAutoStart(false);
      }
      await fetchStatus();
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e as Error).message ?? 'Unknown error';
      setError(msg);
    }
  }, [clearError, fetchStatus, setAutoStart, setError]);

  const setEnabled = useCallback(
    async (enabled: boolean, options?: KeyboardPreferenceOptions) => {
      try {
        await invoke('set_enabled', { enabled });
        if (options?.persistPreference !== false) {
          setAutoEnable(enabled);
          if (enabled) {
            setAutoStart(true);
          }
        }
        await fetchStatus();
      } catch (e) {
        const msg = typeof e === 'string' ? e : (e as Error).message ?? 'Unknown error';
        setError(msg);
      }
    },
    [fetchStatus, setAutoEnable, setAutoStart, setError]
  );

  const setMusicNativeKeySuppression = useCallback(
    async (suppressed: boolean) => {
      try {
        await invoke('set_music_native_keys_suppressed', { suppressed });
        await fetchStatus();
      } catch (e) {
        const msg = typeof e === 'string' ? e : (e as Error).message ?? 'Unknown error';
        setError(msg);
      }
    },
    [fetchStatus, setError]
  );

  const requestAccessibility = useCallback(async () => {
    try {
      await invoke<boolean>('request_accessibility');
      await fetchStatus();
    } catch {
      // Ignore — prompt is best-effort
    }
  }, [fetchStatus]);

  const openAccessibilitySettings = useCallback(async () => {
    try {
      await invoke('open_accessibility_settings');
    } catch {
      // Fallback: try the request prompt instead
      await requestAccessibility();
    }
  }, [requestAccessibility]);

  const requestScreenRecording = useCallback(async () => {
    try {
      await invoke<boolean>('request_screen_recording');
      await fetchStatus();
    } catch {
      // Ignore — prompt is best-effort
    }
  }, [fetchStatus]);

  const openScreenRecordingSettings = useCallback(async () => {
    await requestScreenRecording();

    try {
      await invoke('open_screen_recording_settings');
    } catch {
      // The request prompt above is the best-effort fallback.
    }

    await fetchStatus();
  }, [fetchStatus, requestScreenRecording]);

  return {
    startInterceptor,
    stopInterceptor,
    setEnabled,
    setMusicNativeKeySuppression,
    requestAccessibility,
    openAccessibilitySettings,
    requestScreenRecording,
    openScreenRecordingSettings,
    fetchStatus,
  };
}

export function useKeyboardStartupRestore() {
  const status = useKeyboardStore((state) => state.status);
  const autoStart = useKeyboardStore((state) => state.autoStart);
  const autoEnable = useKeyboardStore((state) => state.autoEnable);
  const { startInterceptor, setEnabled } = useKeyboardService();
  const restoreStateRef = useRef<'idle' | 'restoring' | 'done'>('idle');
  const retryCountRef = useRef(0);

  useEffect(() => {
    if (restoreStateRef.current === 'done' || restoreStateRef.current === 'restoring') {
      return;
    }

    if (!autoStart) {
      restoreStateRef.current = 'done';
      return;
    }

    if (status.platform === 'unknown') {
      return;
    }

    if (!status.interception_supported) {
      restoreStateRef.current = 'done';
      return;
    }

    if (status.requires_accessibility && !status.has_accessibility) {
      return;
    }

    if (retryCountRef.current >= 3) {
      restoreStateRef.current = 'done';
      return;
    }

    restoreStateRef.current = 'restoring';
    retryCountRef.current += 1;

    void (async () => {
      try {
        if (!status.running) {
          await startInterceptor({ persistPreference: false });
        }

        await setEnabled(autoEnable, { persistPreference: false });
        restoreStateRef.current = 'done';
      } catch {
        // Allow retry on next status poll cycle
        restoreStateRef.current = 'idle';
      }
    })();
  }, [
    autoEnable,
    autoStart,
    setEnabled,
    startInterceptor,
    status.has_accessibility,
    status.interception_supported,
    status.platform,
    status.requires_accessibility,
    status.running,
  ]);
}
