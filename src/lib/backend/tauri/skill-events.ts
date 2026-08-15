/**
 * Tauri-only skill filesystem events.
 * Only Tauri ports may import this module.
 */
import type { SkillsFsChangedPayload } from '@/lib/backend/contracts/skill-types';
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
    return () => {};
  }
  try {
    const { listen } = await import('@tauri-apps/api/event');
    const unlisten = await listen<SkillsFsChangedPayload>(SKILLS_FS_CHANGED_EVENT, (event) => {
      handler(event.payload);
    });
    return unlisten;
  } catch {
    return () => {};
  }
}
