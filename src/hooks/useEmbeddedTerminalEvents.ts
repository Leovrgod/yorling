import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useTerminalStore } from '../stores/terminalStore';

interface EmbeddedTerminalDataEvent {
  id: string;
  stream: 'pty' | 'stdout' | 'stderr';
  data: string;
}

interface EmbeddedTerminalExitEvent {
  id: string;
  code: number | null;
}

export function useEmbeddedTerminalEvents(enabled = true) {
  const appendOutput = useTerminalStore((state) => state.appendEmbeddedTerminalOutput);
  const markExited = useTerminalStore((state) => state.markEmbeddedTerminalExited);

  useEffect(() => {
    if (!enabled) {
      return undefined;
    }

    const unlistenData = listen<EmbeddedTerminalDataEvent>('embedded-terminal-data', (event) => {
      appendOutput(event.payload.id, event.payload.data);
    });
    const unlistenExit = listen<EmbeddedTerminalExitEvent>('embedded-terminal-exit', (event) => {
      markExited(event.payload.id, event.payload.code);
    });

    return () => {
      unlistenData.then((cleanup) => cleanup());
      unlistenExit.then((cleanup) => cleanup());
    };
  }, [appendOutput, enabled, markExited]);
}
