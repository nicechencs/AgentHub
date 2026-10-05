import { getBackend } from '@/app/runtime';
import type { GoRouteIsolatedStatus } from '@/lib/backend/contracts/go-route-isolated';

export type {
  GoRouteEdgeErrorCode,
  GoRouteEdgeStatus,
  GoRouteEdgeSurface,
  GoRouteIsolatedPort,
  GoRouteIsolatedState,
  GoRouteIsolatedStatus,
} from '@/lib/backend/contracts/go-route-isolated';
export { shouldApplyGoRouteResult } from '@/lib/backend/contracts/go-route-isolated';

export async function startGoRouteIsolated(): Promise<GoRouteIsolatedStatus> {
  return getBackend().goRouteIsolated.start();
}

export async function stopGoRouteIsolated(): Promise<GoRouteIsolatedStatus> {
  return getBackend().goRouteIsolated.stop();
}

export async function getGoRouteIsolatedStatus(): Promise<GoRouteIsolatedStatus> {
  return getBackend().goRouteIsolated.status();
}
