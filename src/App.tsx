import { lazy, Suspense, useEffect, useState, type ReactNode } from 'react';
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from '@tauri-apps/plugin-autostart';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { AltTabOverlay } from './components/keyboard/AltTabOverlay';
import { BracketOverlay } from './components/keyboard/BracketOverlay';
import { MiniToggle } from './components/common/MiniToggle';
import { KeyboardMapping } from './components/keyboard/KeyboardMapping';
import { Sidebar, SidebarDivider } from './components/common/Sidebar';
import { ClipboardWall } from './components/clipboard/ClipboardWall';
import { IslandSettings } from './components/island/IslandPlaceholder';
import {
  SuperRightClick,
  useSuperRightClickStartupRestore,
} from './components/super-right-click/SuperRightClick';
import {
  useKeyboardPolling,
  useKeyboardService,
  useKeyboardStartupRestore,
} from './hooks/useTauriCommand';
import { useEmbeddedTerminalEvents } from './hooks/useEmbeddedTerminalEvents';
import { getThemeDescription, getUiCopy, LANGUAGE_OPTIONS } from './i18n/copy';
import { useAppStore } from './stores/appStore';
import { useKeyboardStore } from './stores/keyboardStore';
import { THEME_OPTIONS } from './theme/themeOptions';
import type { AppLanguageId, AppThemeId } from './types';
import { toggleWindowZoom } from './utils/windowControls';

const MusicKeyboard = lazy(() =>
  import('./components/music/MusicKeyboard').then((m) => ({ default: m.MusicKeyboard })),
);

const MultiTerminal = lazy(() =>
  import('./components/terminals/MultiTerminal').then((m) => ({ default: m.MultiTerminal })),
);

type UpdateControlPhase = 'idle' | 'checking' | 'available' | 'installing' | 'current' | 'unconfigured' | 'error';

type AppUpdateCheckResult = {
  configured: boolean;
  available: boolean;
  currentVersion: string;
  version: string | null;
  date: string | null;
  notes: string | null;
  message: string | null;
};

function ThemeSwitcher({
  theme,
  setTheme,
  language,
}: {
  theme: AppThemeId;
  setTheme: (theme: AppThemeId) => void;
  language: AppLanguageId;
}) {
  const copy = getUiCopy(language);

  return (
    <div className="theme-switcher" aria-label={copy.titlebar.themeSwitcherLabel} data-no-window-drag>
      {THEME_OPTIONS.map((option) => (
        <button
          key={option.id}
          type="button"
          className={`theme-switcher-option ${theme === option.id ? 'active' : ''}`}
          onClick={() => setTheme(option.id)}
          title={`${option.label} · ${getThemeDescription(option.id, language)}`}
          aria-pressed={theme === option.id}
        >
          <span className={`theme-switcher-swatch ${option.id}`} aria-hidden="true" />
          <span className="theme-switcher-text">{option.label}</span>
        </button>
      ))}
    </div>
  );
}

function LanguageSwitcher({
  language,
  setLanguage,
}: {
  language: AppLanguageId;
  setLanguage: (language: AppLanguageId) => void;
}) {
  const copy = getUiCopy(language);

  return (
    <div className="language-switcher" aria-label={copy.titlebar.languageSwitcherLabel} data-no-window-drag>
      {LANGUAGE_OPTIONS.map((option) => (
        <button
          key={option.id}
          type="button"
          className={`language-switcher-option ${language === option.id ? 'active' : ''}`}
          data-language={option.id}
          onClick={() => setLanguage(option.id)}
          title={option.label}
        >
          <span className="language-switcher-text" lang={option.id === 'zh' ? 'zh-CN' : 'en'}>
            {option.shortLabel}
          </span>
        </button>
      ))}
    </div>
  );
}

function UpdateControl({ language }: { language: AppLanguageId }) {
  const copy = getUiCopy(language);
  const [phase, setPhase] = useState<UpdateControlPhase>('idle');
  const [update, setUpdate] = useState<AppUpdateCheckResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const busy = phase === 'checking' || phase === 'installing';
  const nextVersion = update?.version ?? null;
  const title = error
    ?? update?.notes
    ?? update?.message
    ?? (nextVersion ? copy.titlebar.updateAvailableTitle(nextVersion) : copy.titlebar.checkForUpdates);

  const label = (() => {
    switch (phase) {
      case 'checking':
        return copy.titlebar.checkingForUpdates;
      case 'available':
        return copy.titlebar.installUpdate;
      case 'installing':
        return copy.titlebar.installingUpdate;
      case 'current':
        return copy.titlebar.upToDate;
      case 'unconfigured':
        return copy.titlebar.updateNotConfigured;
      case 'error':
        return copy.titlebar.updateRetry;
      case 'idle':
      default:
        return copy.titlebar.updateLabel;
    }
  })();

  const checkForUpdate = async () => {
    if (busy) return;
    setError(null);
    setPhase('checking');

    try {
      const result = await invoke<AppUpdateCheckResult>('check_app_update');
      setUpdate(result);

      if (!result.configured) {
        setPhase('unconfigured');
      } else if (result.available) {
        setPhase('available');
      } else {
        setPhase('current');
      }
    } catch (error) {
      console.error('Failed to check for updates.', error);
      setError(error instanceof Error ? error.message : String(error));
      setPhase('error');
    }
  };

  const installUpdate = async () => {
    if (busy) return;
    setError(null);
    setPhase('installing');

    try {
      await invoke('install_app_update');
    } catch (error) {
      console.error('Failed to install update.', error);
      setError(error instanceof Error ? error.message : String(error));
      setPhase('available');
    }
  };

  const handleClick = () => {
    if (phase === 'available') {
      void installUpdate();
      return;
    }
    void checkForUpdate();
  };

  return (
    <button
      type="button"
      className={`update-control update-control--${phase}`}
      title={title}
      aria-label={title}
      data-no-window-drag
      disabled={busy}
      onPointerDown={(event) => event.stopPropagation()}
      onClick={handleClick}
    >
      <span className={`status-dot ${phase === 'available' ? 'warning' : phase === 'current' ? 'active' : ''}`} />
      <span className="update-control__label">{label}</span>
    </button>
  );
}

function StartupToggle({ language }: { language: AppLanguageId }) {
  const copy = getUiCopy(language);
  const [enabled, setEnabled] = useState(false);
  const [available, setAvailable] = useState(true);
  const [pending, setPending] = useState(true);

  useEffect(() => {
    let mounted = true;

    void (async () => {
      try {
        const autostartEnabled = await isAutostartEnabled();
        if (mounted) {
          setEnabled(autostartEnabled);
          setAvailable(true);
        }
      } catch (error) {
        console.error('Failed to read autostart status.', error);
        if (mounted) {
          setAvailable(false);
        }
      } finally {
        if (mounted) {
          setPending(false);
        }
      }
    })();

    return () => {
      mounted = false;
    };
  }, []);

  const handleChange = async (nextEnabled: boolean) => {
    if (pending || !available) return;

    const previousEnabled = enabled;
    setEnabled(nextEnabled);
    setPending(true);

    try {
      if (nextEnabled) {
        await enableAutostart();
      } else {
        await disableAutostart();
      }

      setEnabled(await isAutostartEnabled());
    } catch (error) {
      console.error('Failed to update autostart status.', error);
      setEnabled(previousEnabled);
    } finally {
      setPending(false);
    }
  };

  const valueLabel = !available
    ? copy.titlebar.autoLaunchUnavailable
    : enabled ? copy.titlebar.autoLaunchOn : copy.titlebar.autoLaunchOff;
  const actionLabel = enabled ? copy.titlebar.disableAutoLaunch : copy.titlebar.enableAutoLaunch;

  return (
    <div
      className={`startup-toggle ${!available ? 'is-disabled' : ''}`}
      title={available ? actionLabel : copy.titlebar.autoLaunchUnavailable}
      aria-label={`${copy.titlebar.autoLaunchLabel}: ${valueLabel}`}
      data-no-window-drag
      onPointerDown={(event) => event.stopPropagation()}
    >
      <span className="startup-toggle__copy">
        <span className="startup-toggle__label">{copy.titlebar.autoLaunchLabel}</span>
        <span className="startup-toggle__value">{valueLabel}</span>
      </span>
      <MiniToggle
        active={enabled}
        onChange={handleChange}
        ariaLabel={actionLabel}
        disabled={pending || !available}
      />
    </div>
  );
}

function WindowControls({ language }: { language: AppLanguageId }) {
  const copy = getUiCopy(language);

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      console.error('Failed to close the main window.', error);
    }
  };

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      console.error('Failed to minimize the main window.', error);
    }
  };

  const handleZoom = async () => {
    try {
      await toggleWindowZoom(getCurrentWindow());
    } catch (error) {
      console.error('Failed to zoom the main window.', error);
    }
  };

  return (
    <div className="window-controls">
      <button
        type="button"
        className="window-control close"
        title={copy.titlebar.closeWindow}
        aria-label={copy.titlebar.closeWindow}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => { e.stopPropagation(); void handleClose(); }}
      >
        <span className="window-control-icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-control minimize"
        title={copy.titlebar.minimizeWindow}
        aria-label={copy.titlebar.minimizeWindow}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => { e.stopPropagation(); void handleMinimize(); }}
      >
        <span className="window-control-icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        className="window-control zoom"
        title={copy.titlebar.zoomWindow}
        aria-label={copy.titlebar.zoomWindow}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => { e.stopPropagation(); void handleZoom(); }}
      >
        <span className="window-control-icon" aria-hidden="true" />
      </button>
    </div>
  );
}

function ErrorBanner() {
  const { error, clearError } = useKeyboardStore();
  if (!error) return null;

  return (
    <div className="error-banner">
      <span className="error-banner-icon">✕</span>
      <span className="error-banner-text">{error}</span>
      <button className="error-banner-close" onClick={clearError}>×</button>
    </div>
  );
}

function PermissionStatusToggle({
  label,
  granted,
  grantedLabel,
  missingLabel,
  settingsLabel,
  onOpenSettings,
}: {
  label: string;
  granted: boolean;
  grantedLabel: string;
  missingLabel: string;
  settingsLabel: string;
  onOpenSettings: () => void;
}) {
  const valueLabel = granted ? grantedLabel : missingLabel;

  return (
    <div
      className={`workspace-permission-toggle ${granted ? 'is-granted' : 'is-warning'}`}
      title={settingsLabel}
      aria-label={`${label}: ${valueLabel}`}
      data-no-window-drag
      onPointerDown={(event) => event.stopPropagation()}
    >
      <span className={`status-dot ${granted ? 'active' : 'warning'}`} />
      <span className="workspace-permission-toggle__label">{label}</span>
      <span className="workspace-permission-toggle__value">{valueLabel}</span>
      <MiniToggle
        active={granted}
        onChange={onOpenSettings}
        ariaLabel={settingsLabel}
      />
    </div>
  );
}

function WorkspaceHeader({
  engineRunning,
  hasAccessibility,
  hasScreenRecording,
  language,
  onOpenAccessibility,
  onOpenScreenRecording,
  utilityControls,
}: {
  engineRunning: boolean;
  hasAccessibility: boolean;
  hasScreenRecording: boolean;
  language: AppLanguageId;
  onOpenAccessibility: () => void;
  onOpenScreenRecording: () => void;
  utilityControls: ReactNode;
}) {
  const copy = getUiCopy(language);

  return (
    <header className="workspace-header" data-tauri-drag-region>
      <div className="workspace-header-actions" data-no-window-drag>
        {utilityControls}
        <span className={`workspace-status-chip ${engineRunning ? 'is-active' : ''}`}>
          <span className={`status-dot ${engineRunning ? 'active' : ''}`} />
          <span className="workspace-status-chip__label">
            {engineRunning ? copy.sidebar.activeStatus : copy.sidebar.inactiveStatus}
          </span>
        </span>
        <PermissionStatusToggle
          label={copy.statusbar.accessibility}
          granted={hasAccessibility}
          grantedLabel={copy.statusbar.granted}
          missingLabel={copy.statusbar.missing}
          settingsLabel={copy.statusbar.openAccessibilitySettings}
          onOpenSettings={onOpenAccessibility}
        />
        <PermissionStatusToggle
          label={copy.statusbar.screenRecording}
          granted={hasScreenRecording}
          grantedLabel={copy.statusbar.granted}
          missingLabel={copy.statusbar.missing}
          settingsLabel={copy.statusbar.openScreenRecordingSettings}
          onOpenSettings={onOpenScreenRecording}
        />
      </div>
    </header>
  );
}

function MainApp() {
  const {
    activeModule,
    setActiveModule,
    theme,
    setTheme,
    language,
    setLanguage,
  } = useAppStore();
  const { status } = useKeyboardStore();
  const { openAccessibilitySettings, openScreenRecordingSettings } = useKeyboardService();
  const [terminalModuleMounted, setTerminalModuleMounted] = useState(activeModule === 'terminals');

  useKeyboardPolling();
  useKeyboardStartupRestore();
  useEmbeddedTerminalEvents();
  useSuperRightClickStartupRestore();

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.setAttribute('lang', language === 'zh' ? 'zh-CN' : 'en');
  }, [language]);

  useEffect(() => {
    document.body.classList.add('main-window');

    return () => {
      document.body.classList.remove('main-window');
    };
  }, []);

  useEffect(() => {
    if (activeModule === 'terminals') {
      setTerminalModuleMounted(true);
    }
  }, [activeModule]);

  const renderTransientModule = () => {
    switch (activeModule) {
      case 'keyboard':
        return <KeyboardMapping />;
      case 'music':
        return (
          <Suspense fallback={<div className="module-loading" />}>
            <MusicKeyboard />
          </Suspense>
        );
      case 'superRightClick':
        return <SuperRightClick />;
      case 'clipboard':
        return <ClipboardWall />;
      case 'island':
        return <IslandSettings />;
      case 'terminals':
        return null;
    }
  };

  const hasAccessibility = status.has_accessibility;
  const hasScreenRecording = status.has_screen_recording;

  return (
    <div className="app-shell">
      <Sidebar
        activeModule={activeModule}
        onModuleChange={setActiveModule}
        windowControls={<WindowControls language={language} />}
      />
      <SidebarDivider />
      <section className="workspace-pane">
        <WorkspaceHeader
          engineRunning={status.running}
          hasAccessibility={hasAccessibility}
          hasScreenRecording={hasScreenRecording}
          language={language}
          onOpenAccessibility={() => void openAccessibilitySettings()}
          onOpenScreenRecording={() => void openScreenRecordingSettings()}
          utilityControls={(
            <>
              <UpdateControl language={language} />
              <StartupToggle language={language} />
              <LanguageSwitcher language={language} setLanguage={setLanguage} />
              <ThemeSwitcher theme={theme} setTheme={setTheme} language={language} />
            </>
          )}
        />
        <ErrorBanner />
        <main className="app-content">
          <div className="app-module-stack">
            {activeModule !== 'terminals' ? (
              <div className="app-module app-module-active">
                {renderTransientModule()}
              </div>
            ) : null}
            {terminalModuleMounted ? (
              <div
                className={`app-module app-module-persistent ${
                  activeModule === 'terminals' ? 'app-module-active' : 'app-module-hidden'
                }`}
                aria-hidden={activeModule === 'terminals' ? undefined : true}
              >
                <Suspense fallback={<div className="module-loading" />}>
                  <MultiTerminal />
                </Suspense>
              </div>
            ) : null}
          </div>
        </main>
      </section>
    </div>
  );
}

function App() {
  const label = getCurrentWindow().label;

  if (label === 'alt-tab-overlay') {
    return <AltTabOverlay />;
  }

  if (label === 'bracket-overlay') {
    return <BracketOverlay />;
  }

  return <MainApp />;
}

export default App;
