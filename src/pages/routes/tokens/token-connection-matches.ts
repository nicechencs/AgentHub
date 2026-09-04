/**
 * Connections logins that reused a tokens-page entry key (imported API Key).
 * Match is by secret hash only; leftover / pool-owned rows stay out.
 */
import { isLeftoverLocalRouteProvider } from '@/lib/leftover-local-route';
import type { Account, AgentKey, Provider } from '@/lib/types';

export type ConnectionEntryKeyMatch = {
  sourceKind: 'provider' | 'account';
  sourceId: string;
  agentId: AgentKey;
  label: string;
};

export async function hashLocalToken(
  token: string,
  digest: (data: BufferSource) => Promise<ArrayBuffer> = webSha256,
): Promise<string> {
  const trimmed = token.trim();
  if (!trimmed) return '';
  const bytes = new TextEncoder().encode(trimmed);
  const buf = await digest(bytes);
  return [...new Uint8Array(buf)].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

async function webSha256(data: BufferSource): Promise<ArrayBuffer> {
  return crypto.subtle.digest('SHA-256', data);
}

function sameHash(value: string | null | undefined, tokenHash: string): boolean {
  const hash = value?.trim().toLowerCase() ?? '';
  return Boolean(hash) && hash === tokenHash;
}

export function matchesConnectionEntryKeys(input: {
  tokenHash: string;
  providers?: readonly Pick<
    Provider,
    'id' | 'agentId' | 'name' | 'secretHash' | 'home' | 'preset' | 'configText' | 'configFormat'
  >[];
  accounts?: readonly Pick<Account, 'id' | 'agentId' | 'label' | 'kind' | 'secretHash' | 'home'>[];
}): ConnectionEntryKeyMatch[] {
  const tokenHash = input.tokenHash.trim().toLowerCase();
  if (!tokenHash) return [];
  const out: ConnectionEntryKeyMatch[] = [];
  const seen = new Set<string>();

  const push = (match: ConnectionEntryKeyMatch) => {
    const key = `${match.sourceKind}:${match.sourceId}`;
    if (seen.has(key)) return;
    seen.add(key);
    out.push(match);
  };

  for (const provider of input.providers ?? []) {
    if (provider.home === 'route_pool') continue;
    if (isLeftoverLocalRouteProvider(provider)) continue;
    if (!sameHash(provider.secretHash, tokenHash)) continue;
    push({
      sourceKind: 'provider',
      sourceId: provider.id,
      agentId: provider.agentId,
      label: provider.name.trim() || provider.id,
    });
  }

  for (const account of input.accounts ?? []) {
    if (account.home === 'route_pool') continue;
    if (account.kind !== 'apikey') continue;
    if (!sameHash(account.secretHash, tokenHash)) continue;
    push({
      sourceKind: 'account',
      sourceId: account.id,
      agentId: account.agentId,
      label: account.label.trim() || account.id,
    });
  }

  return out;
}

export function connectionMatchAgentNames(
  matches: readonly ConnectionEntryKeyMatch[],
  nameOf: (agentId: AgentKey) => string = (id) => id,
): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const match of matches) {
    const name = nameOf(match.agentId).trim() || match.label || match.agentId;
    if (seen.has(name)) continue;
    seen.add(name);
    names.push(name);
  }
  return names;
}
