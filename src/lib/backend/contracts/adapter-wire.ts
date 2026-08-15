import type { AgentId, Provider } from '@/lib/types';
import {
  mapCoreProvider,
  type CoreProvider,
} from './provider-map';
import type {
  AdapterAction,
  AdapterApplyPlan,
  AdapterApplyResult,
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterEvidence,
  AdapterGateKind,
  AdapterMaturity,
  AdapterPlanChange,
  AdapterProfile,
  AdapterProfileMode,
  AdapterProfileStatus,
  AdapterRoute,
  AdapterRouteAnalysis,
  AdapterServiceImpact,
  AdapterSourceKind,
  AdapterSupport,
} from './adapter';

/** Exact camelCase shape serialized by Rust's `AdapterProfile`. */
export interface AdapterProfileWire {
  id: string;
  name: string;
  sourceKind: AdapterSourceKind;
  sourceId: string;
  targetAgentId: AgentId;
  route: string;
  mode: string;
  status: string;
  ruleId: string;
  ruleVersion: string;
  generatedProviderId?: string | null;
  localPort?: number | null;
  autoStart: boolean;
  lastErrorCode?: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Exact camelCase shape serialized by Rust's `Provider`. */
export interface CoreProviderWire {
  id: string;
  agentId: AgentId;
  name: string;
  settingsConfig: Record<string, unknown>;
  meta: Record<string, unknown>;
  isCurrent: boolean;
  createdAt?: string;
  updatedAt?: string;
}

/** Exact camelCase shape serialized by Rust's `AdapterApplyResult`. */
export interface AdapterApplyResultWire {
  profile: AdapterProfileWire;
  provider: CoreProviderWire;
}

/** Exact camelCase shape serialized by Tauri's `AdapterBridgeStatusDto`. */
export interface AdapterBridgeStatusDtoWire {
  profileId: string;
  port: number | null;
  running: boolean;
  state: string;
  upstreamStatus: string;
  sourceConnectionId?: string;
  startedAtUnixMs?: number;
}

export interface AdapterActionWire {
  kind: string;
  target: string;
  description: string;
  value?: string;
  secret: boolean;
}

export interface AdapterEvidenceWire {
  label: string;
  url: string;
  verifiedAt: string;
}

export interface AdapterRouteAnalysisWire {
  route: string;
  support: string;
  reason: string;
  actions: AdapterActionWire[];
  limitations: string[];
  evidence: AdapterEvidenceWire[];
  ruleId?: string | null;
  gateKind?: string | null;
}

export interface AdapterPlanChangeWire {
  target: string;
  field: string;
  value?: string;
  secret: boolean;
}

export interface AdapterApplyPlanWire {
  analysis: AdapterRouteAnalysisWire;
  targetAgentId: AgentId;
  canApply: boolean;
  maturity?: string;
  reason?: string;
  serviceImpact: string;
  changes: AdapterPlanChangeWire[];
}

function invalidWireValue(field: string, value: unknown): never {
  throw new Error(`Invalid adapter wire ${field}: ${String(value)}`);
}

function mapSourceKind(value: AdapterSourceKind): AdapterSourceKind {
  if (value === 'account' || value === 'provider') return value;
  return invalidWireValue('sourceKind', value);
}

function mapRoute(value: string): AdapterRoute {
  if (value === 'native_endpoint' || value === 'local_bridge' || value === 'config_sync' || value === 'unsupported') {
    return value;
  }
  return invalidWireValue('route', value);
}

function mapProfileRoute(value: string): Exclude<AdapterRoute, 'unsupported'> {
  const route = mapRoute(value);
  if (route === 'unsupported') return invalidWireValue('profile.route', value);
  return route;
}

function mapProfileMode(value: string): AdapterProfileMode {
  if (value === 'api' || value === 'oauth') return value;
  return invalidWireValue('profile.mode', value);
}

function mapSupport(value: string): AdapterSupport {
  if (value === 'stable' || value === 'experimental' || value === 'unsupported') return value;
  return invalidWireValue('support', value);
}

function mapMaturity(value: string | null | undefined): AdapterMaturity {
  if (value === 'stable' || value === 'experimental' || value === 'preview' || value === 'none') {
    return value;
  }
  return 'none';
}

function mapGateKind(value: string | null | undefined): AdapterGateKind {
  if (
    value == null
    || value === ''
    || value === 'none'
  ) {
    return 'none';
  }
  if (
    value === 'preview_only'
    || value === 'subscription_candidate'
    || value === 'unsupported'
  ) {
    return value;
  }
  // Unknown future gate kinds fail closed to generic unsupported chrome.
  return 'unsupported';
}

function mapActionKind(value: string): AdapterAction['kind'] {
  if (
    value === 'set_config'
    || value === 'set_env'
    || value === 'reference_connection_secret'
    || value === 'requires_local_bridge'
  ) {
    return value;
  }
  return invalidWireValue('action.kind', value);
}

function mapServiceImpact(value: string): AdapterServiceImpact {
  if (value === 'none' || value === 'requires_local_bridge') return value;
  return invalidWireValue('serviceImpact', value);
}

function mapProfileStatus(value: string): AdapterProfileStatus {
  if (value === 'applying' || value === 'active' || value === 'needs_attention') return value;
  // A new desktop lifecycle state must never be displayed as an active projection.
  return 'needs_attention';
}

function mapBridgeState(value: string): AdapterBridgeRuntimeState {
  if (
    value === 'starting'
    || value === 'running'
    || value === 'stopping'
    || value === 'stopped'
    || value === 'error'
    || value === 'degraded'
  ) {
    return value;
  }
  // Unknown runtime states are unsafe to treat as live; keep the UI fail-closed.
  return 'error';
}

/**
 * Whitelist upstream health labels. Unknown future values fail closed to
 * `unknown` so the UI never invents connectivity it cannot prove.
 *
 * Desktop DTO emits unknown|connected|stopped|degraded|unavailable. Keep room
 * for future labels without inventing connectivity.
 */
function mapUpstreamStatus(value: string | undefined | null): string {
  if (
    value === 'unknown'
    || value === 'connected'
    || value === 'stopped'
    || value === 'degraded'
    || value === 'unavailable'
  ) {
    return value;
  }
  return 'unknown';
}

function mapAction(wire: AdapterActionWire): AdapterAction {
  const kind = mapActionKind(wire.kind);
  if (wire.secret) {
    return { kind, target: wire.target, description: wire.description, secret: true };
  }
  return {
    kind,
    target: wire.target,
    description: wire.description,
    ...(typeof wire.value === 'string' ? { value: wire.value } : {}),
    secret: false,
  };
}

function mapPlanChange(wire: AdapterPlanChangeWire): AdapterPlanChange {
  if (wire.secret) {
    return { target: wire.target, field: wire.field, secret: true };
  }
  return {
    target: wire.target,
    field: wire.field,
    ...(typeof wire.value === 'string' ? { value: wire.value } : {}),
    secret: false,
  };
}

function mapEvidence(wire: AdapterEvidenceWire): AdapterEvidence {
  return { label: wire.label, url: wire.url, verifiedAt: wire.verifiedAt };
}

function isLoopbackPort(value: number | null | undefined): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value > 0 && value <= 65_535;
}

function mapStartedAt(value: number | undefined): string | null {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return null;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

export function mapAdapterProfile(wire: AdapterProfileWire): AdapterProfile {
  return {
    id: wire.id,
    name: wire.name,
    sourceKind: mapSourceKind(wire.sourceKind),
    sourceId: wire.sourceId,
    targetAgentId: wire.targetAgentId,
    route: mapProfileRoute(wire.route),
    mode: mapProfileMode(wire.mode),
    status: mapProfileStatus(wire.status),
    ruleId: wire.ruleId,
    ruleVersion: wire.ruleVersion,
    generatedProviderId: wire.generatedProviderId ?? null,
    localPort: isLoopbackPort(wire.localPort) ? wire.localPort : null,
    autoStart: wire.autoStart,
    lastErrorCode: wire.lastErrorCode ?? null,
    createdAt: wire.createdAt,
    updatedAt: wire.updatedAt,
  };
}

export function mapAdapterRouteAnalysis(wire: AdapterRouteAnalysisWire): AdapterRouteAnalysis {
  const ruleId = typeof wire.ruleId === 'string' && wire.ruleId.trim() ? wire.ruleId : null;
  return {
    route: mapRoute(wire.route),
    support: mapSupport(wire.support),
    reason: wire.reason,
    actions: wire.actions.map(mapAction),
    limitations: [...wire.limitations],
    evidence: wire.evidence.map(mapEvidence),
    ruleId,
    gateKind: mapGateKind(wire.gateKind),
  };
}

export function mapAdapterApplyPlan(wire: AdapterApplyPlanWire): AdapterApplyPlan {
  const analysis = mapAdapterRouteAnalysis(wire.analysis);
  const reason = typeof wire.reason === 'string' && wire.reason.trim()
    ? wire.reason
    : analysis.reason;
  return {
    analysis,
    targetAgentId: wire.targetAgentId,
    canApply: wire.canApply,
    maturity: mapMaturity(wire.maturity),
    reason,
    serviceImpact: mapServiceImpact(wire.serviceImpact),
    changes: wire.changes.map(mapPlanChange),
  };
}

function asCoreProvider(wire: CoreProviderWire): CoreProvider {
  return {
    id: wire.id,
    agentId: wire.agentId,
    name: wire.name,
    settingsConfig: wire.settingsConfig,
    meta: wire.meta,
    isCurrent: wire.isCurrent,
    createdAt: wire.createdAt,
    updatedAt: wire.updatedAt,
  };
}

export function mapAdapterApplyResult(wire: AdapterApplyResultWire): AdapterApplyResult {
  const provider: Provider = mapCoreProvider(asCoreProvider(wire.provider));
  return {
    profile: mapAdapterProfile(wire.profile),
    provider,
  };
}

export function mapAdapterBridgeStatusDto(
  wire: AdapterBridgeStatusDtoWire,
): AdapterBridgeRuntimeStatus {
  const port = isLoopbackPort(wire.port) ? wire.port : null;
  return {
    profileId: wire.profileId,
    state: mapBridgeState(wire.state),
    port,
    endpoint: port ? `http://127.0.0.1:${port}/v1` : null,
    startedAt: mapStartedAt(wire.startedAtUnixMs),
    upstreamStatus: mapUpstreamStatus(wire.upstreamStatus),
  };
}
