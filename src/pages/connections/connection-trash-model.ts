import type { ConnectionTrashItem } from '@/lib/backend/contracts';
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
  return !!value && GENERATED_ID_RE.test(value);
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
  const emailCandidates = [item.account?.email, item.account?.label, item.label];
  for (const candidate of emailCandidates) {
    const value = candidate?.trim();
    if (value && looksLikeEmail(value) && !isInternalDisplayToken(value)) return value;
  }

  const accountLabel = item.account?.label?.trim();
  if (
    accountLabel
    && !isInternalDisplayToken(accountLabel)
    && !looksLikeUuid(accountLabel)
    && !accountLabel.includes('***')
    && !/token|secret|configText|credentialSummary/i.test(accountLabel)
  ) {
    return accountLabel;
  }
  return undefined;
}

function localRouteTitle(t?: TranslateFn): string {
  return t ? t('kind.route.localRoute') : '本机路由';
}

/** Recycle-bin title: generated/bridge rows become 本机路由 · email, never raw ids. */
export function humanizeTrashLabel(item: ConnectionTrashItem, t?: TranslateFn): string {
  if (!isGeneratedTrashItem(item)) return item.label;
  const title = localRouteTitle(t);
  const identity = sourceIdentity(item);
  if (!identity) return title;
  return `${title} · ${identity}`;
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

function accountEmails(item: ConnectionTrashItem): string[] {
  const emails = new Set<string>();
  for (const value of [item.account?.email, item.account?.label]) {
    const trimmed = value?.trim();
    if (trimmed && looksLikeEmail(trimmed)) emails.add(trimmed.toLowerCase());
  }
  return [...emails];
}

function groupingKeys(item: ConnectionTrashItem): string[] {
  const keys: string[] = [];
  if (item.sourceId) keys.push(`source:${item.sourceId}`);
  keys.push(`triple:${item.agentId}:${item.kind}:${item.sourceId}`);
  if (!isGeneratedTrashItem(item)) return keys;
  for (const live of liveAccountKeys(item)) keys.push(`live:${live}`);
  for (const email of accountEmails(item)) keys.push(`email:${email}`);
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
