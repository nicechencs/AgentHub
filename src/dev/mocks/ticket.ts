/**
 * Mock ticket wallet: aggregate accounts + providers → tickets;
 * is_current + profiles → bindings. Generated providers are excluded.
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

function adapterRouteToBinding(route: Exclude<AdapterRoute, 'unsupported'>): BindingRoute {
  if (route === 'local_bridge') return 'bridge';
  // config_sync + native_endpoint are reshape (same protocol, different config shape).
  if (route === 'config_sync' || route === 'native_endpoint') return 'reshape';
  return 'native';
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
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return 'codex-chatgpt-subscription';
  }
  return 'unknown';
}

function credentialClassOfAccount(account: Account): TicketCredentialClass {
  if (account.kind === 'oauth') return 'oauth';
  if (account.kind === 'apikey') return 'api_key';
  return 'unknown';
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
  return {
    id: ticketId('account', account.id),
    sourceKind: 'account',
    sourceId: account.id,
    agentId: account.agentId,
    label: account.label,
    surface: classifyAccountSurface(account),
    credentialClass: credentialClassOfAccount(account),
    speaks: [],
    importedFrom: null,
  };
}

function providerToTicket(provider: Provider): TicketView {
  const surface = classifyProviderSurface(provider);
  return {
    id: ticketId('provider', provider.id),
    sourceKind: 'provider',
    sourceId: provider.id,
    agentId: provider.agentId,
    label: provider.name,
    surface,
    credentialClass: 'api_key',
    speaks: surface === 'kimi-code-membership' || surface === 'anthropic-api'
      ? ['anthropic-messages']
      : [],
    importedFrom: null,
  };
}

function generatedProviderIds(profiles: readonly AdapterProfile[]): Set<string> {
  const ids = new Set<string>();
  for (const profile of profiles) {
    if (profile.generatedProviderId) ids.add(profile.generatedProviderId);
  }
  return ids;
}

function buildWallet(resolver: MockTicketSourceResolver): TicketWallet {
  const profiles = resolver.listProfiles();
  const generatedIds = generatedProviderIds(profiles);
  const accounts = resolver.listAccounts();
  const providers = resolver.listProviders().filter((p) => !generatedIds.has(p.id));

  const tickets: TicketView[] = [
    ...accounts.map(accountToTicket),
    ...providers.map(providerToTicket),
  ];

  const bindings: BindingView[] = [];
  const seenActive = new Set<string>();

  for (const account of accounts) {
    if (!account.isCurrent) continue;
    const tid = ticketId('account', account.id);
    const key = `${account.agentId}`;
    if (seenActive.has(key)) continue;
    seenActive.add(key);
    bindings.push({
      ticketId: tid,
      agentId: account.agentId,
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    });
  }

  for (const provider of providers) {
    if (!provider.isCurrent) continue;
    const tid = ticketId('provider', provider.id);
    const key = `${provider.agentId}`;
    // Account current wins for demote semantics when both claim current —
    // still record native binding only if no active binding for this agent yet.
    if (seenActive.has(key)) continue;
    seenActive.add(key);
    bindings.push({
      ticketId: tid,
      agentId: provider.agentId,
      route: 'native',
      active: true,
      profileId: null,
      bridge: null,
    });
  }

  for (const profile of profiles) {
    if (profile.sourceKind === 'provider' && generatedIds.has(profile.sourceId)) continue;
    const tid = ticketId(profile.sourceKind, profile.sourceId);
    const generated = profile.generatedProviderId
      ? resolver.listProviders().find((p) => p.id === profile.generatedProviderId)
      : undefined;
    const active = Boolean(generated?.isCurrent);
    if (active) {
      // Replace native binding for this agent if generated provider is current.
      const idx = bindings.findIndex((b) => b.agentId === profile.targetAgentId && b.active);
      if (idx >= 0) bindings.splice(idx, 1);
      seenActive.add(profile.targetAgentId);
    }
    const bridgeStatus = profile.route === 'local_bridge'
      ? resolver.getBridgeStatus(profile.id)
      : undefined;
    bindings.push({
      ticketId: tid,
      agentId: profile.targetAgentId,
      route: adapterRouteToBinding(profile.route),
      active,
      profileId: profile.id,
      bridge: profile.route === 'local_bridge'
        ? (() => {
            const port = profile.localPort
              ?? bridgeStatus?.port
              ?? null;
            if (typeof port !== 'number' || port <= 0) return null;
            return {
              port,
              running: bridgeStatus?.state === 'running' || bridgeStatus?.state === 'starting',
            };
          })()
        : null,
    });
  }

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
