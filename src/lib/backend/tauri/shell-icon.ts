/**
 * Retint the running window (taskbar), tray, and Windows desktop shortcuts.
 * The installer package icon stays the bundled default.
 */
import { logger } from '@/lib/logger';
import { appMarkSvg } from '@/lib/app-mark';
import { ACCENT_PALETTES, type AccentId } from '@/styles/tokens';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:shell-icon');
const SHELL_ICON_SIZE = 128;

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error('accent mark image failed to load'));
    img.src = url;
  });
}

export async function rasterizeAccentMark(
  fill: string,
  size = SHELL_ICON_SIZE,
): Promise<{ rgba: number[]; width: number; height: number }> {
  const blob = new Blob([appMarkSvg(fill)], { type: 'image/svg+xml;charset=utf-8' });
  const url = URL.createObjectURL(blob);
  try {
    const img = await loadImage(url);
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('accent mark canvas unavailable');
    ctx.drawImage(img, 0, 0, size, size);
    const { data } = ctx.getImageData(0, 0, size, size);
    return { rgba: Array.from(data), width: size, height: size };
  } finally {
    URL.revokeObjectURL(url);
  }
}

export async function invokeSetShellIcon(
  rgba: number[],
  width: number,
  height: number,
  accentId: AccentId,
): Promise<void> {
  await invoke('set_shell_icon', { rgba, width, height, accentId });
}

export async function applyShellAccentIcon(id: AccentId): Promise<void> {
  const fill = ACCENT_PALETTES[id].light;
  const icon = await rasterizeAccentMark(fill);
  await invokeSetShellIcon(icon.rgba, icon.width, icon.height, id);
}

export function applyShellAccentIconBestEffort(id: AccentId): void {
  void applyShellAccentIcon(id).catch((error) => {
    log.warn('set shell icon failed', error);
  });
}
