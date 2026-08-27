import type { AgentId } from '@/lib/types';
import retryableErrorContract from './retryable-error-contract.json';

/** Saved connection table selected for a read-only adapter route preview. */
export type AdapterSourceKind = 'account' | 'provider';

export interface AdapterRouteRequest {
  sourceKind: AdapterSourceKind;
  /** Database row id; never a label, credential, or config body. */
  sourceId: string;
  targetAgentId: AgentId;
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
  targetAgentId: AgentId;
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
  targetAgentId: AgentId;
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

/** Deliberately excludes the local bearer and all upstream credentials. */
export interface AdapterBridgeRuntimeStatus {
  profileId: string;
  state: AdapterBridgeRuntimeState;
  port?: number | null;
  endpoint?: string | null;
  startedAt?: string | null;
  upstreamStatus?: AdapterBridgeUpstreamStatus | string | null;
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
  targetAgentId?: AgentId;
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

/** Credential-free member row. Never includes login secrets or Hub tokens. */
export interface RouteMemberOverview {
  sourceKind: AdapterSourceKind;
  sourceId: string;
  enabled: boolean;
}

/** Credential-free default-pool overview. Never includes `hubToken`. */
export interface DefaultRoutePoolOverview {
  id: string;
  targetAgentId: AgentId;
  surface: RoutePoolSurface;
  dialect: RoutePoolDialect;
  v2Enrolled: boolean;
  gatewayPort?: number | null;
  members: RouteMemberOverview[];
  listedModels?: string[];
}

export interface DefaultRoutePoolList {
  enabled: boolean;
  pools: DefaultRoutePoolOverview[];
}

export interface AdapterPort {
  analyze(request: AdapterRouteRequest): Promise<AdapterRouteAnalysis>;
  plan(request: AdapterRouteRequest): Promise<AdapterApplyPlan>;
  listProfiles(filter?: AdapterProfileFilter): Promise<AdapterProfile[]>;
  listDefaultRoutePools(): Promise<DefaultRoutePoolList>;
  enrollNativeToGateway(profileId: string): Promise<DefaultRoutePoolOverview>;
  apply(request: AdapterApplyRequest): Promise<AdapterApplyResult>;
  remove(profileId: string): Promise<void>;
  startBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  stopBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  getBridgeStatus(profileId: string): Promise<AdapterBridgeRuntimeStatus>;
  setBridgeAutoStart(profileId: string, autoStart: boolean): Promise<AdapterProfile>;
}
