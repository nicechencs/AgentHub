import type { ConnectionTrashItem } from '@/lib/backend/contracts';
import { secretTailFromMaskedPreview } from '@/lib/backend/contracts/account-map';
import { extractProviderEndpoint } from '@/lib/backend/contracts/agent-connection';
import type { TranslateFn } from '@/lib/i18n';
import type { Provider } from '@/lib/types';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const GROK_LIVE_RE = /grok-live-/i;
const GENERATED_ID_RE = /grok-live-|-adapter-|-bridge-/i;
const BRIDGE_NAME_RE = /bridge/i;
const NAMED_BRIDGE_RE = /Subscription Bridge|Code Bridge|Anthropic Bridge/i;
const LOOPBACK_RE = /127\.0\.0\.1|\blocalhost\b|::1/i;
const HOST_PORT_RE = /:\d{2,5}\b/;
const LIVE_ID_IN_TEXT_RE = /grok-live-[a-z0-9]+(?:-[a-z0-9]+)*/i;
const TRAILING_HASH_RE = /-[0-9a-f]{16}$/i;

function looksLikeUuid(value: string | undefined | null): boolean {
  return !!value && UUID_RE.test(value.trim());
}

function looksLikeEmail(value: string | undefined | null): boolean {
  if (!value) return false;
  const trimmed = value.trim();
  return trimmed.includes('@') && !trimmed.includes(' ') && /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed);
}

function isLoopbackText(value: string | undefined | null): boolean {
  return !!value && LOOPBACK_RE.test(value);
}

function isInternalDisplayToken(value: string | undefined | null): boolean {
  if (!value) return false;
  const text = value.trim();
  if (!text) return false;
  if (BRIDGE_NAME_RE.test(text) || NAMED_BRIDGE_RE.test(text)) return true;
  if (GENERATED_ID_RE.test(text) || looksLikeUuid(text)) return true;
  if (isLoopbackText(text) || (HOST_PORT_RE.test(text) && isLoopbackText(text))) return true;
  return false;
}

function isGeneratedId(value: string | undefined | null): boolean {
  if (!value) return false;
  const text = value.trim();
  if (/-adapter-|-bridge-/i.test(text)) return true;
  return false;
}

function providerLooksLoopback(provider: Provider | undefined): boolean {
  if (!provider) return false;
  const endpoint = extractProviderEndpoint(provider.configText, provider.configFormat);
  return isLoopbackText(endpoint) || isLoopbackText(provider.configText);
}

function isGeneratedTrashItem(item: ConnectionTrashItem): boolean {
  if (isInternalDisplayToken(item.label) || isInternalDisplayToken(item.provider?.name)) return true;
  if (isGeneratedId(item.sourceId) || isGeneratedId(item.provider?.id) || isGeneratedId(item.account?.id)) {
    return true;
  }
  if (looksLikeUuid(item.label)) return true;
  if (looksLikeUuid(item.sourceId) && (!item.label.trim() || item.label.trim() === item.sourceId || isInternalDisplayToken(item.label))) {
    return true;
  }
  if (providerLooksLoopback(item.provider)) return true;
  return false;
}

function sourceIdentity(item: ConnectionTrashItem): string | undefined {
  const emailCandidates = [
    item.account?.email,
    item.account?.identityLabel,
    item.account?.label,
    item.label,
  ];
  for (const candidate of emailCandidates) {
    const value = candidate?.trim();
    if (value && looksLikeEmail(value) && !isInternalDisplayToken(value)) return value;
  }

  for (const candidate of [item.account?.identityLabel, item.account?.label]) {
    const accountLabel = candidate?.trim();
    if (
      accountLabel
      && !isInternalDisplayToken(accountLabel)
      && !looksLikeUuid(accountLabel)
      && !accountLabel.includes('***')
      && !isMaskOnlyLabel(accountLabel)
      && !last4FromMaskLabel(accountLabel)
      && !/token|secret|configText|credentialSummary/i.test(accountLabel)
    ) {
      return accountLabel;
    }
  }
  return undefined;
}

function isMaskOnlyLabel(value: string): boolean {
  const text = value.trim();
  if (!text) return true;
  if (/^[•*….\s]+(?:（API Key）|\(API Key\))?$/i.test(text)) return true;
  if (/^(API Key)$/i.test(text)) return true;
  return false;
}

function localRouteTitle(t?: TranslateFn): string {
  return t ? t('kind.route.localRoute') : '本机路由';
}

function trashHost(item: ConnectionTrashItem): string | undefined {
  const endpoint = trashItemEndpoint(item);
  if (!endpoint) return undefined;
  try {
    return new URL(endpoint).host;
  } catch {
    return endpoint;
  }
}

function last4FromMaskLabel(value: string | undefined | null): string | undefined {
  const tail = secretTailFromMaskedPreview(value);
  return tail ? tail.replace(/^\*+/, '').slice(-4) : undefined;
}

function last4Label(last4: string, t?: TranslateFn): string {
  return t ? t('connections.trash.last4', { last4 }) : `末尾 ${last4}`;
}

function maskOnlyIdentity(item: ConnectionTrashItem, t?: TranslateFn): string | undefined {
  const last4 = trashItemSecretTail(item);
  const host = trashHost(item);
  if (last4 && host) return `${last4Label(last4, t)} · ${host}`;
  if (last4) return last4Label(last4, t);
  if (host) return host;
  return undefined;
}

/** Recycle-bin title: generated bridges become 本机路由 · identity; mask-only API keys show last4/host. */
export function humanizeTrashLabel(item: ConnectionTrashItem, t?: TranslateFn): string {
  const identity = sourceIdentity(item);
  const maskIdentity = maskOnlyIdentity(item, t);
  if (isMaskOnlyLabel(item.label) && !isGeneratedTrashItem(item)) {
    return identity || maskIdentity || 'API Key';
  }
  if (!isGeneratedTrashItem(item) && !isMaskOnlyLabel(item.label)) return item.label;
  const title = localRouteTitle(t);
  if (identity) return `${title} · ${identity}`;
  if (maskIdentity) return `${title} · ${maskIdentity}`;
  const last4 = trashItemSecretTail(item);
  if (last4) return `${title} · ${last4Label(last4, t)}`;
  const host = trashHost(item);
  if (host) return `${title} · ${host}`;
  return title;
}

export function trashItemSecretTail(item: ConnectionTrashItem): string | undefined {
  const tail = item.account?.secretTail?.trim() || item.provider?.secretTail?.trim();
  if (tail) {
    const last4 = tail.replace(/^\*+/, '').slice(-4);
    if (last4) return last4;
  }
  return (
    last4FromMaskLabel(item.account?.identityLabel) ||
    last4FromMaskLabel(item.account?.label) ||
    last4FromMaskLabel(item.provider?.name) ||
    last4FromMaskLabel(item.label)
  );
}

export function trashItemEndpoint(item: ConnectionTrashItem): string | undefined {
  if (item.provider) {
    const endpoint = extractProviderEndpoint(item.provider.configText, item.provider.configFormat);
    if (endpoint && !isLoopbackText(endpoint)) return endpoint;
  }
  const fromAccount = item.account?.endpoint?.trim();
  if (fromAccount && /^https?:\/\//i.test(fromAccount) && !isLoopbackText(fromAccount)) {
    return fromAccount;
  }
  return undefined;
}

function parseDeletedAt(value: string): number {
  const isoLike = value.replace(' ', 'T');
  const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(isoLike) ? isoLike : `${isoLike}Z`;
  const ms = Date.parse(normalized);
  return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms;
}

function collectRawIds(item: ConnectionTrashItem): string[] {
  return [item.sourceId, item.provider?.id, item.account?.id].filter((id): id is string => !!id);
}

function liveAccountKeys(item: ConnectionTrashItem): string[] {
  const keys = new Set<string>();
  for (const id of collectRawIds(item)) {
    const lower = id.toLowerCase();
    if (!GROK_LIVE_RE.test(lower)) continue;
    const fromLive = lower.slice(lower.indexOf('grok-live-'));
    keys.add(fromLive);
    keys.add(fromLive.replace(TRAILING_HASH_RE, ''));
    const match = fromLive.match(LIVE_ID_IN_TEXT_RE);
    if (match) keys.add(match[0]);
  }
  return [...keys];
}

function normalizeTrashHost(value: string | undefined): string | undefined {
  if (!value) return undefined;
  return value.trim().replace(/\/+$/, '').replace(/\/v1$/i, '').toLowerCase() || undefined;
}

function groupingKeys(item: ConnectionTrashItem): string[] {
  const keys: string[] = [];
  if (item.sourceId) keys.push(`source:${item.sourceId}`);
  keys.push(`triple:${item.agentId}:${item.kind}:${item.sourceId}`);
  const last4 = trashItemSecretTail(item);
  const host = normalizeTrashHost(trashItemEndpoint(item));
  if (last4 && host) {
    keys.push(`ident:${item.agentId}:${item.kind}:${last4}:${host}`);
  }
  // Same grok-live-* id may collapse a leftover API Key with a generated
  // bridge across agents. Do not also union by bare email.
  for (const live of liveAccountKeys(item)) keys.push(`live:${live}`);
  return keys;
}

/** Collapse duplicate recycle-bin rows; keep the newest deletedAt (and its id). */
export function dedupTrashItems(items: ConnectionTrashItem[]): ConnectionTrashItem[] {
  if (items.length <= 1) return items.slice();

  const parent = items.map((_, index) => index);
  const find = (index: number): number => {
    let current = index;
    while (parent[current] !== current) {
      parent[current] = parent[parent[current]];
      current = parent[current];
    }
    return current;
  };
  const union = (a: number, b: number): void => {
    const rootA = find(a);
    const rootB = find(b);
    if (rootA !== rootB) parent[rootB] = rootA;
  };

  const keyToIndex = new Map<string, number>();
  for (let i = 0; i < items.length; i += 1) {
    for (const key of groupingKeys(items[i])) {
      const prev = keyToIndex.get(key);
      if (prev === undefined) keyToIndex.set(key, i);
      else union(prev, i);
    }
  }

  const best = new Map<number, number>();
  for (let i = 0; i < items.length; i += 1) {
    const root = find(i);
    const current = best.get(root);
    if (current === undefined) {
      best.set(root, i);
      continue;
    }
    const newer = parseDeletedAt(items[i].deletedAt) > parseDeletedAt(items[current].deletedAt);
    if (newer) best.set(root, i);
  }

  return [...best.values()]
    .sort((a, b) => {
      const byTime = parseDeletedAt(items[b].deletedAt) - parseDeletedAt(items[a].deletedAt);
      return byTime !== 0 ? byTime : a - b;
    })
    .map((index) => items[index]);
}
