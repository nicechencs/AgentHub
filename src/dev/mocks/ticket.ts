/**
 * Mock ticket wallet: aggregate accounts + providers → tickets;
 * is_current + profiles → bindings. Generated providers are excluded.
 * Keep lockstep with crates/agenthub-core TicketReadService derive rules.
 * Generated projections and leftover 本机路由 providers are not tickets.
 */
import {
  adapterCommandError,
  type AdapterApplyPlan,
  type AdapterApplyRequest,
  type AdapterApplyResult,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfile,
  type AdapterRoute,
  type BindingRoute,
  type BindTicketResult,
  type TicketCredentialClass,
  type TicketPort,
  type TicketSurface,
  type TicketView,
  type TicketWallet,
  type BindingView,
  groupTicketSurfaceMembers,
  memberHealthFromAuthHealth,
} from '@/lib/backend/contracts';
import { authDisplayForAccount } from '@/lib/backend/contracts/auth-state';
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
  applyAdapter?(request: AdapterApplyRequest): Promise<AdapterApplyResult>;
  removeBinding?(profileId: string): void;
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
  'openai-api',
  'xai-api',
  'glm-coding-plan',
  'deepseek-api',
  'codex-chatgpt-subscription',
  'claude-subscription',
  'grok-xai-subscription',
  'unknown',
];

const OPENAI_API_ENDPOINT_NEEDLE = 'api.openai.com';
const XAI_API_ENDPOINT_NEEDLE = 'api.x.ai';
const GLM_CODING_ANTHROPIC_NEEDLE = 'open.bigmodel.cn/api/anthropic';
const GLM_CODING_CHAT_NEEDLE = 'open.bigmodel.cn/api/coding';
const DEEPSEEK_API_ENDPOINT_NEEDLE = 'api.deepseek.com';

function blobContains(value: unknown, needle: string): boolean {
  if (typeof value === 'string') return value.toLowerCase().includes(needle);
  if (!value || typeof value !== 'object') return false;
  return JSON.stringify(value).toLowerCase().includes(needle);
}

function explicitTagMatches(tag: string | undefined, accepted: readonly string[]): boolean {
  return !!tag && accepted.some((item) => item.toLowerCase() === tag.toLowerCase());
}

const PROJECTION_NOT_A_TICKET = '投影不是登录';

function persistedSurface(blob: unknown): TicketSurface | undefined {
  const raw = jsonString(blob, 'surface');
  if (!raw) return undefined;
  if (raw === 'unknown') return undefined;
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
  const tag = provider.preset
    ?? jsonString((provider as ClassifiableProvider).meta, 'provider');
  if (
    explicitTagMatches(tag, ['openai', 'openai-api'])
    || blobContains(provider.configText, OPENAI_API_ENDPOINT_NEEDLE)
  ) {
    return 'openai-api';
  }
  if (
    explicitTagMatches(tag, ['xai', 'xai-api'])
    || blobContains(provider.configText, XAI_API_ENDPOINT_NEEDLE)
  ) {
    return 'xai-api';
  }
  if (
    explicitTagMatches(tag, ['glm-coding-plan'])
    || blobContains(provider.configText, GLM_CODING_ANTHROPIC_NEEDLE)
    || blobContains(provider.configText, GLM_CODING_CHAT_NEEDLE)
  ) {
    return 'glm-coding-plan';
  }
  if (
    explicitTagMatches(tag, ['deepseek-api', 'deepseek'])
    || blobContains(provider.configText, DEEPSEEK_API_ENDPOINT_NEEDLE)
  ) {
    return 'deepseek-api';
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

  if (account.agentId === 'claude' && account.kind === 'oauth') {
    return 'claude-subscription';
  }
  if (account.agentId === 'grok' && account.kind === 'oauth') {
    return 'grok-xai-subscription';
  }
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
  if (
    account.kind === 'apikey'
    && (explicitTagMatches(explicitProvider, ['openai', 'openai-api'])
      || blobContains(row.credentials, OPENAI_API_ENDPOINT_NEEDLE)
      || blobContains(row.extra, OPENAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'openai-api';
  }
  if (
    account.kind === 'apikey'
    && (explicitTagMatches(explicitProvider, ['xai', 'xai-api'])
      || blobContains(row.credentials, XAI_API_ENDPOINT_NEEDLE)
      || blobContains(row.extra, XAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'xai-api';
  }
  if (
    account.kind === 'apikey'
    && (explicitTagMatches(explicitProvider, ['glm-coding-plan'])
      || blobContains(row.credentials, GLM_CODING_ANTHROPIC_NEEDLE)
      || blobContains(row.credentials, GLM_CODING_CHAT_NEEDLE)
      || blobContains(row.extra, GLM_CODING_ANTHROPIC_NEEDLE)
      || blobContains(row.extra, GLM_CODING_CHAT_NEEDLE))
  ) {
    return 'glm-coding-plan';
  }
  if (
    account.kind === 'apikey'
    && (explicitTagMatches(explicitProvider, ['deepseek-api', 'deepseek'])
      || blobContains(row.credentials, DEEPSEEK_API_ENDPOINT_NEEDLE)
      || blobContains(row.extra, DEEPSEEK_API_ENDPOINT_NEEDLE))
  ) {
    return 'deepseek-api';
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
  if (surface === 'openai-api') return ['openai-chat'];
  if (surface === 'xai-api') return ['openai-responses', 'openai-chat'];
  if (surface === 'glm-coding-plan' || surface === 'deepseek-api') {
    return ['anthropic-messages', 'openai-chat'];
  }
  if (surface === 'codex-chatgpt-subscription') return ['openai-responses', 'openai-codex-pkce'];
  if (surface === 'claude-subscription') return ['anthropic-messages', 'anthropic-pkce'];
  if (surface === 'grok-xai-subscription') {
    return ['openai-responses', 'openai-chat', 'xai-device-code'];
  }
  return [];
}

function ticketId(kind: 'account' | 'provider', id: string): string {
  return `${kind}:${id}`;
}

const AGENTHUB_BRIDGE_SLUG = /agenthub_[^\s"'\\]*_bridge/i;

function providerIsNotATicket(
  provider: Provider,
  generatedIds: ReadonlySet<string>,
): boolean {
  if (generatedIds.has(provider.id)) return true;
  const meta = (provider as ClassifiableProvider).meta;
  if (meta?.generatedBy === 'adapter') return true;
  const haystack = `${provider.id}\n${provider.name}\n${provider.configText ?? ''}`;
  return AGENTHUB_BRIDGE_SLUG.test(haystack);
}

function rejectIfProjection(
  ticketIdValue: string,
  sourceKind: 'account' | 'provider',
  sourceId: string,
  resolver: MockTicketSourceResolver,
): void {
  if (sourceKind !== 'provider') return;
  const generated = generatedProviderIds(resolver.listProfiles());
  const provider = resolver.listProviders().find((row) => row.id === sourceId);
  if (provider && providerIsNotATicket(provider, generated)) {
    throw adapterCommandError({
      code: 'invalid_arg',
      message: `${PROJECTION_NOT_A_TICKET}: ${ticketIdValue}`,
      retryable: false,
    });
  }
}

function requireTicketSource(sourceKind: 'account' | 'provider', sourceId: string): void {
  if (sourceKind === 'account' && !getMockAccountById(sourceId)) {
    throw adapterCommandError({
      code: 'not_found',
      message: `account not found: ${sourceId}`,
      retryable: false,
    });
  }
  if (sourceKind === 'provider' && !getMockProviderById(sourceId)) {
    throw adapterCommandError({
      code: 'not_found',
      message: `provider not found: ${sourceId}`,
      retryable: false,
    });
  }
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

/** Returns null when the source ticket is missing (no ghost binding). */
function bindingFromProfile(
  profile: AdapterProfile,
  active: boolean,
  ticketIds: ReadonlySet<string>,
  resolver: MockTicketSourceResolver,
): BindingView | null {
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
  const ticketProviders = allProviders.filter((p) => !providerIsNotATicket(p, generatedIds));

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
    if (!ticketIds.has(tid)) {
      if (providerIsNotATicket(provider, generatedIds)) {
        activeByAgent.delete(provider.agentId);
      }
      continue;
    }
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

  return {
    tickets,
    bindings,
    surfaceGroups: attachSurfaceMemberHealth(
      groupTicketSurfaceMembers(tickets),
      accounts,
    ),
  };
}

function attachSurfaceMemberHealth(
  groups: TicketWallet['surfaceGroups'],
  accounts: readonly Account[],
): TicketWallet['surfaceGroups'] {
  const accountById = new Map(accounts.map((account) => [account.id, account]));
  return groups.map((group) => ({
    ...group,
    members: group.members.map((member) => {
      if (member.health) return member;
      if (member.sourceKind === 'account') {
        const account = accountById.get(member.sourceId);
        if (account) {
          return {
            ...member,
            health: memberHealthFromAuthHealth(authDisplayForAccount(account).health),
          };
        }
        return { ...member, health: 'needs_login' as const };
      }
      return { ...member, health: 'renewable' as const };
    }),
  }));
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
      rejectIfProjection(ticketIdValue, sourceKind, sourceId, resolver);
      requireTicketSource(sourceKind, sourceId);
      return resolver.planAdapter({ sourceKind, sourceId, targetAgentId });
    },
    async bind(ticketIdValue, targetAgentId): Promise<BindTicketResult> {
      await delay(15);
      const { sourceKind, sourceId } = parseTicketId(ticketIdValue);
      rejectIfProjection(ticketIdValue, sourceKind, sourceId, resolver);
      requireTicketSource(sourceKind, sourceId);
      const plan = await resolver.planAdapter({ sourceKind, sourceId, targetAgentId });
      if (!plan.canApply) {
        throw adapterCommandError({
          code: 'unsupported',
          message: '当前适配路径尚不可应用',
          retryable: false,
        });
      }
      if (!resolver.applyAdapter) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'ticket bind is not wired',
          retryable: false,
        });
      }
      await resolver.applyAdapter({ sourceKind, sourceId, targetAgentId });
      const wallet = buildWallet(resolver);
      const binding = wallet.bindings.find(
        (row) => row.ticketId === ticketIdValue && row.agentId === targetAgentId && row.active,
      );
      if (!binding) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: '绑定未成为该 Agent 的当前连接',
          retryable: false,
        });
      }
      return { binding };
    },
    async unbind(ticketIdValue, agentId) {
      await delay(15);
      const { sourceKind, sourceId } = parseTicketId(ticketIdValue);
      const profile = resolver.listProfiles().find(
        (row) =>
          row.sourceKind === sourceKind
          && row.sourceId === sourceId
          && row.targetAgentId === agentId,
      );
      const walletBinding = buildWallet(resolver).bindings.find(
        (row) => row.ticketId === ticketIdValue && row.agentId === agentId,
      );
      const profileId = profile?.id ?? walletBinding?.profileId;
      if (!profileId) {
        throw adapterCommandError({
          code: 'not_found',
          message: `binding not found: ${ticketIdValue} → ${agentId}`,
          retryable: false,
        });
      }
      if (!resolver.removeBinding) {
        throw adapterCommandError({
          code: 'unsupported',
          message: 'ticket unbind is not wired',
          retryable: false,
        });
      }
      resolver.removeBinding(profileId);
    },
  };
}

/** Pure helper for tests: build wallet without delay. */
export function buildMockTicketWallet(resolver: MockTicketSourceResolver): TicketWallet {
  return buildWallet(resolver);
}
