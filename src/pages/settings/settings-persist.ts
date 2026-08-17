import { updateSettings } from '@/lib/api/settings';
import type { AppSettings } from '@/lib/types';

/**
 * Persist a settings patch immediately. Callers apply optimistic UI first;
 * on failure this returns null so they can revert.
 */
export async function persistSettingsPatch(
  patch: Partial<AppSettings>,
): Promise<AppSettings> {
  return updateSettings(patch);
}
