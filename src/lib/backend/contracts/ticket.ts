/**
 * Ticket / Binding read model + bind/unbind write (docs/connection-binding-model.md §2 / §4).
 * Wire shapes match Tauri `list_ticket_wallet` / `plan_ticket` / `bind_ticket` / `unbind_ticket`.
 */
import type { AgentId } from '@/lib/types';
import type { AdapterApplyPlan, AdapterRoute } from './adapter';
import {
  mapAdapterApplyPlan,
  type AdapterApplyPlanWire,
} from './adapter-wire';
import type { AuthHealth } from './auth-state';

/** Product surface recognized at import time (or unknown). */
export type TicketSurface =
  | 'kimi-code-membership'
  | 'anthropic-api'
  | 'openai-api'
  | 'xai-api'
  | 'glm-coding-plan'
  | 'deepseek-api'
  | 'codex-chatgpt-subscription'
  | 'claude-subscription'
  | 'grok-xai-subscription'
  | 'unknown';

/** Credential family on the ticket (UI filter chips). */
export type TicketCredentialClass = 'api_key' | 'oauth' | 'unknown';

/** Binding route in the domain model (not AdapterRoute wire names). */
export type BindingRoute = 'native' | 'reshape' | 'bridge';

/**
 * Plan-route → binding-route. `native` is never synthesized here; it only
 * comes from a current login/provider row with no profile.
 */
export function adapterRouteToBinding(route: AdapterRoute): BindingRoute | null {
  if (route === 'local_bridge') return 'bridge';
  if (route === 'config_sync' || route === 'native_endpoint') return 'reshape';
  return null;
}

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
  /** Bound loopback port; null when the listener has no valid port yet. */
  port: number | null;
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

/**
 * Picker snapshot health (RFC §3.2). Optional on the C1 wire so old callers
 * keep working; when absent the UI overlays account-row AuthHealth.
 */
export type TicketMemberHealth = 'renewable' | 'needs_login' | 'try_once';

/** One known-surface wallet row inside a §5.5 poll pool. */
export interface TicketSurfaceMemberView {
  ticketId: string;
  sourceKind: TicketSourceKind;
  sourceId: string;
  agentId: AgentId;
  label: string;
  /** Present when mock / future live wire attaches picker health. */
  health?: TicketMemberHealth;
}

/** Same `(surface, credentialClass)` members. Unknown surfaces are omitted. */
export interface TicketSurfaceGroupView {
  surface: TicketSurface;
  credentialClass: TicketCredentialClass;
  members: TicketSurfaceMemberView[];
}

export interface TicketWallet {
  tickets: TicketView[];
  bindings: BindingView[];
  surfaceGroups: TicketSurfaceGroupView[];
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
  port: number | null;
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

export interface TicketSurfaceMemberViewWire {
  ticketId: string;
  sourceKind: string;
  sourceId: string;
  agentId: AgentId;
  label: string;
  health?: string;
}

export interface TicketSurfaceGroupViewWire {
  surface: string;
  credentialClass: string;
  members: TicketSurfaceMemberViewWire[];
}

export interface TicketWalletWire {
  tickets: TicketViewWire[];
  bindings: BindingViewWire[];
  surfaceGroups?: TicketSurfaceGroupViewWire[];
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
    || value === 'openai-api'
    || value === 'xai-api'
    || value === 'glm-coding-plan'
    || value === 'deepseek-api'
    || value === 'codex-chatgpt-subscription'
    || value === 'claude-subscription'
    || value === 'grok-xai-subscription'
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
  const port = typeof wire.port === 'number' && isLoopbackPort(wire.port) ? wire.port : null;
  return { port, running: wire.running === true };
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

/** Accept camel, snake, or Rust enum names; unknown values stay unset. */
export function mapTicketMemberHealth(value: unknown): TicketMemberHealth | undefined {
  if (typeof value !== 'string') return undefined;
  const normalized = value.trim().toLowerCase().replace(/[- ]/g, '_');
  if (normalized === 'renewable') return 'renewable';
  if (normalized === 'needs_login' || normalized === 'needslogin') return 'needs_login';
  if (normalized === 'try_once' || normalized === 'tryonce') return 'try_once';
  return undefined;
}

/** RFC §3.2: AuthHealth → picker health. Unknown / NeedsAttention = one try. */
export function memberHealthFromAuthHealth(
  health?: AuthHealth | null,
): TicketMemberHealth {
  if (health === 'needs_login' || health === 'missing') return 'needs_login';
  if (health === 'unknown') return 'try_once';
  return 'renewable';
}

export function ticketMemberHealthLabel(health: TicketMemberHealth): string {
  if (health === 'needs_login') return '需要重新登录';
  if (health === 'try_once') return '可试一次';
  return '可用';
}

export function isIsolatedMemberHealth(health: TicketMemberHealth): boolean {
  return health === 'needs_login';
}

export function surfaceGroupForTicketId(
  groups: readonly TicketSurfaceGroupView[],
  ticketId: string,
): TicketSurfaceGroupView | undefined {
  return groups.find((group) => group.members.some((member) => member.ticketId === ticketId));
}

export function surfaceGroupMemberCount(
  groups: readonly TicketSurfaceGroupView[],
  ticketId: string,
): number {
  const count = surfaceGroupForTicketId(groups, ticketId)?.members.length ?? 0;
  return count > 0 ? count : 1;
}

export function mapTicketSurfaceMember(wire: TicketSurfaceMemberViewWire): TicketSurfaceMemberView {
  const health = mapTicketMemberHealth(wire.health);
  return {
    ticketId: wire.ticketId,
    sourceKind: mapSourceKind(wire.sourceKind),
    sourceId: wire.sourceId,
    agentId: wire.agentId,
    label: typeof wire.label === 'string' ? wire.label : '',
    ...(health ? { health } : {}),
  };
}

export function mapTicketSurfaceGroup(wire: TicketSurfaceGroupViewWire): TicketSurfaceGroupView {
  return {
    surface: mapSurface(wire.surface),
    credentialClass: mapCredentialClass(wire.credentialClass),
    members: (wire.members ?? []).map(mapTicketSurfaceMember),
  };
}

/**
 * Group known-surface tickets by `(surface, credentialClass)`.
 * Lockstep with Rust `group_ticket_surface_members`: skip unknown surface /
 * unknown credential class; mix account+provider; sort members by ticket id.
 */
export function groupTicketSurfaceMembers(
  tickets: readonly TicketView[],
): TicketSurfaceGroupView[] {
  const buckets = new Map<string, TicketView[]>();
  for (const ticket of tickets) {
    if (ticket.surface === 'unknown' || ticket.credentialClass === 'unknown') continue;
    const key = `${ticket.surface}\0${ticket.credentialClass}`;
    const list = buckets.get(key);
    if (list) list.push(ticket);
    else buckets.set(key, [ticket]);
  }
  return [...buckets.keys()]
    .sort((left, right) => left.localeCompare(right))
    .flatMap((key) => {
      const members = (buckets.get(key) ?? [])
        .slice()
        .sort((left, right) => left.id.localeCompare(right.id));
      const first = members[0];
      if (!first) return [];
      return [{
        surface: first.surface,
        credentialClass: first.credentialClass,
        members: members.map((ticket) => ({
          ticketId: ticket.id,
          sourceKind: ticket.sourceKind,
          sourceId: ticket.sourceId,
          agentId: ticket.agentId,
          label: ticket.label,
        })),
      }];
    });
}

export function mapTicketWallet(wire: TicketWalletWire): TicketWallet {
  const tickets = (wire.tickets ?? []).map(mapTicketView);
  const bindings = (wire.bindings ?? []).map(mapBindingView);
  const surfaceGroups = Array.isArray(wire.surfaceGroups)
    ? wire.surfaceGroups.map(mapTicketSurfaceGroup)
    : groupTicketSurfaceMembers(tickets);
  return { tickets, bindings, surfaceGroups };
}

const BIND_RESULT_UNREADABLE = '绑定结果无法识别，请重试';
const PLAN_RESULT_UNREADABLE = '连接方案无法识别，请重试';
const UNBIND_RESULT_UNREADABLE = '停止并还原结果无法识别，请重试';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/** Live Rust `TicketBinding` is identified by these two fields at any wrap level. */
function hasTicketAndAgent(value: unknown): value is BindingViewWire {
  if (!isRecord(value)) return false;
  return typeof value.ticketId === 'string' && typeof value.agentId === 'string';
}

function isAdapterApplyPlanWire(value: unknown): value is AdapterApplyPlanWire {
  if (!isRecord(value)) return false;
  return isRecord(value.analysis) && typeof value.targetAgentId === 'string';
}

/**
 * `plan_ticket` live Rust returns a top-level `AdapterApplyPlan`.
 * Mocks / older fixtures may wrap it as `{ plan }`. Accept both.
 */
export type PlanTicketResultWire = AdapterApplyPlanWire | { plan: AdapterApplyPlanWire };

/** `plan_ticket` returns the same apply-plan shape as `plan_adapter`. */
export function mapPlanTicketResult(wire: PlanTicketResultWire): AdapterApplyPlan {
  try {
    if (isAdapterApplyPlanWire(wire)) {
      return mapAdapterApplyPlan(wire);
    }
    if (isRecord(wire) && isAdapterApplyPlanWire(wire.plan)) {
      return mapAdapterApplyPlan(wire.plan);
    }
    throw new Error(PLAN_RESULT_UNREADABLE);
  } catch (error) {
    if (error instanceof Error && error.message === PLAN_RESULT_UNREADABLE) throw error;
    throw new Error(PLAN_RESULT_UNREADABLE);
  }
}

/**
 * `bind_ticket` live Rust returns a top-level `TicketBinding`
 * (`ticketId` / `agentId` / `route` / …). Mocks and older fixtures wrap it as
 * `{ binding }`. Accept both. Presence of `ticketId` + `agentId` means the
 * object itself is the binding.
 */
export type BindTicketResultWire = { binding: BindingViewWire } | BindingViewWire;

export interface BindTicketResult {
  binding: BindingView;
}

function bindingWireFrom(wire: unknown): BindingViewWire | null {
  if (hasTicketAndAgent(wire)) return wire;
  if (isRecord(wire) && hasTicketAndAgent(wire.binding)) return wire.binding;
  return null;
}

export function mapBindTicketResult(wire: BindTicketResultWire): BindTicketResult {
  try {
    const binding = bindingWireFrom(wire);
    if (!binding) throw new Error(BIND_RESULT_UNREADABLE);
    return { binding: mapBindingView(binding) };
  } catch (error) {
    if (error instanceof Error && error.message === BIND_RESULT_UNREADABLE) throw error;
    throw new Error(BIND_RESULT_UNREADABLE);
  }
}

/**
 * `unbind_ticket` may return `{}`, `null`, or an updated wallet.
 * Callers that need the wallet should `listWallet()` after unbind.
 */
export function mapUnbindTicketResult(wire: unknown): void {
  if (wire == null) return;
  if (typeof wire !== 'object') {
    throw new Error(UNBIND_RESULT_UNREADABLE);
  }
}

/** Stable ticket id: `account:<row-id>` / `provider:<row-id>`. */
export function ticketIdFor(sourceKind: TicketSourceKind, sourceId: string): string {
  return `${sourceKind}:${sourceId}`;
}

/** Success criterion for bind: this Agent's active binding. */
export function isActiveBindingForAgent(
  binding: BindingView,
  targetAgentId: AgentId,
): boolean {
  return binding.active === true && binding.agentId === targetAgentId;
}

/**
 * Confirm-apply success. Hosted local_bridge persist does not occupy
 * the target's current provider; bind still succeeded if the profile exists.
 */
export function isBindSuccessForAgent(
  binding: BindingView,
  targetAgentId: AgentId,
): boolean {
  if (binding.agentId !== targetAgentId) return false;
  if (binding.active === true) return true;
  return binding.route === 'bridge' && Boolean(binding.profileId);
}

export interface TicketPort {
  /** Global ticket wallet + bindings (read-only aggregation). */
  listWallet(): Promise<TicketWallet>;
  /** Plan bind(ticket, agent); same surface as adapter.plan. */
  plan(ticketId: string, targetAgentId: AgentId): Promise<AdapterApplyPlan>;
  /** Bind ticket → agent. Success is the returned active binding. */
  bind(ticketId: string, targetAgentId: AgentId): Promise<BindTicketResult>;
  /** Unbind ticket from agent. Ticket remains; caller may listWallet. */
  unbind(ticketId: string, agentId: AgentId): Promise<void>;
}

/** Route label for Connections usage line and Dashboard card meta. */
export function bindingRouteUsageLabel(route: BindingRoute): string {
  return bindingRouteDashboardLabel(route);
}

/** Route label for Dashboard card meta. */
export function bindingRouteDashboardLabel(route: BindingRoute): string {
  if (route === 'reshape') return '改配置';
  if (route === 'bridge') return '本机路由';
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
  if (surface === 'anthropic-api') return 'API';
  if (surface === 'openai-api') return 'OpenAI';
  if (surface === 'xai-api') return 'xAI';
  if (surface === 'glm-coding-plan') return 'GLM';
  if (surface === 'deepseek-api') return 'DeepSeek';
  if (surface === 'codex-chatgpt-subscription') return '订阅';
  if (surface === 'claude-subscription' || surface === 'grok-xai-subscription') return '订阅';
  return '未识别';
}
