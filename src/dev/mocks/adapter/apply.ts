import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterProfile,
} from '@/lib/backend/contracts/adapter';
import type { Provider } from '@/lib/types';
import { getRuleFixtureById, type MockMaterializeSpec } from './rule-fixtures';
import {
  CODEX_CLAUDE_RULE_ID,
  GROK_CLAUDE_RULE_ID,
  GROK_CODEX_RULE_ID,
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

function materializeFromSpec(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
  safeId: string,
  ruleId: string,
  spec: MockMaterializeSpec,
): { profile: AdapterProfile; provider: Provider } {
  switch (spec.kind) {
    case 'grok_chat': {
      const profile: AdapterProfile = existing ?? {
        id: `adapter-${spec.prefix}-${safeId}`,
        name: `${spec.display} → Grok (${safeId})`,
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: 'grok',
        route: 'native_endpoint',
        mode: 'api',
        status: 'active',
        ruleId: plan.analysis.ruleId!,
        ruleVersion: '1',
        generatedProviderId: `grok-${spec.prefix}-adapter-${safeId}`,
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
            `default = "${spec.alias}"`,
            '',
            `[model."${spec.alias}"]`,
            `model = "${spec.model}"`,
            `base_url = "${spec.baseUrl}"`,
            `api_key = "${CONNECTION_SECRET_MARKER}"`,
            'api_backend = "chat_completions"',
          ].join('\n'),
          configFormat: 'toml',
          isCurrent: true,
        },
      };
    }
    case 'codex_responses': {
      const profile: AdapterProfile = existing ?? {
        id: `adapter-${spec.prefix}-codex-${safeId}`,
        name: `${spec.display} → Codex (${safeId})`,
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: request.targetAgentId,
        route: 'native_endpoint',
        mode: 'api',
        status: 'active',
        ruleId: plan.analysis.ruleId!,
        ruleVersion: '1',
        generatedProviderId: `codex-${spec.prefix}-adapter-${safeId}`,
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
            `model_provider = "${spec.slug}"`,
            `model = "${spec.model}"`,
            'model_reasoning_effort = "high"',
            'preferred_auth_method = "apikey"',
            '',
            `[model_providers.${spec.slug}]`,
            `name = "${spec.display}"`,
            `base_url = "${spec.baseUrl}"`,
            'wire_api = "responses"',
            `experimental_bearer_token = "${CONNECTION_SECRET_MARKER}"`,
          ].join('\n'),
          configFormat: 'toml',
          isCurrent: true,
        },
      };
    }
    case 'claude_native': {
      const profile: AdapterProfile = existing ?? {
        id: `adapter-${spec.prefix}-claude-${safeId}`,
        name: `${spec.display} → Claude (${safeId})`,
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: request.targetAgentId,
        route: 'native_endpoint',
        mode: 'api',
        status: 'active',
        ruleId,
        ruleVersion: '1',
        generatedProviderId: `claude-${spec.prefix}-adapter-${safeId}`,
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
              ANTHROPIC_BASE_URL: spec.baseUrl,
              ANTHROPIC_AUTH_TOKEN: CONNECTION_SECRET_MARKER,
            },
          }),
          configFormat: 'json',
          isCurrent: true,
        },
      };
    }
    case 'dsh': {
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
    case 'pi_slot':
    case 'pi_custom':
    case 'pi_subscription': {
      const subscription = spec.kind === 'pi_subscription';
      const piCustom = spec.kind === 'pi_custom';
      const slot = piSlotFromPlan(plan, spec.slot);
      const profile: AdapterProfile = existing ?? {
        id: `adapter-${spec.prefix}-pi-${safeId}`,
        name: `${spec.display} → Pi (${safeId})`,
        sourceKind: request.sourceKind,
        sourceId: request.sourceId,
        targetAgentId: request.targetAgentId,
        route: 'config_sync',
        mode: subscription ? 'oauth' : 'api',
        status: 'active',
        ruleId,
        ruleVersion: '1',
        generatedProviderId: `pi-${spec.prefix}-adapter-${safeId}`,
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
                        baseUrl: spec.baseUrl,
                        api: 'openai-completions',
                        models: [{ id: spec.model }],
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
  }
}

export function materializeApply(
  request: AdapterApplyRequest,
  plan: AdapterApplyPlan,
  existing: AdapterProfile | undefined,
  now: string,
): { profile: AdapterProfile; provider: Provider } {
  const safeId = safeSourceId(request.sourceId);

  // local_bridge 控制流：不进 reshape fixture 表。
  if (plan.analysis.route === 'local_bridge') {
    const codexClaudeBridge = plan.analysis.ruleId === CODEX_CLAUDE_RULE_ID;
    const grokClaudeBridge = plan.analysis.ruleId === GROK_CLAUDE_RULE_ID;
    const grokCodexBridge = plan.analysis.ruleId === GROK_CODEX_RULE_ID;
    const anthropicBridge = plan.analysis.ruleId === 'anthropic-api-to-codex-v1';
    const profile: AdapterProfile = existing ?? {
      id: codexClaudeBridge
        ? `adapter-codex-claude-bridge-${safeId}`
        : grokClaudeBridge
        ? `adapter-grok-claude-bridge-${safeId}`
        : grokCodexBridge
        ? `adapter-grok-codex-bridge-${safeId}`
        : anthropicBridge
        ? `adapter-anthropic-codex-bridge-${safeId}`
        : `adapter-kimi-codex-bridge-${safeId}`,
      name: codexClaudeBridge
        ? `Codex → Claude Code 本地桥接 (${safeId})`
        : grokClaudeBridge
        ? `Grok → Claude Code 本地桥接 (${safeId})`
        : grokCodexBridge
        ? `Grok → Codex 本机路由 (${safeId})`
        : anthropicBridge
        ? `Anthropic → Codex 本地桥接 (${safeId})`
        : `Kimi → Codex 本地桥接 (${safeId})`,
      sourceKind: request.sourceKind,
      sourceId: request.sourceId,
      targetAgentId: request.targetAgentId,
      route: 'local_bridge',
      mode: codexClaudeBridge || grokClaudeBridge || grokCodexBridge ? 'oauth' : 'api',
      status: 'active',
      ruleId: codexClaudeBridge
        ? CODEX_CLAUDE_RULE_ID
        : grokClaudeBridge
        ? GROK_CLAUDE_RULE_ID
        : grokCodexBridge
        ? GROK_CODEX_RULE_ID
        : anthropicBridge ? 'anthropic-api-to-codex-v1' : 'kimi-membership-to-codex-v1',
      ruleVersion: '1',
      generatedProviderId: codexClaudeBridge
        ? `claude-codex-bridge-${safeId}`
        : grokClaudeBridge
        ? `claude-grok-bridge-${safeId}`
        : grokCodexBridge
        ? `codex-grok-bridge-${safeId}`
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
                model: anthropicBridge ? 'claude-sonnet-4-20250514' : grokCodexBridge ? 'grok-4.5' : 'kimi-k2.5',
              }),
        }),
        configFormat: 'json',
        isCurrent: true,
      },
    };
  }

  const resolvedRuleId = plan.analysis.ruleId
    ?? (plan.analysis.route === 'config_sync' && request.targetAgentId === 'pi'
      ? (plan.analysis.actions.find((item) => item.kind === 'set_config')?.value === 'anthropic'
        ? 'anthropic-api-to-pi-v1'
        : 'kimi-membership-to-pi-v1')
      : null);
  const fixture = resolvedRuleId ? getRuleFixtureById(resolvedRuleId) : undefined;
  if (fixture) {
    return materializeFromSpec(
      request,
      plan,
      existing,
      now,
      safeId,
      fixture.ruleId,
      fixture.materialize,
    );
  }

  // Fallback: preserve prior Claude native default (kimi) if analysis lacked ruleId.
  const fallback = getRuleFixtureById('kimi-membership-to-claude-v1')!;
  return materializeFromSpec(
    request,
    plan,
    existing,
    now,
    safeId,
    fallback.ruleId,
    fallback.materialize,
  );
}
