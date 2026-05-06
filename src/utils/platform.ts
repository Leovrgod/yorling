import type { ModuleName } from '../types';

export type YorlingPlatform = 'macos' | 'windows' | 'linux' | 'unknown';

const ALL_MODULES: ModuleName[] = [
  'keyboard',
  'music',
  'superRightClick',
  'clipboard',
  'island',
  'terminals',
];

export function normalizePlatformName(value?: string | null): YorlingPlatform {
  const normalized = (value ?? '').toLowerCase();

  if (normalized.includes('win')) return 'windows';
  if (normalized.includes('mac') || normalized.includes('darwin')) return 'macos';
  if (normalized.includes('linux')) return 'linux';

  return 'unknown';
}

export function detectYorlingPlatform(navigatorLike: Navigator = globalThis.navigator): YorlingPlatform {
  const userAgentPlatform = (navigatorLike as Navigator & {
    userAgentData?: { platform?: string };
  }).userAgentData?.platform;
  const candidates = [
    userAgentPlatform,
    navigatorLike.platform,
    navigatorLike.userAgent,
  ];

  for (const candidate of candidates) {
    const platform = normalizePlatformName(candidate);
    if (platform !== 'unknown') {
      return platform;
    }
  }

  return 'unknown';
}

export function getEnabledModulesForPlatform(platform: YorlingPlatform): ModuleName[] {
  if (platform === 'windows') {
    return ['keyboard', 'music', 'clipboard', 'island'];
  }

  return ALL_MODULES;
}

export function isModuleEnabledOnPlatform(module: ModuleName, platform: YorlingPlatform): boolean {
  return getEnabledModulesForPlatform(platform).includes(module);
}
