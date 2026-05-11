import { lazy, Suspense, useEffect, useMemo, useState, type ReactNode } from 'react';
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
import {
  detectYorlingPlatform,
  getEnabledModulesForPlatform,
  isModuleEnabledOnPlatform,
  type YorlingPlatform,
} from './utils/platform';

const MusicKeyboard = lazy(() =>
  import('./components/music/MusicKeyboard').then((m) => ({ default: m.MusicKeyboard })),
);

const MultiTerminal = lazy(() =>
  import('./components/terminals/MultiTerminal').then((m) => ({ default: m.MultiTerminal })),
);

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

async function readStartupEnabled(platform: YorlingPlatform): Promise<boolean> {
  if (platform === 'windows') {
    return invoke<boolean>('is_windows_autostart_enabled');
  }

  return isAutostartEnabled();
}

async function setStartupEnabled(platform: YorlingPlatform, enabled: boolean): Promise<boolean> {
  if (platform === 'windows') {
    return invoke<boolean>('set_windows_autostart_enabled', { enabled });
  }

  if (enabled) {
    await enableAutostart();
  } else {
    await disableAutostart();
  }

  return isAutostartEnabled();
}

function StartupToggle({
  language,
  platform,
}: {
  language: AppLanguageId;
  platform: YorlingPlatform;
}) {
  const copy = getUiCopy(language);
  const [enabled, setEnabled] = useState(false);
  const [available, setAvailable] = useState(true);
  const [pending, setPending] = useState(true);

  useEffect(() => {
    let mounted = true;

    void (async () => {
      try {
        const autostartEnabled = await readStartupEnabled(platform);
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
  }, [platform]);

  const handleChange = async (nextEnabled: boolean) => {
    if (pending || !available) return;

    const previousEnabled = enabled;
    setEnabled(nextEnabled);
    setPending(true);

    try {
      setEnabled(await setStartupEnabled(platform, nextEnabled));
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

type WindowControlKind = 'close' | 'minimize' | 'zoom';

function WindowControls({
  language,
  platform,
}: {
  language: AppLanguageId;
  platform: YorlingPlatform;
}) {
  const copy = getUiCopy(language);
  const isWindows = platform === 'windows';
  const controlOrder: WindowControlKind[] = isWindows
    ? ['minimize', 'zoom', 'close']
    : ['close', 'minimize', 'zoom'];

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
      await toggleWindowZoom(getCurrentWindow(), platform);
    } catch (error) {
      console.error('Failed to zoom the main window.', error);
    }
  };

  const handleControlClick = (kind: WindowControlKind) => {
    switch (kind) {
      case 'close':
        void handleClose();
        break;
      case 'minimize':
        void handleMinimize();
        break;
      case 'zoom':
        void handleZoom();
        break;
    }
  };

  const controlLabels: Record<WindowControlKind, string> = {
    close: copy.titlebar.closeWindow,
    minimize: copy.titlebar.minimizeWindow,
    zoom: copy.titlebar.zoomWindow,
  };

  return (
    <div className={`window-controls ${isWindows ? 'window-controls-windows' : 'window-controls-macos'}`}>
      {controlOrder.map((kind) => (
        <button
          key={kind}
          type="button"
          className={`window-control ${kind}`}
          title={controlLabels[kind]}
          aria-label={controlLabels[kind]}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            handleControlClick(kind);
          }}
        >
          <span className="window-control-icon" aria-hidden="true" />
        </button>
      ))}
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
  requiresAccessibility,
  requiresScreenRecording,
  language,
  onOpenAccessibility,
  onOpenScreenRecording,
  utilityControls,
  windowControls,
}: {
  engineRunning: boolean;
  hasAccessibility: boolean;
  hasScreenRecording: boolean;
  requiresAccessibility: boolean;
  requiresScreenRecording: boolean;
  language: AppLanguageId;
  onOpenAccessibility: () => void;
  onOpenScreenRecording: () => void;
  utilityControls: ReactNode;
  windowControls?: ReactNode;
}) {
  const copy = getUiCopy(language);

  return (
    <header
      className={`workspace-header ${windowControls ? 'workspace-header-with-window-controls' : ''}`}
      data-tauri-drag-region
    >
      <div className="workspace-header-actions" data-no-window-drag>
        {utilityControls}
        <span className={`workspace-status-chip ${engineRunning ? 'is-active' : ''}`}>
          <span className={`status-dot ${engineRunning ? 'active' : ''}`} />
          <span className="workspace-status-chip__label">
            {engineRunning ? copy.sidebar.activeStatus : copy.sidebar.inactiveStatus}
          </span>
        </span>
        {requiresAccessibility ? (
          <PermissionStatusToggle
            label={copy.statusbar.accessibility}
            granted={hasAccessibility}
            grantedLabel={copy.statusbar.granted}
            missingLabel={copy.statusbar.missing}
            settingsLabel={copy.statusbar.openAccessibilitySettings}
            onOpenSettings={onOpenAccessibility}
          />
        ) : null}
        {requiresScreenRecording ? (
          <PermissionStatusToggle
            label={copy.statusbar.screenRecording}
            granted={hasScreenRecording}
            grantedLabel={copy.statusbar.granted}
            missingLabel={copy.statusbar.missing}
            settingsLabel={copy.statusbar.openScreenRecordingSettings}
            onOpenSettings={onOpenScreenRecording}
          />
        ) : null}
      </div>
      {windowControls ? (
        <div className="workspace-header-window-controls" data-no-window-drag>
          {windowControls}
        </div>
      ) : null}
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
  const platform = status.platform === 'unknown' ? detectYorlingPlatform() : status.platform;
  const enabledModules = useMemo(() => getEnabledModulesForPlatform(platform), [platform]);
  const activePlatformModule = isModuleEnabledOnPlatform(activeModule, platform)
    ? activeModule
    : 'keyboard';
  const shouldEnableMacOnlyBackgroundModules = platform !== 'windows';
  const [terminalModuleMounted, setTerminalModuleMounted] = useState(
    activePlatformModule === 'terminals',
  );

  useKeyboardPolling();
  useKeyboardStartupRestore();
  useEmbeddedTerminalEvents(shouldEnableMacOnlyBackgroundModules);
  useSuperRightClickStartupRestore(shouldEnableMacOnlyBackgroundModules);

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
    const platformClass = `platform-${platform}`;
    document.body.classList.add(platformClass);

    return () => {
      document.body.classList.remove(platformClass);
    };
  }, [platform]);

  useEffect(() => {
    if (activeModule !== activePlatformModule) {
      setActiveModule(activePlatformModule);
    }
  }, [activeModule, activePlatformModule, setActiveModule]);

  useEffect(() => {
    if (activePlatformModule === 'terminals') {
      setTerminalModuleMounted(true);
    }
  }, [activePlatformModule]);

  const renderTransientModule = () => {
    switch (activePlatformModule) {
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
        activeModule={activePlatformModule}
        enabledModules={enabledModules}
        onModuleChange={setActiveModule}
        windowControls={platform === 'windows' ? null : <WindowControls language={language} platform={platform} />}
      />
      <SidebarDivider />
      <section className="workspace-pane">
        <WorkspaceHeader
          engineRunning={status.running}
          hasAccessibility={hasAccessibility}
          hasScreenRecording={hasScreenRecording}
          requiresAccessibility={status.platform !== 'unknown' && status.requires_accessibility}
          requiresScreenRecording={status.platform !== 'unknown' && status.requires_screen_recording}
          language={language}
          onOpenAccessibility={() => void openAccessibilitySettings()}
          onOpenScreenRecording={() => void openScreenRecordingSettings()}
          utilityControls={(
            <>
              <StartupToggle language={language} platform={platform} />
              <LanguageSwitcher language={language} setLanguage={setLanguage} />
              <ThemeSwitcher theme={theme} setTheme={setTheme} language={language} />
            </>
          )}
          windowControls={
            platform === 'windows' ? <WindowControls language={language} platform={platform} /> : null
          }
        />
        <ErrorBanner />
        <main className="app-content">
          <div className="app-module-stack">
            {activePlatformModule !== 'terminals' ? (
              <div className="app-module app-module-active">
                {renderTransientModule()}
              </div>
            ) : null}
            {terminalModuleMounted ? (
              <div
                className={`app-module app-module-persistent ${
                  activePlatformModule === 'terminals' ? 'app-module-active' : 'app-module-hidden'
                }`}
                aria-hidden={activePlatformModule === 'terminals' ? undefined : true}
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
