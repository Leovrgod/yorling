import { open } from '@tauri-apps/plugin-dialog';

interface PickProjectDirectoryOptions {
  defaultPath?: string;
  title?: string;
}

export async function pickProjectDirectory(options: PickProjectDirectoryOptions = {}): Promise<string | null> {
  const selected = await open({
    title: options.title,
    directory: true,
    multiple: false,
    defaultPath: options.defaultPath,
  });

  return typeof selected === 'string' ? selected : null;
}
