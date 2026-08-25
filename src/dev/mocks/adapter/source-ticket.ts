/**
 * Join key from live/golden source features to a frozen contract row.
 * Does not choose route, reason, ruleId, or canApply.
 */
import type { AdapterRouteRequest } from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import {
  DEEPSEEK_API_ENDPOINT_NEEDLE,
  GLM_CODING_ANTHROPIC_NEEDLE,
  GLM_CODING_CHAT_NEEDLE,
  GLM_CODING_RESPONSES_NEEDLE,
  OPENAI_API_ENDPOINT_NEEDLE,
  XAI_API_ENDPOINT_NEEDLE,
  isCodexAuthJson,
  isKimiMembershipAccount,
  isKimiMembershipProvider,
  jsonString,
  type ClassifiableAccount,
  type MockAdapterSourceResolver,
} from './types';

export type SourceTicketKey =
  | 'kimi-code-membership'
  | 'kimi-non-membership'
  | 'anthropic'
  | 'openai'
  | 'xai'
  | 'glm-coding-plan'
  | 'deepseek-api'
  | 'claude-oauth'
  | 'grok-oauth'
  | 'codex-auth-json'
  | 'codex-oauth'
  | 'other'
  | 'missing';

export function sourceExists(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): boolean {
  if (request.sourceKind === 'provider') return !!resolver.getProviderById(request.sourceId);
  return !!resolver.getAccountById(request.sourceId);
}

export function ticketKeyFromProvider(provider: Provider): Exclude<SourceTicketKey, 'missing'> {
  if (isKimiMembershipProvider(provider)) return 'kimi-code-membership';
  if (
    provider.agentId === 'claude'
    && (provider.preset === 'anthropic'
      || (typeof provider.configText === 'string'
        && provider.configText.toLowerCase().includes('api.anthropic.com')))
  ) {
    return 'anthropic';
  }
  const tag = provider.preset
    ?? jsonString((provider as { meta?: Record<string, unknown> }).meta, 'provider');
  const config = provider.configText ?? '';
  if (
    tag?.toLowerCase() === 'openai'
    || tag?.toLowerCase() === 'openai-api'
    || tag?.toLowerCase() === 'openai-compat'
    || tag?.toLowerCase() === 'openrouter'
    || config.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE)
    || config.toLowerCase().includes('openrouter.ai')
  ) {
    return 'openai';
  }
  if (
    tag?.toLowerCase() === 'xai'
    || tag?.toLowerCase() === 'xai-api'
    || config.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
  ) {
    return 'xai';
  }
  if (
    tag?.toLowerCase() === 'glm-coding-plan'
    || config.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
    || config.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
    || config.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE)
  ) {
    return 'glm-coding-plan';
  }
  if (
    tag?.toLowerCase() === 'deepseek-api'
    || tag?.toLowerCase() === 'deepseek'
    || config.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
  ) {
    return 'deepseek-api';
  }
  if (provider.agentId === 'kimi') return 'kimi-non-membership';
  return 'other';
}

export function ticketKeyFromAccount(account: ClassifiableAccount): Exclude<SourceTicketKey, 'missing'> {
  const explicitProvider =
    jsonString(account.extra, 'provider')
    ?? jsonString(account.credentials, 'provider')
    ?? account.provider?.trim();
  const credentialFormat =
    jsonString(account.credentials, 'format')
    ?? jsonString(account.extra, 'format')
    ?? account.credentialFormat?.trim();

  if (isKimiMembershipAccount(account)) return 'kimi-code-membership';
  if (account.kind === 'apikey' && account.agentId === 'kimi') return 'kimi-non-membership';
  if (account.agentId === 'claude' && account.kind === 'oauth') return 'claude-oauth';
  if (account.agentId === 'grok' && account.kind === 'oauth') return 'grok-oauth';
  if (account.kind === 'apikey' && explicitProvider?.toLowerCase() === 'anthropic') return 'anthropic';
  const credentialsText = JSON.stringify(account.credentials ?? {});
  const extraText = JSON.stringify(account.extra ?? {});
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'openai'
      || explicitProvider?.toLowerCase() === 'openai-api'
      || explicitProvider?.toLowerCase() === 'openai-compat'
      || explicitProvider?.toLowerCase() === 'openrouter'
      || credentialsText.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE)
      || credentialsText.toLowerCase().includes('openrouter.ai')
      || extraText.toLowerCase().includes(OPENAI_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes('openrouter.ai'))
  ) {
    return 'openai';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'xai'
      || explicitProvider?.toLowerCase() === 'xai-api'
      || credentialsText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'xai';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'glm-coding-plan'
      || credentialsText.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || credentialsText.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || credentialsText.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || extraText.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE))
  ) {
    return 'glm-coding-plan';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'deepseek-api'
      || explicitProvider?.toLowerCase() === 'deepseek'
      || credentialsText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE))
  ) {
    return 'deepseek-api';
  }
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return isCodexAuthJson(credentialFormat, account.credentials ?? {})
      ? 'codex-auth-json'
      : 'codex-oauth';
  }
  return 'other';
}

export function ticketKeyForRequest(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): SourceTicketKey {
  if (request.sourceKind === 'provider') {
    const provider = resolver.getProviderById(request.sourceId);
    if (!provider) return 'missing';
    return ticketKeyFromProvider(provider);
  }
  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return 'missing';
  return ticketKeyFromAccount(account);
}
