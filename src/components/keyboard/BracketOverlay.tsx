import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { BracketOverlayPair, BracketOverlayState } from '../../types';

const hiddenState: BracketOverlayState = {
  visible: false,
  pending_pair: null,
  anchor_x: 0,
  anchor_y: 0,
};

const bracketItems: Array<{
  pair: BracketOverlayPair;
  keys: string;
  output: string;
}> = [
  { pair: 'round', keys: 'XK', output: '( )' },
  { pair: 'square', keys: 'ZK', output: '[ ]' },
  { pair: 'curly', keys: 'DK', output: '{ }' },
];

export function BracketOverlay() {
  const [state, setState] = useState<BracketOverlayState>(hiddenState);

  useEffect(() => {
    let active = true;

    document.documentElement.setAttribute('data-theme', 'dark');
    document.body.classList.add('bracket-overlay-window');

    const fetchState = async () => {
      try {
        const nextState = await invoke<BracketOverlayState>('get_bracket_overlay_state');
        if (active) {
          setState(nextState);
        }
      } catch {
        if (active) {
          setState(hiddenState);
        }
      }
    };

    const unlistenPromise = listen<BracketOverlayState>('bracket-overlay-state', (event) => {
      if (active) {
        setState(event.payload);
      }
    });

    void fetchState();

    const resyncInterval = setInterval(() => {
      if (active) void fetchState();
    }, 150);

    return () => {
      active = false;
      clearInterval(resyncInterval);
      void unlistenPromise.then((unlisten) => unlisten());
      document.body.classList.remove('bracket-overlay-window');
    };
  }, []);

  return (
    <div className="bracket-overlay-app">
      {state.visible ? (
        <div className="bracket-overlay-panel animate-fade-in">
          {bracketItems.map((item) => {
            const isActive = state.pending_pair === item.pair;
            const isDimmed = state.pending_pair !== null && !isActive;

            return (
              <div
                key={item.pair}
                className={[
                  'bracket-overlay-chip',
                  isActive ? 'active' : '',
                  isDimmed ? 'dimmed' : '',
                ]
                  .filter(Boolean)
                  .join(' ')}
              >
                <span className="bracket-overlay-keys">{item.keys}</span>
                <span className="bracket-overlay-output">{item.output}</span>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
