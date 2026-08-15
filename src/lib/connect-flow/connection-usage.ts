/**
 * Connections 钱包行用途反查（docs/hub-redesign-plan.md §3.1C）。
 */
import type { AdapterProfile } from '@/lib/api/adapter';
import type { Account, Provider } from '@/lib/types';
import type {
  ConnectionUsage,
  ConnectionUsageEntry,
  ConnectionUsageInput,
  ConnectionUsageMap,
  ConnectSourceRef,
} from './types';
import { connectSourceKey } from './types';

interface MutableUsage {
  status: ConnectionUsage['status'];
  agents: ConnectionUsageEntry[];
}

function generatedProviderIds(profiles: readonly Pick<AdapterProfile, 'generatedProviderId'>[]): Set<string> {
  return new Set(
    profiles
      .map((profile) => profile.generatedProviderId)
      .filter((id): id is string => typeof id === 'string' && id.length > 0),
  );
}

function sourceRef(profile: AdapterProfile): ConnectSourceRef {
  return { kind: profile.sourceKind, id: profile.sourceId };
}

function hasSource(
  profile: AdapterProfile,
  accounts: ReadonlyMap<string, Account>,
  providers: ReadonlyMap<string, Provider>,
): boolean {
  if (profile.sourceKind === 'account') return accounts.has(profile.sourceId);
  return providers.has(profile.sourceId);
}

function generatedProvider(
  profile: AdapterProfile,
  providers: ReadonlyMap<string, Provider>,
): Provider | undefined {
  const id = profile.generatedProviderId;
  if (typeof id !== 'string' || id.length === 0) return undefined;
  return providers.get(id);
}

function addAgent(row: MutableUsage, entry: ConnectionUsageEntry): void {
  const existing = row.agents.find((item) => item.agentId === entry.agentId);
  if (!existing) {
    row.agents.push(entry);
    return;
  }
  // 同一 Agent 直接+兼容去重，直接用途优先。
  if (existing.via === 'adapter' && entry.via === 'direct') {
    existing.via = 'direct';
  }
}

export function computeConnectionUsageMap(input: ConnectionUsageInput): ConnectionUsageMap {
  const { accounts, providers, profiles, poolComplete } = input;
  const generatedIds = generatedProviderIds(profiles);
  const accountById = new Map(accounts.map((account) => [account.id, account]));
  const providerById = new Map(providers.map((provider) => [provider.id, provider]));
  const map = new Map<string, MutableUsage>();

  const ensure = (key: string): MutableUsage => {
    const existing = map.get(key);
    if (existing) return existing;
    const created: MutableUsage = { status: 'known', agents: [] };
    map.set(key, created);
    return created;
  };

  for (const account of accounts) {
    const key = connectSourceKey({ kind: 'account', id: account.id });
    const row = ensure(key);
    if (account.isCurrent) {
      addAgent(row, { agentId: account.agentId, via: 'direct' });
    }
  }

  for (const provider of providers) {
    if (generatedIds.has(provider.id)) continue;
    const key = connectSourceKey({ kind: 'provider', id: provider.id });
    const row = ensure(key);
    if (provider.isCurrent) {
      addAgent(row, { agentId: provider.agentId, via: 'direct' });
    }
  }

  for (const profile of profiles) {
    const ref = sourceRef(profile);
    const key = connectSourceKey(ref);
    const sourceIsGenerated = ref.kind === 'provider' && generatedIds.has(ref.id);
    const sourceOk = hasSource(profile, accountById, providerById);
    const generated = generatedProvider(profile, providerById);

    if (!sourceOk || !generated) {
      if (!sourceIsGenerated) {
        ensure(key).status = 'incomplete';
      }
      continue;
    }

    if (sourceIsGenerated) continue;

    // applying / active / needs_attention 均计入；权威是生成 Provider 是否 isCurrent。
    if (generated.isCurrent) {
      addAgent(ensure(key), { agentId: profile.targetAgentId, via: 'adapter' });
    }
  }

  if (!poolComplete) {
    for (const row of map.values()) {
      row.status = 'incomplete';
    }
  }

  return map;
}
