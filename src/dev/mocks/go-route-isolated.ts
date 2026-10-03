import type {
  GoRouteIsolatedPort,
  GoRouteIsolatedStatus,
} from '@/lib/backend/contracts/go-route-isolated';
import { delay } from '@/dev/mocks/delay';

const MOCK_HOME = '/tmp/agenthub-go-route-isolated/mock';
const MOCK_PORT = 18765;

function stopped(): GoRouteIsolatedStatus {
  return {
    state: 'stopped',
    listenReady: false,
    port: null,
    lastError: null,
    home: null,
  };
}

function snapshot(status: GoRouteIsolatedStatus): GoRouteIsolatedStatus {
  return { ...status };
}

/** Per-backend in-memory status. Each createBackend() gets a fresh port. */
export function createMockGoRouteIsolatedPort(): GoRouteIsolatedPort {
  let status = stopped();

  return {
    async start() {
      if (status.state === 'starting' || status.state === 'ready') {
        return snapshot(status);
      }
      status = {
        state: 'starting',
        listenReady: false,
        port: null,
        lastError: null,
        home: MOCK_HOME,
      };
      await delay(80);
      if (status.state !== 'starting') return snapshot(status);
      status = {
        state: 'ready',
        listenReady: true,
        port: MOCK_PORT,
        lastError: null,
        home: MOCK_HOME,
      };
      return snapshot(status);
    },
    async stop() {
      status = stopped();
      return snapshot(status);
    },
    async status() {
      return snapshot(status);
    },
  };
}
