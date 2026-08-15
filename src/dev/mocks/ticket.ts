/**
 * Mock ticket wallet: aggregate accounts + providers → tickets;
 * is_current + profiles → bindings. Generated providers are excluded.
 * Keep lockstep with crates/agenthub-core TicketReadService derive rules.
 */
import type {
  AdapterApplyPlan,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterRoute,
  BindingRoute,
  TicketCredentialClass,
  TicketPort,
  TicketSurface,
  TicketView,
  TicketWallet,
  BindingView,
} from '@/lib/backend/contracts';
import type { Account, AgentId, Provider } from '@/lib/types';
import { delay } from './delay';
import { getMockAccountById } from './account';
import { getMockProviderById } from './provider';

export interface MockTicketSourceResolver {
  listAccounts(): Account[];
  listProviders(): Provider[];
  listProfiles(): AdapterProfile[];
  getBridgeStatus(profileId: string): AdapterBridgeRuntimeStatus | undefined;
  planAdapter(request: {
    sourceKind: 'account' | 'provider';
    sourceId: string;
    targetAgentId: AgentId;
  }): Promise<AdapterApplyPlan>;
}

/** Optional classify fields mirroring core Account.extra / credentials. */
type ClassifiableAccount = Account & {
  extra?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
};

/** Optional persisted meta mirroring core Provider.meta. */
type ClassifiableProvider = Provider & {
  meta?: Record<string, unknown>;
};

const TICKET_SURFACES: readonly TicketSurface[] = [
  'kimi-code-membership',
  'anthropic-api',
  'codex-chatgpt-subscription',
  'unknown',
];

const PROJECTION_NOT_A_TICKET = '投影不是票 / 禁止二次投影';

function persistedSurface(blob: unknown): TicketSurface | undefined {
  const raw = jsonString(blob, 'surface');
  if (!raw) return undefined;
  return TICKET_SURFACES.find((surface) => surface === raw);
}

function adapterRouteToBinding(route: Exclude<AdapterRoute, 'unsupported'>): BindingRoute {
  if (route === 'local_bridge') return 'bridge';
  // config_sync + native_endpoint are reshape (same protocol, different config shape).
  if (route === 'config_sync' || route === 'native_endpoint') return 'reshape';
  return 'native';
}

function jsonString(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = (value as Record<string, unknown>)[key];
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed || undefined;
}

/** Mirror core `is_codex_auth_json`: format=auth_json OR nested tokens.access/refresh. */
function isCodexAuthJson(format: string | undefined, credentials: unknown): boolean {
  if (format?.toLowerCase() === 'auth_json') return true;
  const tokens = credentials && typeof credentials === 'object'
    ? (credentials as Record<string, unknown>).tokens
    : undefined;
  if (!tokens || typeof tokens !== 'object' || Array.isArray(tokens)) return false;
  return Object.prototype.hasOwnProperty.call(tokens, 'access_token')
    || Object.prototype.hasOwnProperty.call(tokens, 'refresh_token');
}

function classifyProviderSurface(provider: Provider): TicketSurface {
  if (
    provider.agentId === 'kimi'
    && (provider.preset === 'kimi-code-membership'
      || (typeof provider.configText === 'string'
        && provider.configText.toLowerCase().includes('api.kimi.com/coding')))
  ) {
    return 'kimi-code-membership';
  }
  if (
    provider.agentId === 'claude'
    && (provider.preset === 'anthropic'
      || (typeof provider.configText === 'string'
        && provider.configText.toLowerCase().includes('api.anthropic.com')))
  ) {
    return 'anthropic-api';
  }
  return 'unknown';
}

function classifyAccountSurface(account: Account): TicketSurface {
  const row = account as ClassifiableAccount;
  const explicitProvider =
    jsonString(row.extra, 'provider')
    ?? jsonString(row.credentials, 'provider')
    ?? account.provider?.trim();
  const credentialFormat =
    jsonString(row.credentials, 'format')
    ?? jsonString(row.extra, 'format')
    ?? account.credentialFormat?.trim();
  const credentialsBlob = row.credentials ?? {};

  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'anthropic'
      || (typeof row.credentials === 'object'
        && JSON.stringify(row.credentials).toLowerCase().includes('api.anthropic.com'))
      || (typeof row.extra === 'object'
        && JSON.stringify(row.extra).toLowerCase().includes('api.anthropic.com')))
  ) {
    return 'anthropic-api';
  }

  // Lockstep with adapter_route_service identify_source Account arm:
  // Codex OAuth + auth_json → subscription; Codex OAuth without auth_json →
  // same product surface (oauth_other credential class in matrix).
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    if (isCodexAuthJson(credentialFormat, credentialsBlob)) {
      return 'codex-chatgpt-subscription';
    }
    return 'codex-chatgpt-subscription';
  }
  return 'unknown';
}

function credentialClassOfAccount(account: Account): TicketCredentialClass {
  if (account.kind === 'oauth') return 'oauth';
  if (account.kind === 'apikey') return 'api_key';
  return 'unknown';
}

/** Lockstep with TicketSurface::speaks in agenthub-core. */
function speaksOf(surface: TicketSurface): string[] {
  if (surface === 'kimi-code-membership') {
    return ['anthropic-messages', 'openai-chat'];
  }
  if (surface === 'anthropic-api') return ['anthropic-messages'];
  if (surface === 'codex-chatgpt-subscription') return ['openai-responses'];
  return [];
}

function ticketId(kind: 'account' | 'provider', id: string): string {
  return `${kind}:${id}`;
}

function parseTicketId(ticketIdValue: string): { sourceKind: 'account' | 'provider'; sourceId: string } {
  const colon = ticketIdValue.indexOf(':');
  if (colon <= 0) {
    throw new Error(`Invalid ticket id: ${ticketIdValue}`);
  }
  const sourceKind = ticketIdValue.slice(0, colon);
  const sourceId = ticketIdValue.slice(colon + 1);
  if ((sourceKind !== 'account' && sourceKind !== 'provider') || !sourceId) {
    throw new Error(`Invalid ticket id: ${ticketIdValue}`);
  }
  return { sourceKind, sourceId };
}

function accountToTicket(account: Account): TicketView {
  const row = account as ClassifiableAccount;
  const surface = persistedSurface(row.extra) ?? classifyAccountSurface(account);
  return {
    id: ticketId('account', account.id),
    sourceKind: 'account',
    sourceId: account.id,
    agentId: account.agentId,
    label: account.label,
    surface,
    credentialClass: credentialClassOfAccount(account),
    speaks: speaksOf(surface),
    importedFrom: account.agentId,
  };
}

function providerToTicket(provider: Provider): TicketView {
  const row = provider as ClassifiableProvider;
  const surface = persistedSurface(row.meta) ?? classifyProviderSurface(provider);
  return {
    id: ticketId('provider', provider.id),
    sourceKind: 'provider',
    sourceId: provider.id,
    agentId: provider.agentId,
    label: provider.name,
    surface,
    credentialClass: 'api_key',
    speaks: speaksOf(surface),
    importedFrom: provider.agentId,
  };
}

function generatedProviderIds(profiles: readonly AdapterProfile[]): Set<string> {
  const ids = new Set<string>();
  for (const profile of profiles) {
    if (profile.generatedProviderId) ids.add(profile.generatedProviderId);
  }
  return ids;
}

function bridgeFromProfile(
  profile: AdapterProfile,
  resolver: MockTicketSourceResolver,
): BindingView['bridge'] {
  if (profile.route !== 'local_bridge') return null;
  const bridgeStatus = resolver.getBridgeStatus(profile.id);
  const port = profile.localPort ?? bridgeStatus?.port ?? null;
  if (typeof port !== 'number' || port <= 0) return null;
  return {
    port,
    running: bridgeStatus?.state === 'running' || bridgeStatus?.state === 'starting',
  };
}

/** Returns null when route unsupported or source ticket missing (no ghost binding). */
function bindingFromProfile(
  profile: AdapterProfile,
  active: boolean,
  ticketIds: ReadonlySet<string>,
  resolver: MockTicketSourceResolver,
): BindingView | null {
  if (profile.route === 'unsupported') return null;
  const tid = ticketId(profile.sourceKind, profile.sourceId);
  if (!ticketIds.has(tid)) return null;
  return {
    ticketId: tid,
    agentId: profile.targetAgentId,
    route: adapterRouteToBinding(profile.route),
    active,
    profileId: profile.id,
    bridge: bridgeFromProfile(profile, resolver),
  };
}

function buildWallet(resolver: MockTicketSourceResolver): TicketWallet {
  const profiles = resolver.listProfiles();
  const generatedIds = generatedProviderIds(profiles);
  const accounts = resolver.listAccounts();
  const allProviders = resolver.listProviders();
  const ticketProviders = allProviders.filter((p) => !generatedIds.has(p.id));

  const tickets: TicketView[] = [
    ...accounts.map(accountToTicket),
    ...ticketProviders.map(providerToTicket),
  ];
  tickets.sort((a, b) => a.id.localeCompare(b.id));

  const ticketIds = new Set(tickets.map((t) => t.id));
  const providerById = new Map(allProviders.map((p) => [p.id, p]));
  const profileByGenerated = new Map<string, AdapterProfile>();
  for (const profile of profiles) {
    if (profile.generatedProviderId) {
      profileByGenerated.set(profile.generatedProviderId, profile);
    }
  }

  // agent → winning active candidate (provider current beats account current).
  const activeByAgent = new Map<AgentId, BindingView>();
  const activeProfileIds = new Set<string>();

  // (a) current accounts → native active candidates (loses to provider current).
  for (const account of accounts) {
    if (!account.isCurrent) continue;
    const tid = ticketId('account', account.id);
    if (!ticketIds.has(tid)) continue;
    if (activeByAgent.has(account.agentId)) continue;
    activeByAgent.set(account.agentId, {
      ticketId: tid,
      agentId: account.agentId,
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    });
  }

  // (b) current providers — provider wins over any account candidate.
  for (const provider of allProviders) {
    if (!provider.isCurrent) continue;
    const profile = profileByGenerated.get(provider.id);
    if (profile) {
      const binding = bindingFromProfile(profile, true, ticketIds, resolver);
      if (binding) {
        if (binding.profileId) activeProfileIds.add(binding.profileId);
        activeByAgent.set(binding.agentId, binding);
      }
      continue;
    }
    const tid = ticketId('provider', provider.id);
    if (!ticketIds.has(tid)) continue;
    activeByAgent.set(provider.agentId, {
      ticketId: tid,
      agentId: provider.agentId,
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    });
  }

  const bindings: BindingView[] = [...activeByAgent.values()];

  // (c) remaining profiles that are not the active current projection.
  for (const profile of profiles) {
    if (activeProfileIds.has(profile.id)) continue;
    const generated = profile.generatedProviderId;
    if (generated) {
      const genProvider = providerById.get(generated);
      if (genProvider?.isCurrent) {
        // Current projection already handled in (b); if skipped (missing source),
        // do not synthesize a ghost inactive binding.
        continue;
      }
    }
    const binding = bindingFromProfile(profile, false, ticketIds, resolver);
    if (binding) bindings.push(binding);
  }

  bindings.sort((a, b) => {
    const agentCmp = a.agentId.localeCompare(b.agentId);
    if (agentCmp !== 0) return agentCmp;
    const ticketCmp = a.ticketId.localeCompare(b.ticketId);
    if (ticketCmp !== 0) return ticketCmp;
    const profileCmp = (a.profileId ?? '').localeCompare(b.profileId ?? '');
    if (profileCmp !== 0) return profileCmp;
    return Number(b.active) - Number(a.active);
  });

  return { tickets, bindings };
}

export function createMockTicketPort(resolver: MockTicketSourceResolver): TicketPort {
  return {
    async listWallet() {
      await delay(15);
      return buildWallet(resolver);
    },
    async plan(ticketIdValue, targetAgentId) {
      await delay(15);
      const { sourceKind, sourceId } = parseTicketId(ticketIdValue);
      if (sourceKind === 'provider') {
        const generated = generatedProviderIds(resolver.listProfiles());
        const provider = resolver.listProviders().find((row) => row.id === sourceId) as
          | ClassifiableProvider
          | undefined;
        if (generated.has(sourceId) || provider?.meta?.generatedBy === 'adapter') {
          throw new Error(`${PROJECTION_NOT_A_TICKET}: ${ticketIdValue}`);
        }
      }
      if (sourceKind === 'account' && !getMockAccountById(sourceId)) {
        throw new Error(`account not found: ${sourceId}`);
      }
      if (sourceKind === 'provider' && !getMockProviderById(sourceId)) {
        throw new Error(`provider not found: ${sourceId}`);
      }
      return resolver.planAdapter({ sourceKind, sourceId, targetAgentId });
    },
  };
}

/** Pure helper for tests: build wallet without delay. */
export function buildMockTicketWallet(resolver: MockTicketSourceResolver): TicketWallet {
  return buildWallet(resolver);
}
