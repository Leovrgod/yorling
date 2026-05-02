import { useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Toggle } from '../common/Toggle';
import { getUiCopy } from '../../i18n/copy';
import { useAppStore } from '../../stores/appStore';
import { playIslandSound, preloadIslandSound } from '../../island/sound';
import { useIslandStore } from '../../island/store/islandStore';
import type { IslandPluginInfo, IslandProviderInfo, IslandScreenListItem } from '../../types';

export function IslandSettings() {
  const language = useAppStore((state) => state.language);
  const copy = getUiCopy(language);
  const [providers, setProviders] = useState<IslandProviderInfo[]>([]);
  const [plugins, setPlugins] = useState<IslandPluginInfo[]>([]);
  const [screens, setScreens] = useState<IslandScreenListItem[]>([]);
  const [preferredScreen, setPreferredScreen] = useState<string | null>(null);
  const [loading, setLoading] = useState<string | null>(null);
  const [hookFeedback, setHookFeedback] = useState<{
    providerId: string;
    kind: 'success' | 'error';
    message: string;
  } | null>(null);
  const [islandEnabled, setIslandEnabled] = useState(true);
  const soundPrefs = useIslandStore((state) => state.soundPrefs);

  const fetchProviders = async () => {
    try {
      const result = await invoke<IslandProviderInfo[]>('get_provider_status');
      setProviders(result);
    } catch {
      // Not ready yet
    }
  };

  const fetchPlugins = async () => {
    try {
      const result = await invoke<IslandPluginInfo[]>('get_island_plugins');
      setPlugins(result);
    } catch {
      // Plugin platform is optional
    }
  };

  const fetchEnabled = async () => {
    try {
      const enabled = await invoke<boolean>('get_island_enabled');
      setIslandEnabled(enabled);
    } catch {
      // Not ready yet
    }
  };

  const fetchScreens = async () => {
    try {
      const result = await invoke<IslandScreenListItem[]>('get_all_screens');
      setScreens(result);
    } catch {
      // Not ready yet
    }
  };

  const handleScreenChange = async (screenName: string) => {
    const value = screenName === '__auto__' ? null : screenName;
    setPreferredScreen(value);
    try {
      await invoke('set_preferred_screen', { screenName: value });
    } catch (e) {
      console.error('Failed to set preferred screen:', e);
    }
  };

  const fetchPreferredScreen = async () => {
    try {
      const result = await invoke<string | null>('get_preferred_screen');
      setPreferredScreen(result);
    } catch {
      // Not ready yet
    }
  };

  useEffect(() => {
    fetchProviders();
    fetchPlugins();
    fetchEnabled();
    fetchScreens();
    fetchPreferredScreen();
  }, []);

  useEffect(() => {
    preloadIslandSound(soundPrefs.style);
  }, [soundPrefs.style]);

  const handleToggleEnabled = async () => {
    const newValue = !islandEnabled;
    setIslandEnabled(newValue);
    try {
      await invoke('set_island_enabled', { enabled: newValue });
      window.dispatchEvent(new CustomEvent('yorling:island-enabled-change', { detail: newValue }));
      if (newValue && soundPrefs.enabled) {
        playIslandSound(soundPrefs.style, soundPrefs.volume);
      }
    } catch (e) {
      console.error('Failed to toggle island:', e);
      setIslandEnabled(!newValue); // revert
    }
  };

  const handleInstall = async (providerId: string) => {
    setLoading(providerId);
    setHookFeedback(null);
    try {
      await invoke('install_provider_hooks', { providerId });
      await fetchProviders();
      setHookFeedback({
        providerId,
        kind: 'success',
        message: language === 'zh'
          ? 'Hook 已写入配置。请重启或新开一个 Agent 会话来验证实时事件。'
          : 'Hook written to config. Restart or open a new agent session to verify live events.',
      });
    } catch (e) {
      console.error('Failed to install hooks:', e);
      setHookFeedback({
        providerId,
        kind: 'error',
        message: language === 'zh'
          ? `安装失败：${String(e)}`
          : `Install failed: ${String(e)}`,
      });
    } finally {
      setLoading(null);
    }
  };

  const handleUninstall = async (providerId: string) => {
    setLoading(providerId);
    setHookFeedback(null);
    try {
      await invoke('uninstall_provider_hooks', { providerId });
      await fetchProviders();
      setHookFeedback({
        providerId,
        kind: 'success',
        message: language === 'zh'
          ? 'Hook 已从配置中移除。已经打开的 Agent 会话可能还会保留旧状态。'
          : 'Hook removed from config. Already-open agent sessions may keep old state.',
      });
    } catch (e) {
      console.error('Failed to uninstall hooks:', e);
      setHookFeedback({
        providerId,
        kind: 'error',
        message: language === 'zh'
          ? `卸载失败：${String(e)}`
          : `Uninstall failed: ${String(e)}`,
      });
    } finally {
      setLoading(null);
    }
  };

  const getStatusLabel = (status: IslandProviderInfo['hook_status']): string => {
    if (typeof status === 'string') {
      switch (status) {
        case 'installed': return copy.island.hookInstalled;
        case 'not_installed': return copy.island.hookNotInstalled;
        case 'outdated': return copy.island.hookOutdated;
        default: return status;
      }
    }
    if ('broken' in status) return copy.island.hookBroken;
    return 'Unknown';
  };

  const getStatusReason = (status: IslandProviderInfo['hook_status']): string | null => {
    if (typeof status === 'string' || !('broken' in status)) {
      return null;
    }

    return status.broken.reason;
  };

  const getProviderLocation = (provider: IslandProviderInfo): string => {
    if (provider.config_paths.length > 0) {
      return provider.config_paths[0];
    }
    switch (provider.id) {
      case 'claude-code':
        return '~/.claude/settings.json';
      case 'codex':
        return '~/.codex/hooks.json';
      case 'gemini':
        return '~/.gemini/settings.json';
      case 'cursor':
        return '~/.cursor/hooks.json';
      case 'copilot':
        return '~/.copilot/hooks/yorling-island.json';
      default:
        return '-';
    }
  };

  const getStatusDotClass = (status: IslandProviderInfo['hook_status']): string => {
    if (typeof status === 'string') {
      return `island-provider-card__status-dot island-provider-card__status-dot--${status}`;
    }
    return 'island-provider-card__status-dot island-provider-card__status-dot--broken';
  };

  const connectedProviders = useMemo(
    () => providers.filter((provider) => provider.hook_status === 'installed'),
    [providers],
  );
  const orderedProviders = useMemo(() => {
    const priority = (status: IslandProviderInfo['hook_status']): number => {
      if (status === 'installed') return 0;
      if (status === 'outdated') return 1;
      if (typeof status === 'object') return 2;
      return 3;
    };

    return [...providers].sort((a, b) => {
      const priorityDiff = priority(a.hook_status) - priority(b.hook_status);
      if (priorityDiff !== 0) {
        return priorityDiff;
      }
      return a.display_name.localeCompare(b.display_name, language === 'zh' ? 'zh-Hans-CN' : 'en');
    });
  }, [language, providers]);
  const visiblePlugins = plugins.filter((plugin) => plugin.source === 'manifest');

  return (
    <div className="page-enter island-settings">
      <div className="island-settings__hero">
        <div className="island-settings__hero-copy">
          <span className="island-settings__eyebrow">Dynamic Island</span>
          <h2>{copy.island.title}</h2>
          <div className="island-settings__connected">
            <div className="island-settings__connected-header">
              <span className="island-settings__connected-title">
                {language === 'zh' ? '已连接 Agent' : 'Connected Agents'}
              </span>
              {providers.length > 0 ? (
                <span className="island-settings__connected-count">
                  {connectedProviders.length}/{providers.length}
                </span>
              ) : null}
            </div>
            <div className="island-settings__connected-list">
              {connectedProviders.length > 0 ? connectedProviders.map((provider) => (
                <span key={provider.id} className="island-settings__connected-pill">
                  <span className="island-settings__connected-pill-dot" />
                  {provider.display_name}
                </span>
              )) : (
                <span className="island-settings__connected-empty">
                  {language === 'zh' ? '当前还没有已连接的 Agent' : 'No connected agents yet'}
                </span>
              )}
            </div>
          </div>
        </div>
        <div className="island-settings__hero-side">
          <span className="island-settings__status-pill">
            {islandEnabled ? copy.island.enabled : copy.island.disabled}
          </span>
          <Toggle
            active={islandEnabled}
            onChange={() => {
              void handleToggleEnabled();
            }}
          />
        </div>
      </div>

      <div className="card" style={{ marginTop: 'var(--space-4)' }}>
        <div className="card-body" style={{ padding: 'var(--space-6)' }}>
          <h3 className="text-sm" style={{ marginBottom: 'var(--space-4)' }}>
            {copy.island.providersTitle}
          </h3>

          <div className="island-settings__providers">
            {providers.length === 0 ? (
              <div className="text-secondary text-sm">{copy.island.loadingProviders}</div>
            ) : (
              orderedProviders.map((provider) => {
                const isInstalled = provider.hook_status === 'installed';
                const canUninstall = provider.hook_status !== 'not_installed';
                const statusReason = getStatusReason(provider.hook_status);
                const canManageHooks = provider.manages_hooks;
                const healthIssues = provider.health_report.issues.filter(
                  (issue) => issue.severity !== 'info',
                );
                return (
                  <div key={provider.id} className="island-provider-card">
                    <div className="island-provider-card__info">
                      <div className="island-provider-card__name-row">
                        <div className="island-provider-card__name">{provider.display_name}</div>
                        <div className="island-provider-card__status text-tertiary">
                          <span className={getStatusDotClass(provider.hook_status)} />
                          {getStatusLabel(provider.hook_status)}
                        </div>
                      </div>
                      <div className="island-provider-card__location">
                        {copy.island.configPathLabel}: {getProviderLocation(provider)}
                      </div>
                      <div className="island-provider-card__location">
                        {provider.origin === 'external'
                          ? language === 'zh'
                            ? '来源: 第三方插件'
                            : 'Source: External plugin'
                          : language === 'zh'
                            ? '来源: 内建'
                            : 'Source: Built in'}
                        {provider.slot_count > 0
                          ? language === 'zh'
                            ? ` · 插槽 ${provider.slot_count}`
                            : ` · ${provider.slot_count} slots`
                          : ''}
                      </div>
                      {statusReason ? (
                        <div className="island-provider-card__reason">{statusReason}</div>
                      ) : null}
                      {hookFeedback?.providerId === provider.id ? (
                        <div className={`island-provider-card__feedback island-provider-card__feedback--${hookFeedback.kind}`}>
                          {hookFeedback.message}
                        </div>
                      ) : null}
                      {healthIssues.slice(0, 2).map((issue) => (
                        <div key={`${provider.id}-${issue.code}`} className="island-provider-card__reason">
                          {issue.message}
                        </div>
                      ))}
                    </div>
                    <div className="island-provider-card__actions">
                      <button
                        className="island-provider-card__action"
                        onClick={() => handleInstall(provider.id)}
                        disabled={loading === provider.id || !canManageHooks}
                        title={!canManageHooks ? (language === 'zh' ? '该 Provider 需要手动安装插件或 Hook' : 'This provider requires manual plugin or hook setup') : undefined}
                      >
                        {loading === provider.id
                          ? '...'
                          : !canManageHooks
                            ? language === 'zh'
                              ? '手动安装'
                              : 'Manual'
                          : isInstalled
                            ? copy.island.reinstallHook
                            : copy.island.installHook}
                      </button>
                      <button
                        className="island-provider-card__action island-provider-card__action--danger"
                        onClick={() => handleUninstall(provider.id)}
                        disabled={loading === provider.id || !canUninstall || !canManageHooks}
                      >
                        {loading === provider.id ? '...' : copy.island.uninstallHook}
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </div>
        </div>
      </div>

      {visiblePlugins.length > 0 ? (
        <div className="card" style={{ marginTop: 'var(--space-4)' }}>
          <div className="card-body" style={{ padding: 'var(--space-6)' }}>
            <h3 className="text-sm" style={{ marginBottom: 'var(--space-4)' }}>
              {language === 'zh' ? '插件' : 'Plugins'}
            </h3>
            <div className="island-settings__providers">
              {visiblePlugins.map((plugin) => (
                <div key={plugin.id} className="island-provider-card">
                  <div className="island-provider-card__info">
                    <div className="island-provider-card__name-row">
                      <div className="island-provider-card__name">{plugin.name}</div>
                      <div className="island-provider-card__status text-tertiary">
                        {plugin.source === 'manifest'
                          ? language === 'zh'
                            ? 'Manifest'
                            : 'Manifest'
                          : language === 'zh'
                            ? '内建'
                            : 'Built in'}
                      </div>
                    </div>
                    <div className="island-provider-card__location">
                      v{plugin.version}
                      {plugin.provider_id ? ` · ${plugin.provider_id}` : ''}
                      {plugin.slots.length > 0
                        ? language === 'zh'
                          ? ` · ${plugin.slots.length} 个插槽`
                          : ` · ${plugin.slots.length} slots`
                        : ''}
                    </div>
                    {plugin.description ? (
                      <div className="island-provider-card__reason">{plugin.description}</div>
                    ) : null}
                    {plugin.manifest_path ? (
                      <div className="island-provider-card__reason">{plugin.manifest_path}</div>
                    ) : null}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : null}

      {screens.length > 1 && (
        <div className="card" style={{ marginTop: 'var(--space-4)' }}>
          <div className="card-body" style={{ padding: 'var(--space-6)' }}>
            <h3 className="text-sm" style={{ marginBottom: 'var(--space-4)' }}>
              {language === 'zh' ? '显示器' : 'Display'}
            </h3>
            <p className="text-xs text-secondary" style={{ marginBottom: 'var(--space-3)' }}>
              {language === 'zh'
                ? '选择灵动岛显示在哪个屏幕上'
                : 'Choose which screen to show the island on'}
            </p>
            <div className="island-settings__screens">
              <label className="island-settings__screen-option">
                <input
                  type="radio"
                  name="preferred-screen"
                  checked={preferredScreen === null}
                  onChange={() => handleScreenChange('__auto__')}
                />
                <span className="island-settings__screen-label">
                  {language === 'zh' ? '自动（优先刘海屏）' : 'Auto (prefer notch)'}
                </span>
              </label>
              {screens.map((screen) => (
                <label key={screen.screen_name} className="island-settings__screen-option">
                  <input
                    type="radio"
                    name="preferred-screen"
                    checked={preferredScreen === screen.screen_name}
                    onChange={() => handleScreenChange(screen.screen_name)}
                  />
                  <span className="island-settings__screen-label">
                    {screen.screen_name}
                    {screen.has_notch ? ' (notch)' : ''}
                    {screen.is_builtin ? ' (built-in)' : ''}
                    <span className="text-tertiary" style={{ marginLeft: 'var(--space-2)' }}>
                      {Math.round(screen.screen_width)}x{Math.round(screen.screen_height)}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
