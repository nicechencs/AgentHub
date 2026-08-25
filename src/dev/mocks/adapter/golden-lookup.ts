/**
 * Slice C: mock analyze/plan only read golden.expect.
 *
 * Source features join a frozen AdapterRouteService::plan() row. Credential
 * availability is an exact match, not a score. Misses are fail-closed
 * unsupported — there is no classifier fallback.
 * Must not enter the production bundle.
 */
import type {
  AdapterGateKind,
  AdapterReusePath,
  AdapterRoute,
  AdapterRouteRequest,
  AdapterSourceKind,
  AdapterSupport,
} from '@/lib/backend/contracts/adapter';
import type { Account, Provider } from '@/lib/types';
import contract from '../fixtures/adapter-capability-contract.json';
import {
  ticketKeyForRequest,
  ticketKeyFromAccount,
  ticketKeyFromProvider,
  type SourceTicketKey,
} from './source-ticket';
import {
  hasAccountApiKey,
  type ClassifiableAccount,
  type MockAdapterSourceResolver,
} from './types';

/** IDs of pnpm dev:mock connect-flow seeds. Duplicated to avoid an import cycle. */
export const DEV_MOCK_KNOWN_SEED_IDS = [
  'kimi-code-membership',
  'anthropic-api',
  'claude-official-login',
] as const;

export type GoldenApplyPath = 'native' | 'local_bridge' | 'config_sync' | 'rejected';

export interface GoldenExpect {
  route: AdapterRoute;
  support: AdapterSupport;
  canApply: boolean;
  applyPath: GoldenApplyPath;
  ruleId: string | null;
  gateKind: AdapterGateKind;
  reason: string;
  reusePath: AdapterReusePath;
}

export interface GoldenLookupHit {
  id: string;
  expect: GoldenExpect;
}

export interface GoldenLookupMissDetail {
  sourceId: string;
  sourceKind: AdapterSourceKind;
  target: string;
  ticketKey: SourceTicketKey;
  knownSeed: boolean;
}

export interface GoldenLookupStats {
  lookups: number;
  hits: number;
  misses: number;
  knownSeedLookups: number;
  knownSeedHits: number;
  knownSeedMisses: number;
  missDetails: GoldenLookupMissDetail[];
}

type ContractCase = (typeof contract.cases)[number];

interface IndexedGoldenCase {
  id: string;
  kind: AdapterSourceKind;
  target: string;
  ticketKey: Exclude<SourceTicketKey, 'missing'>;
  secret: boolean;
  preset?: string;
  extraProvider?: string;
  extraPreset?: string;
  credentialFormat?: string;
  expect: GoldenExpect;
}

const GOLDEN_SOURCE_ID = '__golden-lookup__';

let stats: GoldenLookupStats = emptyStats();

function emptyStats(): GoldenLookupStats {
  return {
    lookups: 0,
    hits: 0,
    misses: 0,
    knownSeedLookups: 0,
    knownSeedHits: 0,
    knownSeedMisses: 0,
    missDetails: [],
  };
}

export function resetGoldenLookupStats(): void {
  stats = emptyStats();
}

export function getGoldenLookupStats(): GoldenLookupStats {
  return {
    ...stats,
    missDetails: stats.missDetails.map((item) => ({ ...item })),
  };
}

function asRoute(value: string): AdapterRoute {
  if (
    value === 'native_endpoint'
    || value === 'local_bridge'
    || value === 'config_sync'
    || value === 'unsupported'
  ) {
    return value;
  }
  return 'unsupported';
}

function asSupport(value: string): AdapterSupport {
  if (value === 'stable' || value === 'experimental' || value === 'unsupported') return value;
  return 'unsupported';
}

function asApplyPath(value: string): GoldenApplyPath {
  if (value === 'native' || value === 'local_bridge' || value === 'config_sync' || value === 'rejected') {
    return value;
  }
  return 'rejected';
}

function asGateKind(value: string): AdapterGateKind {
  if (
    value === 'none'
    || value === 'preview_only'
    || value === 'subscription_candidate'
    || value === 'unsupported'
  ) {
    return value;
  }
  return 'unsupported';
}

function asReusePath(value: string): AdapterReusePath {
  if (
    value === 'api_endpoint'
    || value === 'native_subscription'
    || value === 'local_bridge'
    || value === 'none'
  ) {
    return value;
  }
  return 'none';
}

function expectFromCase(item: ContractCase): GoldenExpect {
  return {
    route: asRoute(item.expect.route),
    support: asSupport(item.expect.support),
    canApply: item.expect.canApply,
    applyPath: asApplyPath(item.expect.applyPath),
    ruleId: item.expect.ruleId ?? null,
    gateKind: asGateKind(item.expect.gateKind),
    reason: item.expect.reason,
    reusePath: asReusePath(item.expect.reusePath),
  };
}

function extraString(extra: unknown, key: string): string | undefined {
  if (!extra || typeof extra !== 'object') return undefined;
  const raw = (extra as Record<string, unknown>)[key];
  return typeof raw === 'string' && raw.trim() ? raw.trim() : undefined;
}

const POSITIVE_AUTH_HEALTH = new Set(['verified', 'renewable', 'configured']);
const NEGATIVE_AUTH_HEALTH = new Set(['needs_login', 'missing']);
const CREDENTIAL_METADATA_KEYS = new Set(['format', 'provider', 'preset']);

function credentialsHaveAccessToken(credentials: unknown): boolean {
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const candidates = [
    record.access_token,
    (record.tokens as Record<string, unknown> | undefined)?.access_token,
    ((record.body as Record<string, unknown> | undefined)?.tokens as Record<string, unknown> | undefined)
      ?.access_token,
  ];
  return candidates.some((token) => typeof token === 'string' && Boolean(token.trim()));
}

function credentialsHaveTokenSlot(credentials: Record<string, unknown>): boolean {
  if (Object.prototype.hasOwnProperty.call(credentials, 'access_token')) return true;
  const tokens = credentials.tokens;
  if (tokens && typeof tokens === 'object' && !Array.isArray(tokens)) {
    const bag = tokens as Record<string, unknown>;
    return Object.prototype.hasOwnProperty.call(bag, 'access_token')
      || Object.prototype.hasOwnProperty.call(bag, 'refresh_token');
  }
  const body = credentials.body;
  if (body && typeof body === 'object' && !Array.isArray(body)) {
    return credentialsHaveTokenSlot(body as Record<string, unknown>);
  }
  return false;
}

function isCredentialMetadataOnly(credentials: Record<string, unknown>): boolean {
  const keys = Object.keys(credentials);
  return keys.length > 0 && keys.every((key) => CREDENTIAL_METADATA_KEYS.has(key));
}

/**
 * Frozen JSON rows have no tokenValid/authHealth. Index secret from credential
 * contents only; empty or missing bags are "no secret".
 */
function frozenAccountHasUsableSecret(account: ClassifiableAccount): boolean {
  if (account.kind === 'apikey') return hasAccountApiKey(account);
  return credentialsHaveAccessToken(account.credentials);
}

/**
 * Live mock accounts may already be redacted. Prefer tokenValid / authHealth;
 * inspect credential contents only for frozen/test rows that still carry them.
 */
function liveAccountHasUsableSecret(account: ClassifiableAccount): boolean {
  const health = account.liveAuthHealth ?? account.authHealth;
  if (account.tokenValid === false || (health != null && NEGATIVE_AUTH_HEALTH.has(health))) {
    return false;
  }

  const credentials = account.credentials;
  if (credentials && typeof credentials === 'object') {
    const bag = credentials as Record<string, unknown>;
    if (Object.keys(bag).length === 0) {
      return false;
    } else if (account.kind === 'apikey') {
      if (hasAccountApiKey(account)) return true;
      if (!isCredentialMetadataOnly(bag)) return false;
    } else if (credentialsHaveAccessToken(bag)) {
      return true;
    } else if (credentialsHaveTokenSlot(bag) || !isCredentialMetadataOnly(bag)) {
      return false;
    }
  }

  return account.tokenValid === true || (health != null && POSITIVE_AUTH_HEALTH.has(health));
}

function goldenAccount(item: ContractCase): ClassifiableAccount {
  const source = item.source as {
    agentId: string;
    accountKind?: Account['kind'];
    credentialFormat?: string;
    extra?: Record<string, unknown>;
    credentials?: Record<string, unknown>;
  };
  return {
    id: GOLDEN_SOURCE_ID,
    agentId: source.agentId,
    kind: source.accountKind ?? 'oauth',
    label: item.id,
    isCurrent: false,
    tokenValid: true,
    credentialFormat: source.credentialFormat,
    credentials: source.credentials,
    extra: source.extra,
  };
}

function goldenProvider(item: ContractCase): Provider {
  const source = item.source as { agentId: string; preset?: string };
  return {
    id: GOLDEN_SOURCE_ID,
    agentId: source.agentId,
    name: item.id,
    preset: source.preset ?? 'default',
    configText: '{}',
    configFormat: 'json',
    isCurrent: false,
  };
}

function goldenTicketKey(item: ContractCase): Exclude<SourceTicketKey, 'missing'> {
  if (item.source.kind === 'provider') return ticketKeyFromProvider(goldenProvider(item));
  return ticketKeyFromAccount(goldenAccount(item));
}

function goldenHasUsableSecret(item: ContractCase): boolean {
  if (item.source.kind === 'provider') return true;
  return frozenAccountHasUsableSecret(goldenAccount(item));
}

function indexGoldenCases(): IndexedGoldenCase[] {
  return contract.cases.map((item) => {
    const source = item.source as {
      kind: AdapterSourceKind;
      preset?: string;
      extra?: Record<string, unknown>;
      credentialFormat?: string;
      credentials?: Record<string, unknown>;
    };
    return {
      id: item.id,
      kind: source.kind,
      target: item.target,
      ticketKey: goldenTicketKey(item),
      secret: goldenHasUsableSecret(item),
      preset: source.preset,
      extraProvider: extraString(source.extra, 'provider'),
      extraPreset: extraString(source.extra, 'preset'),
      credentialFormat:
        source.credentialFormat
        ?? extraString(source.credentials, 'format'),
      expect: expectFromCase(item),
    };
  });
}

const GOLDEN_INDEX: readonly IndexedGoldenCase[] = indexGoldenCases();

function liveHasUsableSecret(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): boolean {
  if (request.sourceKind === 'provider') return true;
  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return false;
  return liveAccountHasUsableSecret(account);
}

function livePreset(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): string | undefined {
  if (request.sourceKind !== 'provider') return undefined;
  return resolver.getProviderById(request.sourceId)?.preset?.trim() || undefined;
}

function liveAccountFeatures(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): { extraProvider?: string; extraPreset?: string; credentialFormat?: string } {
  if (request.sourceKind !== 'account') return {};
  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return {};
  return {
    extraProvider:
      extraString(account.extra, 'provider')
      ?? extraString(account.credentials, 'provider')
      ?? account.provider?.trim(),
    extraPreset: extraString(account.extra, 'preset'),
    credentialFormat:
      extraString(account.credentials, 'format')
      ?? extraString(account.extra, 'format')
      ?? account.credentialFormat?.trim(),
  };
}

function isKnownMockSeed(
  request: AdapterRouteRequest,
  ticketKey: Exclude<SourceTicketKey, 'missing'>,
): boolean {
  if ((DEV_MOCK_KNOWN_SEED_IDS as readonly string[]).includes(request.sourceId)) return true;
  if (request.sourceId.startsWith('contract-')) return true;
  return GOLDEN_INDEX.some(
    (entry) =>
      entry.kind === request.sourceKind
      && entry.ticketKey === ticketKey
      && entry.target === request.targetAgentId,
  );
}

function scoreCandidate(
  entry: IndexedGoldenCase,
  secret: boolean,
  preset: string | undefined,
  account: { extraProvider?: string; extraPreset?: string; credentialFormat?: string },
): number {
  let score = 0;
  if (entry.secret === secret) score += 8;
  if (entry.preset && preset && entry.preset === preset) score += 2;
  if (entry.extraProvider && account.extraProvider && entry.extraProvider === account.extraProvider) {
    score += 2;
  }
  if (entry.extraPreset && account.extraPreset && entry.extraPreset === account.extraPreset) {
    score += 2;
  }
  if (
    entry.credentialFormat
    && account.credentialFormat
    && entry.credentialFormat === account.credentialFormat
  ) {
    score += 2;
  }
  return score;
}

function pickBest(
  candidates: IndexedGoldenCase[],
  secret: boolean,
  preset: string | undefined,
  account: { extraProvider?: string; extraPreset?: string; credentialFormat?: string },
): IndexedGoldenCase {
  return candidates.reduce((best, item) => {
    const bestScore = scoreCandidate(best, secret, preset, account);
    const itemScore = scoreCandidate(item, secret, preset, account);
    if (itemScore > bestScore) return item;
    if (itemScore < bestScore) return best;
    return item.id < best.id ? item : best;
  });
}

function recordLookup(
  request: AdapterRouteRequest,
  ticketKey: Exclude<SourceTicketKey, 'missing'>,
  knownSeed: boolean,
  hit: IndexedGoldenCase | undefined,
): void {
  stats.lookups += 1;
  if (knownSeed) stats.knownSeedLookups += 1;
  if (hit) {
    stats.hits += 1;
    if (knownSeed) stats.knownSeedHits += 1;
    return;
  }
  stats.misses += 1;
  if (knownSeed) stats.knownSeedMisses += 1;
  stats.missDetails.push({
    sourceId: request.sourceId,
    sourceKind: request.sourceKind,
    target: request.targetAgentId,
    ticketKey,
    knownSeed,
  });
}

export function lookupGoldenExpect(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
  options?: { record?: boolean },
): GoldenLookupHit | null {
  const ticketKey = ticketKeyForRequest(resolver, request);
  if (ticketKey === 'missing') return null;
  const secret = liveHasUsableSecret(resolver, request);
  const candidates = GOLDEN_INDEX.filter(
    (entry) =>
      entry.kind === request.sourceKind
      && entry.ticketKey === ticketKey
      && entry.target === request.targetAgentId
      && entry.secret === secret,
  );
  const preset = livePreset(resolver, request);
  const account = liveAccountFeatures(resolver, request);
  const hit = candidates.length === 0
    ? undefined
    : candidates.length === 1
      ? candidates[0]
      : pickBest(candidates, secret, preset, account);
  const knownSeed = isKnownMockSeed(request, ticketKey);
  if (options?.record !== false) {
    recordLookup(request, ticketKey, knownSeed, hit);
  }
  return hit ? { id: hit.id, expect: hit.expect } : null;
}

export function goldenTargetsForTicket(
  kind: AdapterSourceKind,
  ticketKey: Exclude<SourceTicketKey, 'missing'>,
): string[] {
  const targets = new Set<string>();
  for (const entry of GOLDEN_INDEX) {
    if (entry.kind === kind && entry.ticketKey === ticketKey) targets.add(entry.target);
  }
  return [...targets].sort();
}

export { ticketKeyForRequest };
