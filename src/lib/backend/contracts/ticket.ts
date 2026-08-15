/**
 * Ticket / Binding read model (docs/connection-binding-model.md §2 / §5–§6 step 1).
 * Wire shapes match Tauri `list_ticket_wallet` / `plan_ticket`.
 */
import type { AgentId } from '@/lib/types';
import type { AdapterApplyPlan } from './adapter';
import {
  mapAdapterApplyPlan,
  type AdapterApplyPlanWire,
} from './adapter-wire';

/** Product surface recognized at import time (or unknown). */
export type TicketSurface =
  | 'kimi-code-membership'
  | 'anthropic-api'
  | 'codex-chatgpt-subscription'
  | 'unknown';

/** Credential family on the ticket (UI filter chips). */
export type TicketCredentialClass = 'api_key' | 'oauth' | 'unknown';

/** Binding route in the domain model (not AdapterRoute wire names). */
export type BindingRoute = 'native' | 'reshape' | 'bridge';

export type TicketSourceKind = 'account' | 'provider';

/** One authorization ticket in the global wallet. */
export interface TicketView {
  /** Stable id: `provider:<row-id>` or `account:<row-id>`. */
  id: string;
  sourceKind: TicketSourceKind;
  sourceId: string;
  /** Agent that owns the underlying account/provider row. */
  agentId: AgentId;
  label: string;
  surface: TicketSurface;
  credentialClass: TicketCredentialClass;
  speaks: string[];
  importedFrom: AgentId | null;
}

/** Optional bridge runtime snapshot on an active bridge binding. */
export interface BindingBridgeRuntime {
  port: number;
  running: boolean;
}

/** One ticket→agent binding from the read model. */
export interface BindingView {
  ticketId: string;
  agentId: AgentId;
  route: BindingRoute;
  active: boolean;
  profileId: string | null;
  bridge: BindingBridgeRuntime | null;
}

export interface TicketWallet {
  tickets: TicketView[];
  bindings: BindingView[];
}

/** Exact camelCase shape from Rust `list_ticket_wallet`. */
export interface TicketViewWire {
  id: string;
  sourceKind: string;
  sourceId: string;
  agentId: AgentId;
  label: string;
  surface: string;
  credentialClass: string;
  speaks: string[];
  importedFrom?: string | null;
}

export interface BindingBridgeRuntimeWire {
  port: number;
  running: boolean;
}

export interface BindingViewWire {
  ticketId: string;
  agentId: AgentId;
  route: string;
  active: boolean;
  profileId?: string | null;
  bridge?: BindingBridgeRuntimeWire | null;
}

export interface TicketWalletWire {
  tickets: TicketViewWire[];
  bindings: BindingViewWire[];
}

function invalidWireValue(field: string, value: unknown): never {
  throw new Error(`Invalid ticket wire ${field}: ${String(value)}`);
}

function mapSourceKind(value: string): TicketSourceKind {
  if (value === 'account' || value === 'provider') return value;
  return invalidWireValue('sourceKind', value);
}

function mapSurface(value: string): TicketSurface {
  if (
    value === 'kimi-code-membership'
    || value === 'anthropic-api'
    || value === 'codex-chatgpt-subscription'
    || value === 'unknown'
  ) {
    return value;
  }
  // Unknown future surfaces fail closed to unknown (still visible in wallet).
  return 'unknown';
}

function mapCredentialClass(value: string): TicketCredentialClass {
  if (value === 'api_key' || value === 'oauth' || value === 'unknown') return value;
  return 'unknown';
}

function mapBindingRoute(value: string): BindingRoute {
  if (value === 'native' || value === 'reshape' || value === 'bridge') return value;
  return invalidWireValue('route', value);
}

function isLoopbackPort(value: number): boolean {
  return Number.isInteger(value) && value > 0 && value <= 65_535;
}

function mapBridge(wire: BindingBridgeRuntimeWire | null | undefined): BindingBridgeRuntime | null {
  if (wire == null) return null;
  if (!isLoopbackPort(wire.port)) return null;
  return { port: wire.port, running: wire.running === true };
}

export function mapTicketView(wire: TicketViewWire): TicketView {
  const importedFrom =
    typeof wire.importedFrom === 'string' && wire.importedFrom.trim()
      ? (wire.importedFrom as AgentId)
      : null;
  return {
    id: wire.id,
    sourceKind: mapSourceKind(wire.sourceKind),
    sourceId: wire.sourceId,
    agentId: wire.agentId,
    label: wire.label,
    surface: mapSurface(wire.surface),
    credentialClass: mapCredentialClass(wire.credentialClass),
    speaks: Array.isArray(wire.speaks) ? wire.speaks.map(String) : [],
    importedFrom,
  };
}

export function mapBindingView(wire: BindingViewWire): BindingView {
  return {
    ticketId: wire.ticketId,
    agentId: wire.agentId,
    route: mapBindingRoute(wire.route),
    active: wire.active === true,
    profileId: typeof wire.profileId === 'string' && wire.profileId.trim() ? wire.profileId : null,
    bridge: mapBridge(wire.bridge),
  };
}

export function mapTicketWallet(wire: TicketWalletWire): TicketWallet {
  return {
    tickets: (wire.tickets ?? []).map(mapTicketView),
    bindings: (wire.bindings ?? []).map(mapBindingView),
  };
}

/** `plan_ticket` returns the same apply-plan wire as `plan_adapter`. */
export function mapPlanTicketResult(wire: AdapterApplyPlanWire): AdapterApplyPlan {
  return mapAdapterApplyPlan(wire);
}

export interface TicketPort {
  /** Global ticket wallet + bindings (read-only aggregation). */
  listWallet(): Promise<TicketWallet>;
  /** Plan bind(ticket, agent); same surface as adapter.plan. */
  plan(ticketId: string, targetAgentId: AgentId): Promise<AdapterApplyPlan>;
}

/** Route label for Connections「正用于」line. */
export function bindingRouteUsageLabel(route: BindingRoute): string {
  if (route === 'reshape') return '改配置';
  if (route === 'bridge') return '本机桥';
  return '切换';
}

/** Route label for Dashboard card meta. */
export function bindingRouteDashboardLabel(route: BindingRoute): string {
  if (route === 'reshape') return '改配置';
  if (route === 'bridge') return '本机桥';
  return '直连';
}

/** Credential-class chip label. */
export function ticketCredentialClassLabel(cls: TicketCredentialClass): string {
  if (cls === 'oauth') return '官方登录';
  if (cls === 'api_key') return 'API Key';
  return '未识别';
}

/** Surface chip label (short). */
export function ticketSurfaceLabel(surface: TicketSurface): string {
  if (surface === 'kimi-code-membership') return '会员';
  if (surface === 'anthropic-api') return '官方';
  if (surface === 'codex-chatgpt-subscription') return '订阅';
  return '未识别';
}
