import type { AgentKey } from '@/lib/types';
import retryableErrorContract from './retryable-error-contract.json';

/** Saved connection table selected for a read-only adapter route preview. */
export type AdapterSourceKind = 'account' | 'provider';

export interface AdapterRouteRequest {
  sourceKind: AdapterSourceKind;
  /** Database row id; never a label, credential, or config body. */
  sourceId: string;
  targetAgentId: AgentKey;
}

/** Closed compatibility routes; unknown wire values are rejected at the adapter boundary. */
export type AdapterRoute = 'native_endpoint' | 'local_bridge' | 'config_sync' | 'unsupported';

/** User-facing reuse family derived from the planner route. */
export type AdapterReusePath = 'api_endpoint' | 'native_subscription' | 'local_bridge' | 'none';

/**
 * Persisted credential family for a profile. Orthogonal to `sourceKind` (table) and `route`
 * (projection). API Key conversions stay `api` even when they use a local protocol bridge.
 */
export type AdapterProfileMode = 'api' | 'oauth';

/** A rule can be stable, experimental, or explicitly unsupported. */
export type AdapterSupport = 'stable' | 'experimental' | 'unsupported';

/**
 * Planner four-tier maturity of a graph edge.
 * Distinct from `support` (matrix confidence) and `canApply` (write now).
 */
export type AdapterMaturity = 'stable' | 'experimental' | 'preview' | 'none';

/**
 * Structured gate / presentation class from core analyze.
 * Prefer this over parsing `reason` text. Does not authorize writes.
 */
export type AdapterGateKind =
  | 'none'
  | 'preview_only'
  | 'subscription_candidate'
  | 'unsupported';

export type AdapterActionKind =
  | 'set_config'
  | 'set_env'
  | 'reference_connection_secret'
  | 'requires_local_bridge';

/** Safe action preview. A secret action only refers to the selected Connection. */
export type AdapterAction =
  | {
      kind: AdapterActionKind;
      target: string;
      description: string;
      value?: string;
      secret: false;
    }
  | {
      kind: AdapterActionKind;
      target: string;
      description: string;
      secret: true;
      value?: never;
    };

/** Official, date-stamped basis for the compatibility conclusion. */
export interface AdapterEvidence {
  label: string;
  url: string;
  verifiedAt: string;
}

/** Safe core response: no secret values or raw configuration are present. */
export interface AdapterRouteAnalysis {
  route: AdapterRoute;
  support: AdapterSupport;
  reason: string;
  actions: AdapterAction[];
  limitations: string[];
  evidence: AdapterEvidence[];
  /** Capability-matrix rule id when a cell matched. */
  ruleId?: string | null;
  /** Structured gate class for UI chrome; prefer over parsing reason. */
  gateKind?: AdapterGateKind;
}

export type AdapterServiceImpact = 'none' | 'requires_local_bridge';

/** One future config mutation. A secret change has no serializable value. */
export type AdapterPlanChange =
  | { target: string; field: string; value?: string; secret: false }
  | { target: string; field: string; secret: true; value?: never };

/** Safe preview of the eventual configuration mutation. `plan()` is the only planner exit. */
export interface AdapterApplyPlan {
  analysis: AdapterRouteAnalysis;
  targetAgentId: AgentKey;
  /**
   * True only when the capability matrix is open **and** plan's private
   * `write_gate` allows a write now (Account stays false). Matrix alone
   * never authorizes writes.
   */
  canApply: boolean;
  /** Four-tier edge maturity. Missing on older wires → treat as `none`. */
  maturity?: AdapterMaturity;
  /** Planner-derived presentation path. Missing on older wires → derive from route. */
  reusePath?: AdapterReusePath;
  /**
   * Planner-facing reason. Prefer this over `analysis.reason` when
   * `canApply` is false (same-edge Account adds an explicit write-gate note).
   */
  reason?: string;
  serviceImpact: AdapterServiceImpact;
  changes: AdapterPlanChange[];
}

/** Persisted lifecycle state for a credential-free adapter projection. */
export type AdapterProfileStatus = 'applying' | 'active' | 'needs_attention';

/** A saved adapter projection. It identifies connections but never serializes their secrets. */
export interface AdapterProfile {
  id: string;
  name: string;
  sourceKind: AdapterSourceKind;
  sourceId: string;
  targetAgentId: AgentKey;
  route: Exclude<AdapterRoute, 'unsupported'>;
  /** Credential family: API Key conversion vs official-login proxy. Independent of `route`. */
  mode: AdapterProfileMode;
  status: AdapterProfileStatus;
  ruleId: string;
  ruleVersion: string;
  generatedProviderId?: string | null;
  /** Bound loopback port for a local bridge; direct routes leave this empty. */
  localPort?: number | null;
  /** Restored by the desktop host after launch when this is a local bridge. */
  autoStart: boolean;
  lastErrorCode?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Credential-free, in-process state of one local bridge listener. */
export type AdapterBridgeRuntimeState =
  | 'starting'
  | 'running'
  | 'stopping'
  | 'stopped'
  | 'error'
  | 'degraded';

export type AdapterBridgeUpstreamStatus =
  | 'unknown'
  | 'connected'
  | 'stopped'
  | 'degraded'
  | 'unavailable';

/** One inbound local-route request. Never includes Authorization, bodies, or keys. */
export interface AdapterBridgeInboundRequest {
  at: string;
  method: string;
  path: string;
  status: number;
  ok: boolean;
}

export type RouteTraceStageStatus = 'pending' | 'ok' | 'failed' | 'skipped';

export interface RouteTraceMember {
  label: string;
  sourceKind: string;
  sourceId: string;
  ticketId?: string | null;
}

export interface RouteTracePoolAttempt {
  member: RouteTraceMember;
  status: RouteTraceStageStatus;
  code?: string | null;
  message?: string | null;
}

export interface RouteTraceLocalAuth {
  status: RouteTraceStageStatus;
  profileId?: string | null;
  port?: number | null;
  code?: string | null;
  message?: string | null;
}

export interface RouteTracePool {
  status: RouteTraceStageStatus;
  selectedMember?: RouteTraceMember | null;
  attempts?: RouteTracePoolAttempt[];
  code?: string | null;
  message?: string | null;
}

export interface RouteTraceConversion {
  status: RouteTraceStageStatus;
  path: string;
  result?: string | null;
  code?: string | null;
  message?: string | null;
}

export interface RouteTraceUpstreamAuth {
  status: RouteTraceStageStatus;
  httpStatus?: number | null;
  code?: string | null;
  message?: string | null;
}

export interface RouteTraceUpstream {
  status: RouteTraceStageStatus;
  url?: string | null;
  member?: RouteTraceMember | null;
  model?: string | null;
  upstreamModel?: string | null;
  httpStatus?: number | null;
  code?: string | null;
  message?: string | null;
}

/** One completed local-route request trace for monitoring. Credential-free. */
export interface AdapterBridgeRouteTrace {
  requestId: string;
  at: string;
  profileId?: string | null;
  method: string;
  path: string;
  httpStatus: number;
  ok: boolean;
  model?: string | null;
  latencyMs?: number | null;
  ttftMs?: number | null;
  inputTokens?: number | null;
  outputTokens?: number | null;
  localAuth: RouteTraceLocalAuth;
  pool: RouteTracePool;
  conversion: RouteTraceConversion;
  upstreamAuth: RouteTraceUpstreamAuth;
  upstream: RouteTraceUpstream;
  failureStage?: string | null;
}

/** Loopback listener status. `localToken` is the bearer that actually authenticates. */
export interface AdapterBridgeRuntimeStatus {
  profileId: string;
  state: AdapterBridgeRuntimeState;
  port?: number | null;
  endpoint?: string | null;
  startedAt?: string | null;
  upstreamStatus?: AdapterBridgeUpstreamStatus | string | null;
  /** Newest first. Empty when no tool has connected yet. */
  recentInbound?: AdapterBridgeInboundRequest[];
  /** Newest first. Per-request route traces for monitoring. */
  recentRouteTraces?: AdapterBridgeRouteTrace[];
  /** Authenticated inbound requests since this process started (not ring-capped). */
  totalRequestCount?: number;
  /** Failed authenticated inbound requests since this process started. */
  failedRequestCount?: number;
  /** ISO time of the latest authenticated inbound request this process has seen. */
  lastRequestAt?: string | null;
  /** Loopback bearer (`ahb_…`) accepted by this listener. */
  localToken?: string | null;
}

export type AdapterApplyRequest = AdapterRouteRequest;

/** Generated provider is safe: it contains a connection reference, never a credential value. */
export interface AdapterApplyResult {
  profile: AdapterProfile;
  provider: import('@/lib/types').Provider;
}

export interface AdapterProfileFilter {
  sourceKind?: AdapterSourceKind;
  sourceId?: string;
  targetAgentId?: AgentKey;
  mode?: AdapterProfileMode;
  route?: Exclude<AdapterRoute, 'unsupported'>;
  status?: AdapterProfileStatus;
  autoStart?: boolean;
}

/** Structured Adapter command error shared by Tauri and mock. */
export interface AdapterCommandErrorFields {
  code: string;
  message: string;
  details?: string | null;
  retryable: boolean;
}

export class AdapterCommandError extends Error implements AdapterCommandErrorFields {
  readonly code: string;
  readonly details?: string | null;
  readonly retryable: boolean;

  constructor(fields: AdapterCommandErrorFields) {
    super(fields.message);
    this.name = 'AdapterCommandError';
    this.code = fields.code;
    this.details = fields.details ?? null;
    this.retryable = fields.retryable;
  }
}

/** Shared with desktop `is_adapter_error_retryable` via retryable-error-contract.json. */
export function isAdapterErrorCodeRetryable(code: string): boolean {
  if (retryableErrorContract.retryableExact.some((item) => item === code)) return true;
  return retryableErrorContract.retryablePrefixes.some((prefix) => code.startsWith(prefix));
}

export function adapterCommandError(fields: {
  code: string;
  message: string;
  details?: string | null;
  retryable?: boolean;
}): AdapterCommandError {
  return new AdapterCommandError({
    code: fields.code,
    message: fields.message,
    details: fields.details ?? null,
    retryable: fields.retryable ?? isAdapterErrorCodeRetryable(fields.code),
  });
}

export type RoutePoolSurface = 'messages' | 'responses' | 'chat_completions';
export type RoutePoolDialect = 'claude' | 'codex' | 'grok' | 'kimi' | 'dsh' | 'generic';

export type MemberAvailability = 'ready' | 'cooling' | 'isolated' | 'disabled';

/** Credential-free member row. Never includes login secrets or Hub tokens. */
export interface RouteMemberOverview {
  id?: string;
  sourceKind: AdapterSourceKind;
  sourceId: string;
  /** Safe account/provider label; never a credential or source-id fallback. */
  displayLabel?: string;
  /** Masked OAuth refresh-token tail, e.g. `**1234`; never the raw token. */
  refreshTokenTail?: string;
  enabled: boolean;
  priority?: number;
  availability?: MemberAvailability;
}

/** Credential-free default-pool overview. Never includes `hubToken`. */
export interface DefaultRoutePoolOverview {
  id: string;
  targetAgentId: AgentKey;
  surface: RoutePoolSurface;
  dialect: RoutePoolDialect;
  unifiedGatewayEnrolled: boolean;
  gatewayPort?: number | null;
  members: RouteMemberOverview[];
  listedModels?: string[];
}

export interface DefaultRoutePoolList {
  enabled: boolean;
  pools: DefaultRoutePoolOverview[];
  /** Kimi and DSH share one chat-completions local token when true. */
  chatCompletionsShared?: boolean;
}

export interface AttachPoolOwnedAuthorizationRequest {
  sourceKind: AdapterSourceKind;
  sourceId: string;
  targetAgentId: AgentKey;
  surface: RoutePoolSurface;
}

/** One credential-free Connections row selected for route-pool enrollment. */
export interface SyncConnectionSource {
  sourceKind: AdapterSourceKind;
  sourceId: string;
}

/** Optional selection for route-pool enrollment; omitted means all eligible rows. */
export interface SyncConnectionAuthorizationsRequest {
  sources: SyncConnectionSource[];
}

export interface SyncConnectionAuthorizationsResult {
  added: number;
  skipped: number;
}

/** Shared local-gateway status for the board switch. */
export type LocalGatewayStatus = {
  running: boolean;
  port: number | null;
  statuses: AdapterBridgeRuntimeStatus[];
  /** Local-auth failures without a bound route (newest first). */
  unauthenticatedTraces?: AdapterBridgeRouteTrace[];
  /** True while restore or start is bringing local forwarding back. */
  restarting: boolean;
};

/** Loopback bearer for the tokens page. */
export type LocalTokenRecord = {
  id: string;
  poolId: string;
  token: string;
  name: string;
  primary: boolean;
};

/** Result of a tokens-page model-path probe with an entry key. */
export type LocalTokenProbeOutcome =
  | 'ok'
  | 'unauthorized'
  | 'unreachable'
  | 'rejected'
  | 'invalid';

export type SourceModelCatalogSource = 'live' | 'custom' | 'empty';

export type SourceModelCatalog = {
  models: string[];
  source: SourceModelCatalogSource;
  canCustomize: boolean;
};

export type LocalTokenProbeResult = {
  outcome: LocalTokenProbeOutcome;
  httpStatus: number | null;
  latencyMs: number;
  upstreamStatus: string | null;
  requestUrl: string | null;
  requestMethod: string | null;
  requestBody: string | null;
  responseBody: string | null;
  errorMessage: string | null;
};

export interface AdapterPort {
  analyze(request: AdapterRouteRequest): Promise<AdapterRouteAnalysis>;
  plan(request: AdapterRouteRequest): Promise<AdapterApplyPlan>;
  listProfiles(filter?: AdapterProfileFilter): Promise<AdapterProfile[]>;
  listDefaultRoutePools(): Promise<DefaultRoutePoolList>;
  listLocalTokens(): Promise<LocalTokenRecord[]>;
  listLocalTokenModels(token: string): Promise<string[]>;
  refreshLocalTokenModels(token: string): Promise<string[]>;
  setLocalTokenCustomModels(token: string, models: string[]): Promise<string[]>;
  ensureSourceModelCatalog(
    sourceKind: AdapterSourceKind,
    sourceId: string,
  ): Promise<SourceModelCatalog>;
  setSourceCustomModels(
    sourceKind: AdapterSourceKind,
    sourceId: string,
    models: string[],
  ): Promise<SourceModelCatalog>;
  testLocalToken(
    endpoint: string,
    token: string,
    path: string,
    model?: string | null,
  ): Promise<LocalTokenProbeResult>;
  setLocalToken(poolId: string, token: string): Promise<LocalTokenRecord>;
  createLocalToken(poolId: string, name: string): Promise<LocalTokenRecord>;
  setLocalTokenName(id: string, name: string): Promise<LocalTokenRecord>;
  deleteLocalToken(id: string): Promise<void>;
  setChatCompletionsShared(shared: boolean): Promise<DefaultRoutePoolList>;
  attachPoolOwnedAuthorization(
    request: AttachPoolOwnedAuthorizationRequest,
  ): Promise<DefaultRoutePoolOverview>;
  setRouteAuthorizationEnabled(
    sourceKind: AdapterSourceKind,
    sourceId: string,
    enabled: boolean,
  ): Promise<number>;
  setRouteAuthorizationPriority(
    sourceKind: AdapterSourceKind,
    sourceId: string,
    priority: number,
  ): Promise<number>;
  removeRouteAuthorization(
    sourceKind: AdapterSourceKind,
    sourceId: string,
  ): Promise<number>;
  recycleRouteMembership(
    sourceKind: AdapterSourceKind,
    sourceId: string,
  ): Promise<number>;
  syncConnectionAuthorizations(
    request?: SyncConnectionAuthorizationsRequest,
  ): Promise<SyncConnectionAuthorizationsResult>;
  enrollNativeToGateway(profileId: string): Promise<DefaultRoutePoolOverview>;
  apply(request: AdapterApplyRequest): Promise<AdapterApplyResult>;
  remove(profileId: string): Promise<void>;
  startBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  stopBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  getBridgeStatus(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  setBridgeAutoStart(profileId: string, autoStart: boolean): Promise<AdapterProfile>;
  startLocalGateway(): Promise<LocalGatewayStatus>;
  stopLocalGateway(): Promise<LocalGatewayStatus>;
  getLocalGatewayStatus(): Promise<LocalGatewayStatus>;
}
