export type GoRouteIsolatedState = 'stopped' | 'starting' | 'ready' | 'failed';
export interface GoRouteIsolatedStatus {
  state: GoRouteIsolatedState;
  listenReady: boolean;
  port: number | null;
  lastError: string | null;
  home: string | null;
}
export interface GoRouteIsolatedPort {
  start(): Promise<GoRouteIsolatedStatus>;
  stop(): Promise<GoRouteIsolatedStatus>;
  status(): Promise<GoRouteIsolatedStatus>;
}
