import type { GoRouteIsolatedStatus } from '@/lib/backend/contracts/go-route-isolated';
import type { TranslateFn } from '@/lib/i18n';

/** Supervisor text from status_with_reload_error. The process can stay ready. */
export const GO_ISOLATED_CONFIG_UPDATE_ERROR =
  'Go route configuration could not be updated';

/** Error line under the dev Go route strip. Null means do not add a line. */
export function goIsolatedVisibleError(
  status: Pick<GoRouteIsolatedStatus, 'state' | 'lastError'>,
  t: TranslateFn,
): string | null {
  if (status.state === 'stopped') return null;
  const error = status.lastError?.trim() ?? '';
  if (!error) return null;
  if (error === GO_ISOLATED_CONFIG_UPDATE_ERROR) {
    return t('routes.board.goIsolatedUpdateFailed');
  }
  return error;
}

/**
 * A healthy status poll replaces lastError. Keep the config-update failure
 * while the process is still not stopped, until start/stop returns a new status.
 */
export function retainGoIsolatedConfigError<
  T extends Pick<GoRouteIsolatedStatus, 'state' | 'lastError'>,
>(current: T, next: T): T {
  if (current.state === 'stopped' || next.state === 'stopped') return next;
  if (next.lastError?.trim()) return next;
  if (current.lastError?.trim() !== GO_ISOLATED_CONFIG_UPDATE_ERROR) return next;
  return { ...next, lastError: current.lastError };
}
