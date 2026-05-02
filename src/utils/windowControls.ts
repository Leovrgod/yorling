interface WindowZoomTarget {
  isFullscreen(): Promise<boolean>;
  setFullscreen(fullscreen: boolean): Promise<void>;
  isMaximized(): Promise<boolean>;
  maximize(): Promise<void>;
  unmaximize(): Promise<void>;
}

function isMacWindowPlatform(platform: string = globalThis.navigator?.platform ?? ''): boolean {
  return platform.toLowerCase().includes('mac');
}

export async function toggleWindowZoom(
  window: WindowZoomTarget,
  platform: string = globalThis.navigator?.platform ?? '',
): Promise<void> {
  if (isMacWindowPlatform(platform)) {
    if (await window.isFullscreen()) {
      await window.setFullscreen(false);
      return;
    }

    await window.setFullscreen(true);
    return;
  }

  if (await window.isMaximized()) {
    await window.unmaximize();
    return;
  }

  await window.maximize();
}
