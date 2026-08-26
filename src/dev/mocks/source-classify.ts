/**
 * Shared mock source feature detectors.
 * Ticket mock maps MockSourceId → TicketSurface; adapter mock maps → SourceTicketKey.
 * Unique branches (kimi-non-membership, kimi account membership) stay in each consumer.
 */
import type { Account, Provider } from '@/lib/types';

export type ClassifiableAccount = Account & {
  extra?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
};

export type ClassifiableProvider = Provider & {
  meta?: Record<string, unknown>;
};

/** Canonical mock source id. Consumers map this onto their own enums. */
export type MockSourceId =
  | 'kimi-code-membership'
  | 'anthropic'
  | 'openai'
  | 'xai'
  | 'glm-coding-plan'
  | 'deepseek-api'
  | 'claude-oauth'
  | 'grok-oauth'
  | 'codex-auth-json'
  | 'codex-oauth';

export const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
export const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
export const ANTHROPIC_API_ENDPOINT_NEEDLE = 'api.anthropic.com';
export const OPENAI_API_ENDPOINT_NEEDLE = 'api.openai.com';
export const OPENROUTER_ENDPOINT_NEEDLE = 'openrouter.ai';
export const XAI_API_ENDPOINT_NEEDLE = 'api.x.ai';
export const GLM_CODING_ANTHROPIC_NEEDLE = 'open.bigmodel.cn/api/anthropic';
export const GLM_CODING_CHAT_NEEDLE = 'open.bigmodel.cn/api/coding';
export const GLM_CODING_RESPONSES_NEEDLE = 'open.bigmodel.cn/api/v1';
export const DEEPSEEK_API_ENDPOINT_NEEDLE = 'api.deepseek.com';

const OPENAI_TAGS = ['openai', 'openai-api', 'openai-compat', 'openrouter'] as const;
const XAI_TAGS = ['xai', 'xai-api'] as const;
const GLM_TAGS = ['glm-coding-plan'] as const;
const DEEPSEEK_TAGS = ['deepseek-api', 'deepseek'] as const;

export function jsonString(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = (value as Record<string, unknown>)[key];
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed || undefined;
}

export function blobContains(value: unknown, needle: string): boolean {
  if (typeof value === 'string') return value.toLowerCase().includes(needle);
  if (!value || typeof value !== 'object') return false;
  return JSON.stringify(value).toLowerCase().includes(needle);
}

export function explicitTagMatches(tag: string | undefined, accepted: readonly string[]): boolean {
  return !!tag && accepted.some((item) => item.toLowerCase() === tag.toLowerCase());
}

export function isCodexAuthJson(format: string | undefined, credentials: unknown): boolean {
  if (format?.toLowerCase() === 'auth_json') return true;
  const tokens = credentials && typeof credentials === 'object'
    ? (credentials as Record<string, unknown>).tokens
    : undefined;
  if (!tokens || typeof tokens !== 'object' || Array.isArray(tokens)) return false;
  return Object.prototype.hasOwnProperty.call(tokens, 'access_token')
    || Object.prototype.hasOwnProperty.call(tokens, 'refresh_token');
}

export function textLooksLikeKimiCoding(text: string | undefined): boolean {
  return typeof text === 'string' && text.toLowerCase().includes(KIMI_CODING_ENDPOINT_NEEDLE);
}

export function isKimiMembershipProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  return provider.agentId === 'kimi'
    && (provider.preset === KIMI_MEMBERSHIP_PRESET
      || textLooksLikeKimiCoding(provider.configText));
}

export function isKimiMembershipAccount(account: ClassifiableAccount | undefined): boolean {
  if (!account) return false;
  if (account.agentId !== 'kimi' || account.kind !== 'apikey') return false;
  const extra = account.extra ?? {};
  const credentials = account.credentials ?? {};
  const tags = [
    jsonString(extra, 'provider'),
    jsonString(extra, 'preset'),
    jsonString(credentials, 'provider'),
  ];
  return tags.some((tag) => tag?.toLowerCase() === KIMI_MEMBERSHIP_PRESET)
    || textLooksLikeKimiCoding(JSON.stringify(extra))
    || textLooksLikeKimiCoding(JSON.stringify(credentials));
}

export function isAnthropicApiProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  return provider.agentId === 'claude'
    && (provider.preset === 'anthropic'
      || (typeof provider.configText === 'string'
        && provider.configText.toLowerCase().includes(ANTHROPIC_API_ENDPOINT_NEEDLE)));
}

export function looksLikeAnthropicTag(tag: string | undefined): boolean {
  return tag?.toLowerCase() === 'anthropic';
}

export function looksLikeOpenai(tag: string | undefined, blobs: readonly unknown[]): boolean {
  return explicitTagMatches(tag, OPENAI_TAGS)
    || blobs.some((blob) =>
      blobContains(blob, OPENAI_API_ENDPOINT_NEEDLE)
      || blobContains(blob, OPENROUTER_ENDPOINT_NEEDLE));
}

export function looksLikeXai(tag: string | undefined, blobs: readonly unknown[]): boolean {
  return explicitTagMatches(tag, XAI_TAGS)
    || blobs.some((blob) => blobContains(blob, XAI_API_ENDPOINT_NEEDLE));
}

export function looksLikeGlmCoding(
  tag: string | undefined,
  blobs: readonly unknown[],
  includeResponses = false,
): boolean {
  if (explicitTagMatches(tag, GLM_TAGS)) return true;
  const needles = includeResponses
    ? [GLM_CODING_ANTHROPIC_NEEDLE, GLM_CODING_CHAT_NEEDLE, GLM_CODING_RESPONSES_NEEDLE]
    : [GLM_CODING_ANTHROPIC_NEEDLE, GLM_CODING_CHAT_NEEDLE];
  return blobs.some((blob) => needles.some((needle) => blobContains(blob, needle)));
}

export function looksLikeDeepseek(tag: string | undefined, blobs: readonly unknown[]): boolean {
  return explicitTagMatches(tag, DEEPSEEK_TAGS)
    || blobs.some((blob) => blobContains(blob, DEEPSEEK_API_ENDPOINT_NEEDLE));
}

export function providerSourceTag(provider: Provider): string | undefined {
  return provider.preset
    ?? jsonString((provider as ClassifiableProvider).meta, 'provider');
}

export function accountExplicitProvider(account: ClassifiableAccount): string | undefined {
  return jsonString(account.extra, 'provider')
    ?? jsonString(account.credentials, 'provider')
    ?? account.provider?.trim();
}

export function accountCredentialFormat(account: ClassifiableAccount): string | undefined {
  return jsonString(account.credentials, 'format')
    ?? jsonString(account.extra, 'format')
    ?? account.credentialFormat?.trim();
}

export type ClassifyProviderSourceOptions = {
  /** Adapter mock also treats the GLM Responses URL as glm-coding-plan. Ticket mock does not. */
  includeGlmResponses?: boolean;
};

export function classifyProviderSource(
  provider: Provider,
  options: ClassifyProviderSourceOptions = {},
): MockSourceId | null {
  if (isKimiMembershipProvider(provider)) return 'kimi-code-membership';
  if (isAnthropicApiProvider(provider)) return 'anthropic';
  const tag = providerSourceTag(provider);
  const blobs = [provider.configText];
  if (looksLikeOpenai(tag, blobs)) return 'openai';
  if (looksLikeXai(tag, blobs)) return 'xai';
  if (looksLikeGlmCoding(tag, blobs, options.includeGlmResponses ?? false)) return 'glm-coding-plan';
  if (looksLikeDeepseek(tag, blobs)) return 'deepseek-api';
  return null;
}

export type ClassifyAccountSourceOptions = {
  /** Adapter mock also treats the GLM Responses URL as glm-coding-plan. Ticket mock does not. */
  includeGlmResponses?: boolean;
  /** Ticket mock also classifies apikey accounts whose credentials/extra mention the Anthropic host. */
  includeAnthropicEndpoint?: boolean;
};

export function classifyAccountSource(
  account: ClassifiableAccount,
  options: ClassifyAccountSourceOptions = {},
): MockSourceId | null {
  const explicitProvider = accountExplicitProvider(account);
  const credentialFormat = accountCredentialFormat(account);

  if (account.agentId === 'claude' && account.kind === 'oauth') return 'claude-oauth';
  if (account.agentId === 'grok' && account.kind === 'oauth') return 'grok-oauth';

  if (account.kind === 'apikey') {
    if (
      looksLikeAnthropicTag(explicitProvider)
      || (options.includeAnthropicEndpoint && accountObjectsContainNeedle(account, ANTHROPIC_API_ENDPOINT_NEEDLE))
    ) {
      return 'anthropic';
    }
    const blobs = [account.credentials, account.extra];
    if (looksLikeOpenai(explicitProvider, blobs)) return 'openai';
    if (looksLikeXai(explicitProvider, blobs)) return 'xai';
    if (looksLikeGlmCoding(explicitProvider, blobs, options.includeGlmResponses ?? false)) {
      return 'glm-coding-plan';
    }
    if (looksLikeDeepseek(explicitProvider, blobs)) return 'deepseek-api';
  }

  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return isCodexAuthJson(credentialFormat, account.credentials ?? {})
      ? 'codex-auth-json'
      : 'codex-oauth';
  }
  return null;
}

/** Ticket mock: `typeof === 'object'` then JSON.stringify, including null. */
function accountObjectsContainNeedle(account: ClassifiableAccount, needle: string): boolean {
  const credentials = account.credentials;
  const extra = account.extra;
  return (typeof credentials === 'object'
      && JSON.stringify(credentials).toLowerCase().includes(needle))
    || (typeof extra === 'object'
      && JSON.stringify(extra).toLowerCase().includes(needle));
}
