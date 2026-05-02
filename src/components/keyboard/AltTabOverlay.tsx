import { useCallback, useEffect, useRef, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AltTabAppIconUpdate,
  AltTabOverlaySelection,
  AltTabOverlayState,
  AltTabOverlayThumbnailUpdate,
} from '../../types';
import {
  getAltTabAppLabel,
  getAltTabTitleBarFontSize,
  getAltTabTitleBarHeight,
} from './altTabOverlayTileMeta';

type OverlayRowItem = {
  index: number;
  tileHeight: number;
  tileWidth: number;
  windowInfo: AltTabOverlayState['windows'][number];
};

type AltTabItemStyle = CSSProperties & {
  '--alt-tab-titlebar-height': string;
  '--alt-tab-titlebar-font-size': string;
};

const hiddenState: AltTabOverlayState = {
  visible: false,
  session_id: 0,
  selected_index: null,
  hovered_index: null,
  windows: [],
  layout: {
    rows: 0,
    panel_width: 0,
    panel_height: 0,
    tile_gap: 18,
    tile_height: 0,
    tiles: [],
  },
};

export function AltTabOverlay() {
  const [state, setState] = useState<AltTabOverlayState>(hiddenState);
  const [iconsByPid, setIconsByPid] = useState<Map<number, string>>(() => new Map());
  const [thumbsByWid, setThumbsByWid] = useState<Map<number, string>>(() => new Map());
  const stateRef = useRef(state);
  const pendingHoveredWindowIdRef = useRef<number | null>(null);
  const hoverFrameRef = useRef<number | null>(null);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const clearPendingHoveredWindow = useCallback(() => {
    if (hoverFrameRef.current !== null) {
      window.cancelAnimationFrame(hoverFrameRef.current);
      hoverFrameRef.current = null;
    }
    pendingHoveredWindowIdRef.current = null;
  }, []);

  const applyOverlayState = useCallback(
    (nextState: AltTabOverlayState) => {
      if (nextState.hovered_index === null) {
        clearPendingHoveredWindow();
      }

      const currentSessionId =
        stateRef.current.visible && stateRef.current.session_id > 0 ? stateRef.current.session_id : 0;
      const nextSessionId = nextState.visible && nextState.session_id > 0 ? nextState.session_id : 0;
      if (currentSessionId !== nextSessionId) {
        setThumbsByWid(new Map());
      }

      stateRef.current = nextState;
      setState(nextState);
    },
    [clearPendingHoveredWindow],
  );

  const flushHoveredWindow = useCallback(() => {
    hoverFrameRef.current = null;

    const windowId = pendingHoveredWindowIdRef.current;
    pendingHoveredWindowIdRef.current = null;

    const current = stateRef.current;
    if (!current.visible || current.session_id === 0) {
      return;
    }

    void invoke('set_alt_tab_hovered_window', { windowId });
  }, []);

  const scheduleHoveredWindow = useCallback(
    (windowId: number | null, index: number | null) => {
      const current = stateRef.current;
      if (!current.visible || current.session_id === 0) {
        return;
      }

      if (
        current.hovered_index === index &&
        pendingHoveredWindowIdRef.current === windowId
      ) {
        return;
      }

      setState((currentState) => {
        if (!currentState.visible || currentState.session_id !== current.session_id) {
          return currentState;
        }

        if (currentState.hovered_index === index) {
          return currentState;
        }

        return {
          ...currentState,
          hovered_index: index,
        };
      });

      pendingHoveredWindowIdRef.current = windowId;
      if (hoverFrameRef.current === null) {
        hoverFrameRef.current = window.requestAnimationFrame(flushHoveredWindow);
      }
    },
    [flushHoveredWindow],
  );

  useEffect(() => {
    let active = true;

    document.documentElement.setAttribute('data-theme', 'dark');
    document.body.classList.add('alt-tab-window');

    const poll = async () => {
      try {
        const nextState = await invoke<AltTabOverlayState>('get_alt_tab_overlay_state');
        if (active) {
          applyOverlayState(nextState);
        }
      } catch {
        if (active) {
          applyOverlayState(hiddenState);
        }
      }
    };

    void poll();
    const stateUnlistenPromise = listen<AltTabOverlayState>('alt-tab-overlay-state', (event) => {
      if (active) {
        applyOverlayState(event.payload);
      }
    });
    const selectionUnlistenPromise = listen<AltTabOverlaySelection>(
      'alt-tab-overlay-selection',
      (event) => {
        if (active) {
          const current = stateRef.current;
          if (
            event.payload.visible &&
            (current.windows.length === 0 || current.session_id !== event.payload.session_id)
          ) {
            return;
          }
          applyOverlayState({
            ...current,
            visible: event.payload.visible,
            session_id: event.payload.session_id,
            selected_index: event.payload.selected_index,
            hovered_index: event.payload.hovered_index,
          });
        }
      },
    );
    const thumbnailUnlistenPromise = listen<AltTabOverlayThumbnailUpdate>(
      'alt-tab-overlay-thumbnail',
      (event) => {
        if (active) {
          if (stateRef.current.session_id !== event.payload.session_id) {
            return;
          }

          setThumbsByWid((current) => {
            const next = new Map(current);
            if (event.payload.thumbnail_data_url) {
              next.set(event.payload.window_id, event.payload.thumbnail_data_url);
            } else {
              next.delete(event.payload.window_id);
            }
            return next;
          });
        }
      },
    );
    const appIconUnlistenPromise = listen<AltTabAppIconUpdate>(
      'alt-tab-overlay-app-icon',
      (event) => {
        if (active) {
          setIconsByPid((current) => {
            const next = new Map(current);
            if (event.payload.app_icon_data_url) {
              next.set(event.payload.owner_pid, event.payload.app_icon_data_url);
            } else {
              next.delete(event.payload.owner_pid);
            }
            return next;
          });
        }
      },
    );

    return () => {
      active = false;
      clearPendingHoveredWindow();
      void Promise.all([
        stateUnlistenPromise,
        selectionUnlistenPromise,
        thumbnailUnlistenPromise,
        appIconUnlistenPromise,
      ]).then((unlisteners) => {
        unlisteners.forEach((unlisten) => unlisten());
      });
      document.body.classList.remove('alt-tab-window');
    };
  }, [applyOverlayState, clearPendingHoveredWindow]);

  if (!state.visible) {
    return <div className="alt-tab-app" />;
  }

  const tileGap = Math.max(state.layout.tile_gap || 18, 0);
  const fallbackTileHeight = Math.max(state.layout.tile_height || 240, 1);
  const panelWidth = Math.max(state.layout.panel_width || 0, 0);
  const panelHeight = Math.max(state.layout.panel_height || 0, 0);
  const rowCount = Math.max(state.layout.rows || 0, 1);
  const rows: OverlayRowItem[][] = Array.from({ length: rowCount }, () => []);

  state.windows.forEach((windowInfo, index) => {
    const tile = state.layout.tiles[index];
    const tileHeight = Math.max(tile?.height || fallbackTileHeight, 1);
    const aspectRatio =
      windowInfo.width > 0 && windowInfo.height > 0 ? windowInfo.width / windowInfo.height : 1.6;
    const tileWidth = Math.max(tile?.width || Math.round(tileHeight * aspectRatio), 1);
    const rowIndex = Math.max(0, Math.min(tile?.row ?? 0, rowCount - 1));
    rows[rowIndex].push({
      index,
      tileHeight,
      tileWidth,
      windowInfo,
    });
  });

  const visibleRows = rows.filter((row) => row.length > 0);

  return (
    <div className="alt-tab-app">
      <div
        className="alt-tab-panel animate-fade-in"
        style={{
          width: panelWidth > 0 ? `${panelWidth}px` : undefined,
          height: panelHeight > 0 ? `${panelHeight}px` : undefined,
        }}
      >
        <div className="alt-tab-list" style={{ gap: `${tileGap}px` }}>
          {visibleRows.map((row, rowIndex) => (
            <div className="alt-tab-row" key={`row-${rowIndex}`} style={{ gap: `${tileGap}px` }}>
              {row.map(({ index, tileHeight, tileWidth, windowInfo }) => {
                const isSelected = index === state.selected_index;
                const isHovered = index === state.hovered_index;
                const appIconDataUrl = iconsByPid.get(windowInfo.owner_pid);
                const thumbnailDataUrl = thumbsByWid.get(windowInfo.id);
                const appLabel = getAltTabAppLabel(windowInfo);
                const title = windowInfo.title.trim() || appLabel;
                const accessibilityLabel =
                  title === appLabel ? title : `${appLabel} — ${title}`;
                const fallbackLabel = windowInfo.title.trim() || appLabel;
                const tileStyle: AltTabItemStyle = {
                  width: `${tileWidth}px`,
                  flex: `0 0 ${tileWidth}px`,
                  '--alt-tab-titlebar-height': `${getAltTabTitleBarHeight(tileHeight)}px`,
                  '--alt-tab-titlebar-font-size': `${getAltTabTitleBarFontSize(tileHeight)}px`,
                };

                return (
                  <button
                    key={`${windowInfo.owner_pid}-${windowInfo.id}`}
                    className={[
                      'alt-tab-item',
                      isSelected ? 'selected' : '',
                      isHovered ? 'hovered' : '',
                    ]
                      .filter(Boolean)
                      .join(' ')}
                    type="button"
                    title={accessibilityLabel}
                    aria-label={accessibilityLabel}
                    style={tileStyle}
                    onMouseEnter={() => {
                      scheduleHoveredWindow(windowInfo.id, index);
                    }}
                    onMouseMove={() => {
                      scheduleHoveredWindow(windowInfo.id, index);
                    }}
                    onMouseLeave={() => {
                      scheduleHoveredWindow(null, null);
                    }}
                    onMouseDown={(event) => {
                      event.preventDefault();
                    }}
                    onClick={() => {
                      clearPendingHoveredWindow();
                      applyOverlayState(hiddenState);
                      void invoke('activate_alt_tab_window', { windowId: windowInfo.id });
                    }}
                  >
                    <div className="alt-tab-item-preview" style={{ height: `${tileHeight}px` }}>
                      <div className="alt-tab-item-titlebar">
                        <div className="alt-tab-item-titlebar-app">
                          {appIconDataUrl ? (
                            <img
                              src={appIconDataUrl}
                              alt=""
                              className="alt-tab-item-app-icon"
                              draggable={false}
                            />
                          ) : (
                            <span
                              className="alt-tab-item-app-icon alt-tab-item-app-icon-placeholder"
                              aria-hidden="true"
                            />
                          )}
                          <span className="alt-tab-item-titlebar-text">{appLabel}</span>
                        </div>
                      </div>
                      <div className="alt-tab-item-media">
                        {thumbnailDataUrl ? (
                          <img
                            src={thumbnailDataUrl}
                            alt=""
                            className="alt-tab-item-thumbnail"
                            draggable={false}
                          />
                        ) : (
                          <div className="alt-tab-item-fallback">
                            <div className="alt-tab-item-fallback-title">{fallbackLabel}</div>
                            {fallbackLabel !== appLabel ? (
                              <div className="alt-tab-item-fallback-app">{appLabel}</div>
                            ) : null}
                          </div>
                        )}
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
