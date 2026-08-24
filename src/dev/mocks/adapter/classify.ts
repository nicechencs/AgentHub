import type { AdapterRouteRequest } from '@/lib/backend/contracts/adapter';
import {
  type ClassifiableAccount,
  type MockAdapterSourceResolver,
  type RouteSourceLabel,
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
} from './types';

export function classify(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): RouteSourceLabel {
  if (request.sourceKind === 'provider') {
    const provider = resolver.getProviderById(request.sourceId);
    if (!provider) return 'not_found';
    if (isKimiMembershipProvider(provider)) {
      return 'kimi_membership';
    }
    if (
      provider.agentId === 'claude'
      && (provider.preset === 'anthropic'
        || (typeof provider.configText === 'string'
          && provider.configText.toLowerCase().includes('api.anthropic.com')))
    ) {
      return 'anthropic_api_key';
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
      return 'openai_api_key';
    }
    if (
      tag?.toLowerCase() === 'xai'
      || tag?.toLowerCase() === 'xai-api'
      || config.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
    ) {
      return 'xai_api_key';
    }
    if (
      tag?.toLowerCase() === 'glm-coding-plan'
      || config.toLowerCase().includes(GLM_CODING_ANTHROPIC_NEEDLE)
      || config.toLowerCase().includes(GLM_CODING_CHAT_NEEDLE)
      || config.toLowerCase().includes(GLM_CODING_RESPONSES_NEEDLE)
    ) {
      return 'glm_coding_plan';
    }
    if (
      tag?.toLowerCase() === 'deepseek-api'
      || tag?.toLowerCase() === 'deepseek'
      || config.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
    ) {
      return 'deepseek_api';
    }
    if (provider.agentId === 'kimi') return 'kimi_non_membership';
    return 'other';
  }

  const account = resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined;
  if (!account) return 'not_found';
  const explicitProvider =
    jsonString(account.extra, 'provider')
    ?? jsonString(account.credentials, 'provider')
    ?? account.provider?.trim();
  const credentialFormat =
    jsonString(account.credentials, 'format')
    ?? jsonString(account.extra, 'format')
    ?? account.credentialFormat?.trim();

  if (isKimiMembershipAccount(account)) {
    return 'kimi_membership';
  }
  if (account.kind === 'apikey' && account.agentId === 'kimi') {
    return 'kimi_non_membership';
  }
  if (account.agentId === 'claude' && account.kind === 'oauth') {
    return 'claude_subscription';
  }
  if (account.agentId === 'grok' && account.kind === 'oauth') {
    return 'grok_xai_subscription';
  }
  if (account.kind === 'apikey' && explicitProvider?.toLowerCase() === 'anthropic') {
    return 'anthropic_api_key';
  }
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
    return 'openai_api_key';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'xai'
      || explicitProvider?.toLowerCase() === 'xai-api'
      || credentialsText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(XAI_API_ENDPOINT_NEEDLE))
  ) {
    return 'xai_api_key';
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
    return 'glm_coding_plan';
  }
  if (
    account.kind === 'apikey'
    && (explicitProvider?.toLowerCase() === 'deepseek-api'
      || explicitProvider?.toLowerCase() === 'deepseek'
      || credentialsText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE)
      || extraText.toLowerCase().includes(DEEPSEEK_API_ENDPOINT_NEEDLE))
  ) {
    return 'deepseek_api';
  }
  if (account.agentId === 'codex' && account.kind === 'oauth') {
    return isCodexAuthJson(credentialFormat, account.credentials ?? {})
      ? 'codex_subscription'
      : 'codex_subscription_oauth_other';
  }
  if (account.agentId === 'claude' && account.kind === 'oauth') {
    return 'claude_subscription';
  }
  if (account.agentId === 'grok' && account.kind === 'oauth') {
    return 'grok_xai_subscription';
  }
  return 'other';
}
