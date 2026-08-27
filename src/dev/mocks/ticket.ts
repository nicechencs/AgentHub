/**
 * Mock ticket wallet: aggregate accounts + providers → tickets;
 * is_current + profiles → bindings. Generated providers are excluded.
 * Keep lockstep with crates/agenthub-core TicketReadService derive rules.
 * Generated projections and leftover 本机路由 providers are not tickets.
 * Unpersisted surfaces come from planAdapter sourceProduct, not a second classify.
 */
import {
  adapterCommandError,
  type AdapterApplyPlan,
  type AdapterApplyRequest,
  type AdapterApplyResult,
  type AdapterBridgeRuntimeStatus,
  type AdapterProfile,
  type BindTicketResult,
  type TicketCredentialClass,
  type TicketPort,
  type TicketSurface,
  type TicketView,
  type TicketWallet,
  type BindingView,
  adapterRouteToBinding,
  groupTicketSurfaceMembers,
  memberHealthFromAuthHealth,
} from '@/lib/backend/contracts';
import { authDisplayForAccount } from '@/lib/backend/contracts/auth-state';
import { ticketSurfaceSpeaks } from '@/lib/backend/contracts/ticket-speaks';
import type { Account, AgentId, Provider } from '@/lib/types';
import type { ClassifyProductId } from '@/lib/backend/contracts/source-classify-contract';
import { delay } from './delay';
import { getMockAccountById } from './account';
import { getMockProviderById } from './provider';
import { sourceProductOfPlan } from './adapter/source-product';
import {
  jsonString,
  type ClassifiableAccount,
  type ClassifiableProvider,
} from './source-classify';

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

const PROJECTION_NOT_A_TICKET = '自动生成的配置不是登录';

function persistedSurface(blob: unknown): TicketSurface | undefined {
  const raw = jsonString(blob, 'surface');
  if (!raw) return undefined;
  if (raw === 'unknown') return undefined;
  return TICKET_SURFACES.find((surface) => surface === raw);
}

function ticketSurfaceFromProduct(product: ClassifyProductId): TicketSurface {
  if (product === 'other') return 'unknown';
  return TICKET_SURFACES.find((surface) => surface === product) ?? 'unknown';
}

async function surfaceFromPlan(
  resolver: MockTicketSourceResolver,
  sourceKind: 'account' | 'provider',
  sourceId: string,
  targetAgentId: AgentId,
): Promise<TicketSurface> {
  const plan = await resolver.planAdapter({ sourceKind, sourceId, targetAgentId });
  return ticketSurfaceFromProduct(sourceProductOfPlan(plan));
}

function credentialClassOfAccount(account: Account): TicketCredentialClass {
  if (account.kind === 'oauth') return 'oauth';
  if (account.kind === 'apikey') return 'api_key';
  return 'unknown';
}

function speaksOf(surface: TicketSurface): string[] {
  return ticketSurfaceSpeaks(surface);
}

function ticketId(kind: 'account' | 'provider', id: string): string {
  return `${kind}:${id}`;
}

const AGENTHUB_BRIDGE_SLUG = /agenthub_[^\s"'\\]*_bridge/i;

function accountIsProjection(account: Account): boolean {
  const row = account as ClassifiableAccount;
  const haystack = `${JSON.stringify(row.credentials ?? {})}\n${JSON.stringify(row.extra ?? {})}`;
  return /\bahb_/.test(haystack) || AGENTHUB_BRIDGE_SLUG.test(haystack);
}

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
  if (sourceKind === 'account') {
    const account = resolver.listAccounts().find((row) => row.id === sourceId);
    if (account && accountIsProjection(account)) {
      throw adapterCommandError({
        code: 'invalid_arg',
        message: `${PROJECTION_NOT_A_TICKET}: ${ticketIdValue}`,
        retryable: false,
      });
    }
    return;
  }
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

async function accountToTicket(
  account: Account,
  resolver: MockTicketSourceResolver,
): Promise<TicketView> {
  const row = account as ClassifiableAccount;
  const surface = persistedSurface(row.extra)
    ?? await surfaceFromPlan(resolver, 'account', account.id, account.agentId);
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

async function providerToTicket(
  provider: Provider,
  resolver: MockTicketSourceResolver,
): Promise<TicketView> {
  const row = provider as ClassifiableProvider;
  const surface = persistedSurface(row.meta)
    ?? await surfaceFromPlan(resolver, 'provider', provider.id, provider.agentId);
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
  const route = adapterRouteToBinding(profile.route);
  if (route == null) return null;
  return {
    ticketId: tid,
    agentId: profile.targetAgentId,
    route,
    active,
    profileId: profile.id,
    bridge: bridgeFromProfile(profile, resolver),
  };
}

async function buildWallet(resolver: MockTicketSourceResolver): Promise<TicketWallet> {
  const profiles = resolver.listProfiles();
  const generatedIds = generatedProviderIds(profiles);
  const accounts = resolver.listAccounts();
  const allProviders = resolver.listProviders();
  const ticketProviders = allProviders.filter((p) => !providerIsNotATicket(p, generatedIds));

  const tickets: TicketView[] = [
    ...await Promise.all(
      accounts
        .filter((account) => !accountIsProjection(account))
        .map((account) => accountToTicket(account, resolver)),
    ),
    ...await Promise.all(
      ticketProviders.map((provider) => providerToTicket(provider, resolver)),
    ),
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
      return await buildWallet(resolver);
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
      const wallet = await buildWallet(resolver);
      const binding = wallet.bindings.find(
        (row) => row.ticketId === ticketIdValue && row.agentId === targetAgentId && row.active,
      );
      if (!binding) {
        throw adapterCommandError({
          code: 'invalid_arg',
          message: '还没有切到这份登录',
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
      const walletBinding = (await buildWallet(resolver)).bindings.find(
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

/** Helper for tests: build wallet without the ticket-port delay. */
export function buildMockTicketWallet(
  resolver: MockTicketSourceResolver,
): Promise<TicketWallet> {
  return buildWallet(resolver);
}
