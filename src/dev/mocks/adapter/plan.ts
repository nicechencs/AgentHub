import type {
  AdapterApplyPlan,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import {
  CLAUDE_NATIVE_EXPERIMENTAL_RULES,
  CODEX_CLAUDE_RULE_ID,
  DEEPSEEK_CLAUDE_BASE_URL,
  DEEPSEEK_CLAUDE_RULE_ID,
  DEEPSEEK_CODEX_BASE_URL,
  EXPLICIT_API_TO_CODEX_RULES,
  EXPLICIT_API_TO_PI_RULES,
  GLM_CLAUDE_BASE_URL,
  GLM_CLAUDE_RULE_ID,
  GLM_CODEX_BASE_URL,
  GLM_CODEX_RULE_ID,
  GROK_CLAUDE_RULE_ID,
  GROK_NATIVE_RULE_IDS,
  KIMI_CLAUDE_BASE_URL,
  KIMI_GROK_BASE_URL,
  KIMI_GROK_RULE_ID,
  KIMI_MEMBERSHIP_RULE_IDS,
  NATIVE_SUBSCRIPTION_PI_RULE_IDS,
  OPENAI_GROK_BASE_URL,
  SAME_EDGE_UNWRITABLE_REASON,
  change,
  hasAccountApiKey,
  isKimiMembershipAccount,
  secretChange,
  type ClassifiableAccount,
  type MockAdapterSourceResolver,
} from './types';

export function buildPlan(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
  analysis: AdapterRouteAnalysis,
): AdapterApplyPlan {
  const configuredProvider = analysis.actions.find(
    (item) => item.kind === 'set_config' && item.target === 'Pi',
  )?.value;
  const claudeBaseUrl = analysis.ruleId === GLM_CLAUDE_RULE_ID
    ? GLM_CLAUDE_BASE_URL
    : analysis.ruleId === DEEPSEEK_CLAUDE_RULE_ID
      ? DEEPSEEK_CLAUDE_BASE_URL
      : KIMI_CLAUDE_BASE_URL;
  const changes = analysis.route === 'native_endpoint' && request.targetAgentId === 'grok'
    ? [
        change(
          'grok',
          'baseUrl',
          analysis.ruleId === KIMI_GROK_RULE_ID ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL,
        ),
        change('grok', 'model', analysis.ruleId === KIMI_GROK_RULE_ID ? 'kimi-k2.5' : 'gpt-4o'),
        change('grok', 'apiBackend', 'chat_completions'),
        secretChange('grok', 'apiKey'),
      ]
    : analysis.route === 'native_endpoint' && request.targetAgentId === 'claude'
    ? [
        change('claude', 'baseUrl', claudeBaseUrl),
        change('claude', 'claudeAuthEnv', 'ANTHROPIC_AUTH_TOKEN'),
        secretChange('claude', 'apiKey'),
      ]
      : analysis.route === 'local_bridge' && request.targetAgentId === 'codex'
        ? [
            change(
              'codex',
              'provider',
              analysis.ruleId === 'anthropic-api-to-codex-v1'
                ? 'AgentHub Anthropic 本地桥接'
                : 'AgentHub Kimi 本地桥接',
            ),
            change('codex', 'baseUrl', 'http://127.0.0.1:<本机端口>/v1'),
          ]
        : analysis.route === 'local_bridge' && request.targetAgentId === 'claude'
          ? [
              change('claude', 'ANTHROPIC_BASE_URL', 'http://127.0.0.1:<本机端口>'),
              secretChange('claude', 'ANTHROPIC_AUTH_TOKEN'),
            ]
        : analysis.route === 'native_endpoint' && request.targetAgentId === 'codex'
          ? [
              change('codex', 'provider', analysis.ruleId === GLM_CODEX_RULE_ID ? 'GLM Coding Plan' : 'DeepSeek API'),
              change('codex', 'baseUrl', analysis.ruleId === GLM_CODEX_RULE_ID ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL),
              change('codex', 'wireApi', 'responses'),
            ]
        : analysis.route === 'config_sync' && request.targetAgentId === 'pi'
      ? [
          change('pi', 'provider', configuredProvider ?? 'anthropic'),
          secretChange(
            'pi',
            analysis.ruleId && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId)
              ? 'auth'
              : 'apiKey',
          ),
        ]
        : analysis.route === 'config_sync' && request.targetAgentId === 'dsh'
      ? [
          change('dsh', 'provider', 'deepseek-official'),
          change('dsh', 'apiKeyEnv', 'DEEPSEEK_API_KEY'),
          secretChange('dsh', 'apiKey'),
        ]
      : [];
  const implementedPath =
    (analysis.route === 'native_endpoint' && analysis.support === 'stable' && request.targetAgentId === 'claude')
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && (request.targetAgentId === 'claude' || request.targetAgentId === 'codex')
      && !!analysis.ruleId && CLAUDE_NATIVE_EXPERIMENTAL_RULES.has(analysis.ruleId))
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && request.targetAgentId === 'codex'
      && !!analysis.ruleId && EXPLICIT_API_TO_CODEX_RULES.has(analysis.ruleId))
    || (analysis.route === 'native_endpoint' && analysis.support === 'experimental'
      && request.targetAgentId === 'grok'
      && !!analysis.ruleId && GROK_NATIVE_RULE_IDS.has(analysis.ruleId))
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental' && request.targetAgentId === 'codex')
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental'
      && request.sourceKind === 'account'
      && request.targetAgentId === 'claude'
      && analysis.ruleId === CODEX_CLAUDE_RULE_ID
      && hasCodexAccessToken(resolver, request.sourceId))
    || (analysis.route === 'local_bridge' && analysis.support === 'experimental'
      && request.sourceKind === 'account'
      && request.targetAgentId === 'claude'
      && analysis.ruleId === GROK_CLAUDE_RULE_ID
      && hasGrokAccessToken(resolver, request.sourceId))
    || (analysis.route === 'config_sync' && analysis.support === 'stable' && request.targetAgentId === 'pi')
    || (analysis.route === 'config_sync' && analysis.support === 'experimental'
      && request.targetAgentId === 'pi'
      && !!analysis.ruleId
      && EXPLICIT_API_TO_PI_RULES.has(analysis.ruleId))
    || (analysis.route === 'config_sync' && analysis.support === 'experimental'
      && request.targetAgentId === 'pi'
      && !!analysis.ruleId
      && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId))
    || (analysis.route === 'config_sync' && analysis.support === 'stable' && request.targetAgentId === 'dsh');
  const accountSource = request.sourceKind === 'account'
    ? resolver.getAccountById(request.sourceId) as ClassifiableAccount | undefined
    : undefined;
  const accountExplicitApiToPi = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'pi'
    && !!analysis.ruleId
    && EXPLICIT_API_TO_PI_RULES.has(analysis.ruleId);
  const accountNativeSubscriptionPi = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'pi'
    && !!analysis.ruleId
    && NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId);
  const accountExplicitApiToCodex = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'codex'
    && !!analysis.ruleId
    && EXPLICIT_API_TO_CODEX_RULES.has(analysis.ruleId);
  const accountClaudeNative = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && !!analysis.ruleId
    && CLAUDE_NATIVE_EXPERIMENTAL_RULES.has(analysis.ruleId);
  const accountKimiMembership = request.sourceKind === 'account'
    && implementedPath
    && !!analysis.ruleId
    && KIMI_MEMBERSHIP_RULE_IDS.has(analysis.ruleId)
    && isKimiMembershipAccount(accountSource)
    && hasAccountApiKey(accountSource);
  const accountCodexClaudeBridge = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && analysis.ruleId === CODEX_CLAUDE_RULE_ID
    && hasCodexAccessToken(resolver, request.sourceId);
  const accountGrokNative = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'grok'
    && !!analysis.ruleId
    && GROK_NATIVE_RULE_IDS.has(analysis.ruleId)
    && hasAccountApiKey(accountSource);
  const accountGrokClaudeBridge = request.sourceKind === 'account'
    && implementedPath
    && request.targetAgentId === 'claude'
    && analysis.ruleId === GROK_CLAUDE_RULE_ID
    && hasGrokAccessToken(resolver, request.sourceId);
  const writeGate = (request.sourceKind === 'provider' && implementedPath)
    || accountKimiMembership
    || accountGrokNative
    || accountExplicitApiToPi
    || accountExplicitApiToCodex
    || accountClaudeNative
    || accountNativeSubscriptionPi
    || accountCodexClaudeBridge
    || accountGrokClaudeBridge;
  const canApply = writeGate;
  const maturity = mockPlanMaturity(analysis);
  const reusePath = NATIVE_SUBSCRIPTION_PI_RULE_IDS.has(analysis.ruleId ?? '')
    ? 'native_subscription' as const
    : analysis.route === 'unsupported'
      ? 'none' as const
      : analysis.route === 'local_bridge'
        ? 'local_bridge' as const
        : 'api_endpoint' as const;
  // Same-edge Account stays closed except explicit API → Pi / Codex.
  const reason = implementedPath && request.sourceKind !== 'provider'
    && !accountExplicitApiToPi
    && !accountExplicitApiToCodex
    && !accountClaudeNative
    && !accountKimiMembership
    && !accountGrokNative
    && !accountNativeSubscriptionPi
    && !accountCodexClaudeBridge
    && !accountGrokClaudeBridge
    ? `${analysis.reason} ${SAME_EDGE_UNWRITABLE_REASON}`
    : analysis.reason;
  return {
    analysis,
    targetAgentId: request.targetAgentId,
    canApply,
    maturity,
    reusePath,
    reason,
    serviceImpact: analysis.route === 'local_bridge' ? 'requires_local_bridge' : 'none',
    changes,
  };
}

export function hasCodexAccessToken(
  resolver: MockAdapterSourceResolver,
  sourceId: string,
): boolean {
  const account = resolver.getAccountById(sourceId) as ClassifiableAccount | undefined;
  if (!account || account.agentId !== 'codex' || account.kind !== 'oauth') return false;
  const credentials = account.credentials;
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const tokens = record.tokens;
  if (tokens && typeof tokens === 'object' && !Array.isArray(tokens)) {
    const accessToken = (tokens as Record<string, unknown>).access_token;
    if (typeof accessToken === 'string' && accessToken.trim()) return true;
  }
  const body = record.body;
  if (body && typeof body === 'object' && !Array.isArray(body)) {
    const bodyTokens = (body as Record<string, unknown>).tokens;
    if (bodyTokens && typeof bodyTokens === 'object' && !Array.isArray(bodyTokens)) {
      const accessToken = (bodyTokens as Record<string, unknown>).access_token;
      return typeof accessToken === 'string' && Boolean(accessToken.trim());
    }
  }
  return false;
}

export function hasGrokAccessToken(
  resolver: MockAdapterSourceResolver,
  sourceId: string,
): boolean {
  const account = resolver.getAccountById(sourceId) as ClassifiableAccount | undefined;
  if (!account || account.agentId !== 'grok' || account.kind !== 'oauth') return false;
  const credentials = account.credentials;
  if (!credentials || typeof credentials !== 'object') return false;
  const record = credentials as Record<string, unknown>;
  const candidates = [
    record.access_token,
    (record.tokens as Record<string, unknown> | undefined)?.access_token,
    ((record.body as Record<string, unknown> | undefined)?.tokens as Record<string, unknown> | undefined)?.access_token,
  ];
  return candidates.some((token) => typeof token === 'string' && Boolean(token.trim()));
}

/** Mirror of core `adapter_maturity_from_decision` on the public analysis surface. */
export function mockPlanMaturity(analysis: AdapterRouteAnalysis): AdapterApplyPlan['maturity'] {
  const matrixOpen = analysis.route !== 'unsupported' && analysis.support !== 'unsupported';
  if (analysis.gateKind === 'preview_only') return 'preview';
  if (matrixOpen && analysis.support === 'stable') return 'stable';
  if (matrixOpen && analysis.support === 'experimental') return 'experimental';
  if (analysis.gateKind === 'subscription_candidate') {
    return 'preview';
  }
  return 'none';
}
