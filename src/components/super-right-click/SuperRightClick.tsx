import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import { Toggle } from '../common/Toggle';

const SUPER_RIGHT_CLICK_AUTOSTART_KEY = 'yorling.superRightClick.autoStart';
const SUPER_RIGHT_CLICK_TERMINAL_KEY = 'yorling.superRightClick.selectedTerminal';

interface SuperRightClickStatus {
  running: boolean;
  enabled: boolean;
  mode: string;
  native_reference: string;
  native_bundle_id: string;
  native_installed: boolean;
  native_enabled: boolean;
}

interface DetectedTerminalApp {
  id: string;
  label: string;
  installed: boolean;
  embedded: boolean;
  bundle_id: string | null;
  app_path: string | null;
}

interface TerminalInventory {
  terminals: DetectedTerminalApp[];
}

function canUseStorage(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined';
}

function readStoredTerminalId(): string {
  if (!canUseStorage()) return 'terminal';
  return window.localStorage.getItem(SUPER_RIGHT_CLICK_TERMINAL_KEY) || 'terminal';
}

function persistStoredTerminalId(terminalId: string): void {
  if (canUseStorage()) {
    window.localStorage.setItem(SUPER_RIGHT_CLICK_TERMINAL_KEY, terminalId);
  }
}

function persistAutoStart(autoStart: boolean): void {
  if (canUseStorage()) {
    window.localStorage.setItem(SUPER_RIGHT_CLICK_AUTOSTART_KEY, String(autoStart));
  }
}

function readAutoStart(): boolean {
  return canUseStorage() && window.localStorage.getItem(SUPER_RIGHT_CLICK_AUTOSTART_KEY) === 'true';
}

function fallbackStatus(): SuperRightClickStatus {
  return {
    running: false,
    enabled: false,
    mode: 'finder-sync-unavailable',
    native_reference: '',
    native_bundle_id: 'com.yorling.app.FinderSync',
    native_installed: false,
    native_enabled: false,
  };
}

function fallbackTerminal(): DetectedTerminalApp {
  return {
    id: 'terminal',
    label: 'Terminal',
    installed: true,
    embedded: false,
    bundle_id: 'com.apple.Terminal',
    app_path: null,
  };
}

export function useSuperRightClickStartupRestore(enabled = true) {
  const restoredRef = useRef(false);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    if (restoredRef.current || !readAutoStart()) {
      return;
    }

    restoredRef.current = true;
    void (async () => {
      try {
        await invoke('start_super_right_click');
      } catch {
        // The settings page will surface extension enablement details when opened.
      }
    })();
  }, [enabled]);
}

export function SuperRightClick() {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language).superRightClick;
  const [status, setStatus] = useState<SuperRightClickStatus>(fallbackStatus);
  const [inventory, setInventory] = useState<TerminalInventory | null>(null);
  const [selectedTerminalId, setSelectedTerminalId] = useState(readStoredTerminalId);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const availableTerminals = useMemo(() => {
    const terminals = inventory?.terminals.filter((terminal) => (
      !terminal.embedded && terminal.installed
    )) ?? [];
    return terminals.length > 0 ? terminals : [fallbackTerminal()];
  }, [inventory]);

  const selectedTerminal = availableTerminals.find((terminal) => terminal.id === selectedTerminalId)
    ?? availableTerminals[0];

  const fetchStatus = useCallback(async () => {
    try {
      const nextStatus = await invoke<SuperRightClickStatus>('get_super_right_click_status');
      setStatus(nextStatus);
    } catch {
      setStatus(fallbackStatus());
    }
  }, []);

  const fetchInventory = useCallback(async () => {
    try {
      const nextInventory = await invoke<TerminalInventory>('get_terminal_inventory');
      setInventory(nextInventory);
    } catch {
      setInventory({ terminals: [fallbackTerminal()] });
    }
  }, []);

  useEffect(() => {
    void fetchStatus();
    void fetchInventory();
    const interval = window.setInterval(() => {
      void fetchStatus();
    }, 5000);

    return () => window.clearInterval(interval);
  }, [fetchInventory, fetchStatus]);

  useEffect(() => {
    if (availableTerminals.some((terminal) => terminal.id === selectedTerminalId)) return;
    setSelectedTerminalId(availableTerminals[0].id);
  }, [availableTerminals, selectedTerminalId]);

  useEffect(() => {
    void invoke('set_super_right_click_terminal', { terminalId: selectedTerminalId }).catch(() => {
      // The native extension will fall back to Terminal if this has not been written yet.
    });
  }, [selectedTerminalId]);

  const handleTerminalChange = (terminalId: string) => {
    setSelectedTerminalId(terminalId);
    persistStoredTerminalId(terminalId);
  };

  const setRunning = async (running: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (running) {
        await invoke('start_super_right_click');
      } else {
        await invoke('stop_super_right_click');
      }
      persistAutoStart(running);
      await fetchStatus();
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(`${copy.launchErrorPrefix}: ${message}`);
      await fetchStatus();
    } finally {
      setBusy(false);
    }
  };

  const openPermissionSettings = async (command: string) => {
    setError(null);
    try {
      await invoke(command);
    } catch (caught) {
      const message = typeof caught === 'string' ? caught : (caught as Error).message ?? 'Unknown error';
      setError(message);
    } finally {
      await fetchStatus();
    }
  };

  const openFinderExtensionsSettings = async () => {
    await openPermissionSettings('open_finder_sync_extension_settings');
  };

  const openFinderAutomationSettings = async () => {
    await openPermissionSettings('open_finder_automation_settings');
  };

  const openAccessibilitySettings = async () => {
    await openPermissionSettings('open_accessibility_settings');
  };

  const actions = [
    copy.actionNewTxt,
    copy.actionNewMd,
    copy.actionOpenTerminal,
    copy.actionCopyPath,
    copy.actionMoveTo,
    copy.actionToggleHidden,
    copy.actionSnapToGrid,
  ];

  const nativeStatusLabel = status.native_enabled
    ? copy.nativeEnabled
    : status.native_installed
      ? copy.nativeDisabled
      : copy.nativeMissing;
  const isActive = status.enabled;
  const statusLabel = status.enabled
    ? (status.native_enabled ? copy.enabled : nativeStatusLabel)
    : copy.idle;

  return (
    <div className="super-right-click-page">
      <header className="super-right-click-heading">
        <div>
          <h1>{copy.title}</h1>
          <p>{copy.subtitle}</p>
        </div>
        <div className={`super-right-click-status ${status.running ? 'running' : ''}`}>
          <span className={`status-dot ${status.running ? 'active' : 'warning'}`} />
          <span>{statusLabel}</span>
          <small>{status.mode}</small>
        </div>
      </header>

      <section className="super-right-click-layout">
        <div className="super-right-click-panel super-right-click-control-panel">
          <div className="super-right-click-panel-heading">
            <span>{copy.title}</span>
            <Toggle
              active={isActive}
              onChange={setRunning}
              label={isActive ? copy.enabled : copy.disabled}
              disabled={busy}
            />
          </div>

          <div className="super-right-click-field">
            <label htmlFor="super-right-click-terminal">{copy.terminalLabel}</label>
            <div className="super-right-click-select-row">
              <select
                id="super-right-click-terminal"
                value={selectedTerminal?.id ?? 'terminal'}
                onChange={(event) => handleTerminalChange(event.target.value)}
              >
                {availableTerminals.map((terminal) => (
                  <option key={terminal.id} value={terminal.id}>
                    {terminal.label}
                  </option>
                ))}
              </select>
              <button type="button" onClick={() => void fetchInventory()}>
                {copy.refresh}
              </button>
            </div>
            <code>{selectedTerminal?.app_path ?? selectedTerminal?.bundle_id ?? copy.noTerminal}</code>
          </div>

          <div className="super-right-click-permission-panel">
            <div>
              <strong>{copy.permissionRecoveryTitle}</strong>
              <p>{copy.permissionRecoveryDescription}</p>
            </div>
            <div className="super-right-click-permission-actions">
              <button type="button" onClick={() => void openFinderExtensionsSettings()}>
                {copy.nativeSettingsAction}
              </button>
              <button type="button" onClick={() => void openFinderAutomationSettings()}>
                {copy.systemEventsPermissionAction}
              </button>
              <button type="button" onClick={() => void openAccessibilitySettings()}>
                {copy.accessibilitySettingsAction}
              </button>
            </div>
          </div>

          <div className="super-right-click-permission-panel super-right-click-coverage-panel">
            <div>
              <strong>{copy.coverageTitle}</strong>
              <p>{copy.coverageDescription}</p>
            </div>
          </div>

          {error ? <div className="super-right-click-error">{error}</div> : null}
        </div>

        <div className="super-right-click-panel">
          <div className="super-right-click-panel-heading">
            <span>{copy.actionsTitle}</span>
          </div>
          <div className="super-right-click-action-list">
            {actions.map((action) => (
              <div key={action} className="super-right-click-action-row">
                <span>{action}</span>
              </div>
            ))}
          </div>
        </div>

      </section>
    </div>
  );
}
