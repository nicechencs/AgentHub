export type GoRouteIsolatedState = 'stopped' | 'starting' | 'ready' | 'failed';
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
