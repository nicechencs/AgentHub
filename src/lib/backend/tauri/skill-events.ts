/**
 * Tauri-only skill filesystem events.
 * Only Tauri ports may import this module.
 */
import type { SkillsFsChangedPayload } from '@/lib/backend/contracts/skill-types';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { isTauriApp } from '@/lib/platform';

export const SKILLS_FS_CHANGED_EVENT = 'skills-fs-changed';

export type { SkillsFsChangedPayload };

/**
 * Subscribe to debounced skill-directory changes from the Rust watcher.
 * Returns an unsubscribe function.
 */
export async function onSkillsFsChanged(
  handler: (payload?: SkillsFsChangedPayload) => void,
): Promise<() => void> {
  if (!isTauriApp()) {
    throw unavailableError(
      '技能文件变更订阅',
      '当前不是 Tauri 桌面运行时；请使用桌面应用，或开发时注入 mock backend',
    );
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<SkillsFsChangedPayload>(SKILLS_FS_CHANGED_EVENT, (event) => {
      handler(event.payload);
    });
    return unlisten;
  } catch (error) {
    throw unavailableError(
      '技能文件变更订阅',
      error instanceof Error ? error.message : String(error),
    );
  }
}
