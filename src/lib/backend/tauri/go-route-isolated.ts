import type {
  GoRouteEdgeStatus,
  GoRouteIsolatedPort,
  GoRouteIsolatedStatus,
} from '@/lib/backend/contracts/go-route-isolated';
import { invoke } from './invoke';

type GoRouteIsolatedStatusWire = Omit<GoRouteIsolatedStatus, 'edgeStatuses'> & {
  edgeStatuses?: GoRouteEdgeStatus[];
};

function normalizeStatus(status: GoRouteIsolatedStatusWire): GoRouteIsolatedStatus {
  return {
    ...status,
    // Older isolated supervisors omit this additive field. Keep the desktop
    // contract useful while the shell and sidecar are upgraded independently.
    edgeStatuses: Array.isArray(status.edgeStatuses)
      ? status.edgeStatuses.map((edge) => ({ ...edge }))
      : [],
  };
}

async function invokeStatus(command: string): Promise<GoRouteIsolatedStatus> {
  return normalizeStatus(await invoke<GoRouteIsolatedStatusWire>(command));
}

export function createTauriGoRouteIsolatedPort(): GoRouteIsolatedPort {
  return {
    start() {
      return invokeStatus('start_go_route_isolated');
    },
    stop() {
      return invokeStatus('stop_go_route_isolated');
    },
    status() {
      return invokeStatus('get_go_route_isolated_status');
    },
  };
}
