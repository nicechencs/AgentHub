export type GoRouteIsolatedState = 'stopped' | 'starting' | 'ready' | 'failed';

/** Safe downstream surface reported by the isolated Go runtime. */
export type GoRouteEdgeSurface = 'messages' | 'responses' | 'chat_completions';

/** Fixed failure category reported by the isolated Go runtime. */
export type GoRouteEdgeErrorCode =
  | 'route_busy'
  | 'request_canceled'
  | 'downstream_write_failed'
  | 'invalid_request'
  | 'request_unauthorized'
  | 'request_not_found'
  | 'request_too_large'
  | 'upstream_unavailable'
  | 'request_failed';

/**
 * Per-pool aggregate from the isolated Go runtime.
 *
 * This is process-lifetime control state, not persisted usage or an Activity
 * record. It deliberately excludes entry keys, request bodies, login data,
 * upstream text, request ids, member labels, and models.
 */
export interface GoRouteEdgeStatus {
  poolId: string;
  surface: GoRouteEdgeSurface;
  memberCount: number;
  healthyMemberCount: number;
  inFlightCount: number;
  requestSuccessCount: number;
  requestFailureCount: number;
  lastErrorCode: GoRouteEdgeErrorCode | null;
}

export interface GoRouteIsolatedStatus {
  state: GoRouteIsolatedState;
  listenReady: boolean;
  port: number | null;
  lastError: string | null;
  home: string | null;
  lifecycle: string | null;
  inFlightCount: number;
  memberCount: number;
  healthyMemberCount: number;
  edgeStatuses: GoRouteEdgeStatus[];
  recovering: boolean;
  restartCount: number;
}
export interface GoRouteIsolatedPort {
  start(): Promise<GoRouteIsolatedStatus>;
  stop(): Promise<GoRouteIsolatedStatus>;
  status(): Promise<GoRouteIsolatedStatus>;
}

export function shouldApplyGoRouteResult(
  requestGeneration: number,
  currentGeneration: number,
  paused: boolean,
): boolean {
  return !paused && requestGeneration === currentGeneration;
}
