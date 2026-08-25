/**
 * mock adapter helpers. Not a route planner: product decisions come from golden.
 */
import {
  type AdapterAction,
  type AdapterBridgeRuntimeStatus,
  type AdapterEvidence,
  type AdapterPlanChange,
  type AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { Account, Provider } from '@/lib/types';

const verifiedAt = '2026-08-12';

export interface MockAdapterState {
  profiles: AdapterProfile[];
  bridgeStatuses: Map<string, AdapterBridgeRuntimeStatus>;
  generatedProviders: Map<string, Provider>;
  removeGeneratedProvider?: (provider: Provider) => void;
}

export interface MockAdapterSourceResolver {
  getAccountById(id: string): Account | undefined;
  getProviderById(id: string): Provider | undefined;
  upsertGeneratedProvider?(provider: Provider): Provider;
  removeGeneratedProvider?(provider: Provider): void;
}

export type ClassifiableAccount = Account & {
  extra?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
};

export function evidence(label: string, url: string): AdapterEvidence {
  return { label, url, verifiedAt };
}

export function action(
  kind: AdapterAction['kind'],
  target: string,
  description: string,
  value?: string,
): AdapterAction {
  return { kind, target, description, value, secret: false };
}

export function secretAction(target: string, description: string): AdapterAction {
  return { kind: 'reference_connection_secret', target, description, secret: true };
}

export function change(target: string, field: string, value?: string): AdapterPlanChange {
  return { target, field, value, secret: false };
}

export function secretChange(target: string, field: string): AdapterPlanChange {
  return { target, field, secret: true };
}

export const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
export const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
export const OPENAI_API_ENDPOINT_NEEDLE = 'api.openai.com';
export const XAI_API_ENDPOINT_NEEDLE = 'api.x.ai';
export const GLM_CODING_ANTHROPIC_NEEDLE = 'open.bigmodel.cn/api/anthropic';
export const GLM_CODING_CHAT_NEEDLE = 'open.bigmodel.cn/api/coding';
export const GLM_CODING_RESPONSES_NEEDLE = 'open.bigmodel.cn/api/v1';
export const DEEPSEEK_API_ENDPOINT_NEEDLE = 'api.deepseek.com';

export function jsonString(value: unknown, key: string): string | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = (value as Record<string, unknown>)[key];
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed || undefined;
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

export function hasAccountApiKey(account: ClassifiableAccount | undefined): boolean {
  if (!account || account.kind !== 'apikey') return false;
  const credentials = account.credentials;
  return !!credentials
    && jsonString(credentials, 'format')?.toLowerCase() === 'api_key'
    && !!jsonString(credentials, 'api_key');
}
