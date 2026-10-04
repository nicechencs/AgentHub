import type {
  GoRouteIsolatedPort,
  GoRouteIsolatedStatus,
} from '@/lib/backend/contracts/go-route-isolated';
import { invoke } from './invoke';

export function createTauriGoRouteIsolatedPort(): GoRouteIsolatedPort {
  return {
    start() {
      return invoke<GoRouteIsolatedStatus>('start_go_route_isolated');
    },
    stop() {
      return invoke<GoRouteIsolatedStatus>('stop_go_route_isolated');
    },
    status() {
      return invoke<GoRouteIsolatedStatus>('get_go_route_isolated_status');
    },
  };
}
