import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import {
  CODEX_CLAUDE_RULE_ID,
  DEEPSEEK_CLAUDE_BASE_URL,
  DEEPSEEK_CLAUDE_RULE_ID,
  DEEPSEEK_CODEX_BASE_URL,
  DEEPSEEK_CODEX_PROVIDER_SLUG,
  DEEPSEEK_PI_BASE_URL,
  GLM_CLAUDE_BASE_URL,
  GLM_CLAUDE_RULE_ID,
  GLM_CODEX_BASE_URL,
  GLM_CODEX_PROVIDER_SLUG,
  GLM_CODEX_RULE_ID,
  GLM_PI_BASE_URL,
  GROK_CLAUDE_RULE_ID,
  KIMI_CLAUDE_BASE_URL,
  KIMI_GROK_BASE_URL,
  KIMI_GROK_RULE_ID,
  NATIVE_SUBSCRIPTION_PI_RULE_IDS,
  OPENAI_GROK_BASE_URL,
} from './types';

/** Browser-only mirror of the core's explicit routing rules. */
const CONNECTION_SECRET_MARKER = '$AGENTHUB_CONNECTION_SECRET$';

function safeSourceId(sourceId: string): string {
  return sourceId.replace(/[^a-zA-Z0-9_-]/g, '-').slice(0, 40) || 'source';
}

function piSlotFromPlan(plan: AdapterApplyPlan, fallback: string): string {
  return plan.analysis.actions.find(
    (item) => item.kind === 'set_config' && item.target === 'Pi',
  )?.value ?? fallback;
}

export function materializeApply(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
): { profile: AdapterProfile; provider: Provider } {
  const safeId = safeSourceId(request.sourceId);
  if (plan.analysis.route === 'native_endpoint' && request.targetAgentId === 'grok') {
    const kimi = plan.analysis.ruleId === KIMI_GROK_RULE_ID;
    const alias = kimi ? 'agenthub_kimi' : 'agenthub_openai';
    const model = kimi ? 'kimi-k2.5' : 'gpt-4o';
    const baseUrl = kimi ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL;
    const prefix = kimi ? 'kimi-grok' : 'openai-grok';
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${prefix}-${safeId}`,
      name: `${kimi ? 'Kimi' : 'OpenAI'} → Grok (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: 'grok',
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `grok-${prefix}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'grok',
        name: profile.name,
        preset: 'openai-chat',
        configText: [
          '[models]',
          `default = "${alias}"`,
          '',
          `[model."${alias}"]`,
          `model = "${model}"`,
          `base_url = "${baseUrl}"`,
          `api_key = "${CONNECTION_SECRET_MARKER}"`,
          'api_backend = "chat_completions"',
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }
  if (plan.analysis.route === 'native_endpoint' && request.targetAgentId === 'codex') {
    const glm = plan.analysis.ruleId === GLM_CODEX_RULE_ID;
    const slug = glm ? GLM_CODEX_PROVIDER_SLUG : DEEPSEEK_CODEX_PROVIDER_SLUG;
    const model = glm ? 'glm-5.3' : 'deepseek-v4-flash';
    const baseUrl = glm ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL;
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${glm ? 'glm' : 'deepseek'}-codex-${safeId}`,
      name: `${glm ? 'GLM Coding Plan' : 'DeepSeek'} → Codex (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'native_endpoint',
      mode: 'api',
      status: 'active',
      ruleId: plan.analysis.ruleId!,
      ruleVersion: '1',
      generatedProviderId: `codex-${glm ? 'glm' : 'deepseek'}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'codex',
        name: profile.name,
        preset: 'openai-compatible',
        configText: [
          `model_provider = "${slug}"`,
          `model = "${model}"`,
          'model_reasoning_effort = "high"',
          'preferred_auth_method = "apikey"',
          '',
          `[model_providers.${slug}]`,
          `name = "${glm ? 'GLM Coding Plan' : 'DeepSeek'}"`,
          `base_url = "${baseUrl}"`,
          'wire_api = "responses"',
          `experimental_bearer_token = "${CONNECTION_SECRET_MARKER}"`,
        ].join('\n'),
        configFormat: 'toml',
        isCurrent: true,
      },
    };
  }
  if (plan.analysis.route === 'local_bridge') {
    const codexClaudeBridge = plan.analysis.ruleId === CODEX_CLAUDE_RULE_ID;
    const grokClaudeBridge = plan.analysis.ruleId === GROK_CLAUDE_RULE_ID;
    const anthropicBridge = plan.analysis.ruleId === 'anthropic-api-to-codex-v1';
    const profile: AdapterProfile = existing ?? {
      id: codexClaudeBridge
        ? `adapter-codex-claude-bridge-${safeId}`
        : grokClaudeBridge
        ? `adapter-grok-claude-bridge-${safeId}`
        : anthropicBridge
        ? `adapter-anthropic-codex-bridge-${safeId}`
        : `adapter-kimi-codex-bridge-${safeId}`,
      name: codexClaudeBridge
        ? `Codex → Claude Code 本地桥接 (${safeId})`
        : grokClaudeBridge
        ? `Grok → Claude Code 本地桥接 (${safeId})`
        : anthropicBridge
        ? `Anthropic → Codex 本地桥接 (${safeId})`
        : `Kimi → Codex 本地桥接 (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'local_bridge',
      mode: codexClaudeBridge || grokClaudeBridge ? 'oauth' : 'api',
      status: 'active',
      ruleId: codexClaudeBridge
        ? CODEX_CLAUDE_RULE_ID
        : grokClaudeBridge
        ? GROK_CLAUDE_RULE_ID
        : anthropicBridge ? 'anthropic-api-to-codex-v1' : 'kimi-membership-to-codex-v1',
      ruleVersion: '1',
      generatedProviderId: codexClaudeBridge
        ? `claude-codex-bridge-${safeId}`
        : grokClaudeBridge
        ? `claude-grok-bridge-${safeId}`
        : anthropicBridge
        ? `codex-anthropic-bridge-${safeId}`
        : `codex-kimi-bridge-${safeId}`,
      localPort: 32123,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: codexClaudeBridge || grokClaudeBridge ? 'claude' : 'codex',
        name: profile.name,
        preset: codexClaudeBridge || grokClaudeBridge ? 'anthropic' : 'openai-compatible',
        configText: JSON.stringify({
          ...(codexClaudeBridge || grokClaudeBridge
            ? {
                env: {
                  ANTHROPIC_BASE_URL: `http://127.0.0.1:${profile.localPort ?? 32123}`,
                  ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
                },
              }
            : {
                baseUrl: `http://127.0.0.1:${profile.localPort ?? 32123}/v1`,
                model: anthropicBridge ? 'claude-sonnet-4-20250514' : 'kimi-k2.5',
              }),
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (plan.analysis.route === 'config_sync' && request.targetAgentId === 'dsh') {
    const profile: AdapterProfile = existing ?? {
      id: `adapter-deepseek-dsh-${safeId}`,
      name: `DeepSeek → DSH (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: 'api',
      status: 'active',
      ruleId: 'deepseek-api-to-dsh-v1',
      ruleVersion: '1',
      generatedProviderId: `dsh-deepseek-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'dsh',
        name: profile.name,
        preset: 'deepseek',
        configText: JSON.stringify({
          provider: 'deepseek-official',
          apiKeyEnv: 'DEEPSEEK_API_KEY',
          apiKey: CONNECTION_SECRET_MARKER,
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  if (plan.analysis.route === 'config_sync' && request.targetAgentId === 'pi') {
    const ruleId = plan.analysis.ruleId
      ?? (plan.analysis.actions.find((item) => item.kind === 'set_config')?.value === 'anthropic'
        ? 'anthropic-api-to-pi-v1'
        : 'kimi-membership-to-pi-v1');
    const slotFallback = ruleId === 'openai-api-to-pi-v1'
      ? 'openai'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xai'
        : ruleId === 'glm-coding-plan-to-pi-v1'
          ? 'glm-coding-plan'
          : ruleId === 'deepseek-api-to-pi-v1'
            ? 'deepseek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'anthropic'
          : 'kimi-for-coding';
    const display = ruleId === 'openai-api-to-pi-v1'
      ? 'OpenAI'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xAI'
        : ruleId === 'glm-coding-plan-to-pi-v1'
          ? 'GLM Coding Plan'
          : ruleId === 'deepseek-api-to-pi-v1'
            ? 'DeepSeek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'Anthropic'
          : 'Kimi';
    const subscription = NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(ruleId);
    const subscriptionDisplay = ruleId === 'claude-subscription-to-pi-v1'
      ? 'Claude'
      : ruleId === 'codex-subscription-to-pi-v1'
        ? 'Codex / ChatGPT'
        : 'Grok / xAI';
    const prefix = subscription
      ? ruleId === 'claude-subscription-to-pi-v1'
        ? 'claude-oauth'
        : ruleId === 'codex-subscription-to-pi-v1'
          ? 'codex-oauth'
          : 'grok-oauth'
      : ruleId === 'openai-api-to-pi-v1'
      ? 'openai'
      : ruleId === 'xai-api-to-pi-v1'
        ? 'xai'
      : ruleId === 'glm-coding-plan-to-pi-v1'
        ? 'glm'
        : ruleId === 'deepseek-api-to-pi-v1'
          ? 'deepseek'
        : ruleId === 'anthropic-api-to-pi-v1'
          ? 'anthropic'
          : 'kimi';
    const slot = piSlotFromPlan(plan, slotFallback);
    const piCustom = ruleId === 'glm-coding-plan-to-pi-v1' || ruleId === 'deepseek-api-to-pi-v1';
    const piBaseUrl = ruleId === 'glm-coding-plan-to-pi-v1' ? GLM_PI_BASE_URL : DEEPSEEK_PI_BASE_URL;
    const piModel = ruleId === 'glm-coding-plan-to-pi-v1' ? 'glm-4.6' : 'deepseek-chat';
    const profile: AdapterProfile = existing ?? {
      id: `adapter-${prefix}-pi-${safeId}`,
      name: `${subscription ? subscriptionDisplay : display} → Pi (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'config_sync',
      mode: subscription ? 'oauth' : 'api',
      status: 'active',
      ruleId,
      ruleVersion: '1',
      generatedProviderId: `pi-${prefix}-adapter-${safeId}`,
      localPort: null,
      autoStart: false,
      createdAt: now,
      updatedAt: now,
    };
    return {
      profile,
      provider: {
        id: profile.generatedProviderId!,
        agentId: 'pi',
        name: profile.name,
        preset: slot,
        configText: JSON.stringify(subscription
          ? {
              auth: {
                [slot]: {
                  type: 'oauth',
                  access: CONNECTION_SECRET_MARKER,
                  refresh: CONNECTION_SECRET_MARKER,
                },
              },
            }
          : piCustom
            ? {
                models: {
                  providers: {
                    [slot]: {
                      baseUrl: piBaseUrl,
                      api: 'openai-completions',
                      models: [{ id: piModel }],
                      apiKey: CONNECTION_SECRET_MARKER,
                    },
                  },
                },
              }
            : {
                slot,
                apiKey: CONNECTION_SECRET_MARKER,
              }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  const claudeLayout = plan.analysis.ruleId === GLM_CLAUDE_RULE_ID
    ? { prefix: 'glm', display: 'GLM', baseUrl: GLM_CLAUDE_BASE_URL, ruleId: GLM_CLAUDE_RULE_ID }
    : plan.analysis.ruleId === DEEPSEEK_CLAUDE_RULE_ID
      ? { prefix: 'deepseek', display: 'DeepSeek', baseUrl: DEEPSEEK_CLAUDE_BASE_URL, ruleId: DEEPSEEK_CLAUDE_RULE_ID }
      : { prefix: 'kimi', display: 'Kimi', baseUrl: KIMI_CLAUDE_BASE_URL, ruleId: 'kimi-membership-to-claude-v1' };
  const profile: AdapterProfile = existing ?? {
    id: `adapter-${claudeLayout.prefix}-claude-${safeId}`,
    name: `${claudeLayout.display} → Claude (${safeId})`,
    sourceKind: request.sourceKind,
    sourceId: request.sourceId,
    targetAgentId: request.targetAgentId,
    route: 'native_endpoint',
    mode: 'api',
    status: 'active',
    ruleId: claudeLayout.ruleId,
    ruleVersion: '1',
    generatedProviderId: `claude-${claudeLayout.prefix}-adapter-${safeId}`,
    localPort: null,
    autoStart: false,
    createdAt: now,
    updatedAt: now,
  };
  return {
    profile,
    provider: {
      id: profile.generatedProviderId!,
      agentId: 'claude',
      name: profile.name,
      preset: 'anthropic-compatible',
      configText: JSON.stringify({
        env: {
          ANTHROPIC_BASE_URL: claudeLayout.baseUrl,
          ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
        },
      }),
      configFormat: 'json',
      isCurrent: true,
    },
  };
}
