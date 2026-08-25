import {
  adapterCommandError,
  type AdapterRouteAnalysis,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { lookupGoldenExpect, overlayAnalysisFromExpect } from './golden-lookup';
import { analysisFromFixture, findRuleFixture } from './rule-fixtures';
import { classify } from './classify';
import {
  AGENT_NO_WRITER_REASON,
  CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
  CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID,
  CODEX_CLAUDE_RULE_ID,
  CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON,
  CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
  CODEX_SUBSCRIPTION_TO_CODEX_REASON,
  CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID,
  CODEX_DSH_RULE_ID,
  CODEX_GROK_RULE_ID,
  CODEX_KIMI_RULE_ID,
  CODEX_SUBSCRIPTION_TO_DSH_REASON,
  CODEX_SUBSCRIPTION_TO_GROK_REASON,
  CODEX_SUBSCRIPTION_TO_KIMI_REASON,
  GROK_CLAUDE_RULE_ID,
  GROK_CODEX_RULE_ID,
  GROK_SUBSCRIPTION_TO_CLAUDE_REASON,
  GROK_SUBSCRIPTION_TO_CODEX_REASON,
  GROK_SUBSCRIPTION_TO_DSH_REASON,
  GROK_SUBSCRIPTION_TO_KIMI_REASON,
  KIMI_NON_MEMBERSHIP_REASON,
  action,
  agentBindCapability,
  evidence,
  unsupported,
  unsupportedReasonFromGraph,
  type MockAdapterSourceResolver,
} from './types';

export function analyze(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  const fallback = analyzeFromClassifier(resolver, request);
  const hit = lookupGoldenExpect(resolver, request);
  if (!hit) return fallback;
  return overlayAnalysisFromExpect(fallback, hit.expect);
}

function analyzeFromClassifier(
  resolver: MockAdapterSourceResolver,
  request: AdapterRouteRequest,
): AdapterRouteAnalysis {
  const source = classify(resolver, request);
  if (source === 'not_found') {
    throw adapterCommandError({
      code: 'not_found',
      message: `${request.sourceKind} not found: ${request.sourceId}`,
      retryable: false,
    });
  }
  const compatibilityEvidence = [evidence(
    'AgentHub：厂商、API 与 OAuth 适配规则',
    'https://github.com/nicechencs/AgentHub/blob/release/docs/provider-api-oauth-adaptation.md',
  )];
  // Bind-entry table first: no writer → infeasible. Cursor must take this path.
  if (!agentBindCapability(request.targetAgentId).writer) {
    return unsupported(AGENT_NO_WRITER_REASON, compatibilityEvidence);
  }

  // Reshape arms: ruleId fixture 表（mock 投影，非规则真源）。
  const fixture = findRuleFixture(source, request.targetAgentId);
  if (fixture) {
    return analysisFromFixture(fixture, compatibilityEvidence);
  }

  // —— 以下保留必须留下的控制流：local_bridge / subscription 互转 / 关闭 cell ——
  if (source === 'kimi_membership' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'Kimi Code 会员接到 Codex 需要本机转发。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex 和 Kimi 说的话对不上，需要本机转发。')],
      limitations: [
        '将在本机地址启动本机转发，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '本机转发仍是实验性覆盖；长流与工具调用可能受实现限制。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('Kimi Code: Codex local routing', 'https://www.kimi.com/code/docs/third-party-tools/codex.html')],
      ruleId: 'kimi-membership-to-codex-v1',
      gateKind: 'none',
    };
  }
  if (source === 'anthropic_api_key' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: '这份 Anthropic API Key 接到 Codex 需要本机转发。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex 和 Anthropic 说的话对不上，需要本机转发。')],
      limitations: [
        '将在本机地址启动本机转发，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '本机转发仍是实验性覆盖：下游 Responses，上游 Anthropic Messages。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('Anthropic Messages API', 'https://docs.anthropic.com/en/api/messages')],
      ruleId: 'anthropic-api-to-codex-v1',
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'claude') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: '这份 OpenAI 兼容登录接到 Claude 需要本机转发。',
      actions: [action('requires_local_bridge', 'Claude', 'Claude 和 OpenAI 兼容接口说的话对不上，需要本机转发。')],
      limitations: [
        '将在本机地址启动本机转发，并切换 Claude 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '本机转发：下游 Messages，上游 OpenAI Chat Completions。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('OpenAI Chat Completions API', 'https://platform.openai.com/docs/api-reference/chat')],
      ruleId: 'openai-api-to-claude-v1',
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'grok') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: '这份 OpenAI 兼容登录接到 Grok 需要本机转发。',
      actions: [action('requires_local_bridge', 'Grok', 'Grok 和 OpenAI 兼容接口说的话对不上，需要本机转发。')],
      limitations: [
        '将在本机地址启动本机转发，并切换 Grok 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '本机转发：下游 Responses，上游 OpenAI Chat Completions。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('OpenAI Chat Completions API', 'https://platform.openai.com/docs/api-reference/chat')],
      ruleId: 'openai-api-to-grok-bridge-v1',
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: '这份 OpenAI API Key 接到 Codex 需要本机转发。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex 和 OpenAI 说的话对不上，需要本机转发。')],
      limitations: [
        '将在本机地址启动本机转发，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '本机转发仍是实验性覆盖：下游 Responses，上游 OpenAI Chat Completions。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('OpenAI Chat Completions API', 'https://platform.openai.com/docs/api-reference/chat')],
      ruleId: 'openai-api-to-codex-v1',
      gateKind: 'none',
    };
  }

  if (source === 'claude_subscription' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
      actions: [],
      limitations: [
        '会把 Codex 指到本机路由；上游 Claude 订阅 token 不会写入 Codex。',
        '实验性本机转发：下游 Responses，上游 Anthropic Messages OAuth。',
        '规则还没做完，现在接不上；thinking 无签名时降级关闭。',
        'Claude access token 过期后需重新同步登录；Hub 本轮不自动 refresh。',
      ],
      evidence: compatibilityEvidence,
      ruleId: CLAUDE_SUBSCRIPTION_TO_CODEX_RULE_ID,
      gateKind: 'preview_only',
    };
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'kimi') {
    return unsupported(GROK_SUBSCRIPTION_TO_KIMI_REASON, compatibilityEvidence);
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'dsh') {
    return unsupported(GROK_SUBSCRIPTION_TO_DSH_REASON, compatibilityEvidence);
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'codex') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: GROK_SUBSCRIPTION_TO_CODEX_REASON,
      actions: [
        action(
          'requires_local_bridge',
          'Codex',
          '会把 Codex 指到本机路由；上游 Grok 登录不会写入 Codex。',
        ),
        action(
          'set_config',
          'Codex',
          '写入 Codex 的本机路由端点。',
          'AgentHub Grok 本机路由',
        ),
      ],
      limitations: [
        '会把 Codex 指到本机路由；上游 Grok 登录不会写入 Codex。',
        'AgentHub 需保持在托盘运行。',
        'Grok 登录过期后需重新同步；Hub 本轮不自动刷新。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: compatibilityEvidence,
      ruleId: GROK_CODEX_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'claude') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: GROK_SUBSCRIPTION_TO_CLAUDE_REASON,
      actions: [
        action(
          'requires_local_bridge',
          'Claude Code',
          'Claude 和 Grok 说的话对不上，需要本机转发。',
        ),
        action(
          'set_env',
          'Claude Code',
          '写入 Claude Code 的本机地址 Base URL 与本机 bearer；不会写入上游 OAuth token。',
          'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
        ),
      ],
      limitations: [
        '会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机地址；上游 xAI OAuth token 不进 Claude。',
        '实验性本机转发：Claude Messages → xAI Responses (cli-chat-proxy)；AgentHub 需保持在托盘运行。',
        'Grok access token 过期后需重新同步 Grok 登录；Hub 本轮不自动 refresh。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: compatibilityEvidence,
      ruleId: GROK_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  if (
    (source === 'codex_subscription' || source === 'codex_subscription_oauth_other')
    && request.targetAgentId === 'codex'
  ) {
    return {
      route: 'native_endpoint',
      support: 'stable',
      reason: CODEX_SUBSCRIPTION_TO_CODEX_REASON,
      actions: [
        action('set_config', 'Codex', '写入 Codex 官方登录，不改本机路由。', '官方登录'),
      ],
      limitations: [
        '会把这份官方登录写进 Codex；不会改到本机路由。',
        '应用后这份登录成为 Codex 当前登录。',
      ],
      evidence: compatibilityEvidence,
      ruleId: CODEX_SUBSCRIPTION_TO_CODEX_RULE_ID,
      gateKind: 'none',
    };
  }
  if (
    (source === 'codex_subscription' || source === 'codex_subscription_oauth_other')
    && (request.targetAgentId === 'grok' || request.targetAgentId === 'kimi' || request.targetAgentId === 'dsh')
  ) {
    const target = request.targetAgentId;
    const reason = target === 'grok'
      ? CODEX_SUBSCRIPTION_TO_GROK_REASON
      : target === 'kimi'
        ? CODEX_SUBSCRIPTION_TO_KIMI_REASON
        : CODEX_SUBSCRIPTION_TO_DSH_REASON;
    const ruleId = target === 'grok'
      ? CODEX_GROK_RULE_ID
      : target === 'kimi'
        ? CODEX_KIMI_RULE_ID
        : CODEX_DSH_RULE_ID;
    const label = target === 'dsh' ? 'DeepSeek Harness' : target === 'kimi' ? 'Kimi' : 'Grok';
    const loopback = target === 'dsh' ? 'http://127.0.0.1:<本机端口>' : 'http://127.0.0.1:<本机端口>/v1';
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason,
      actions: [
        action(
          'requires_local_bridge',
          label,
          `会把 ${label} 指到本机路由；上游 Codex 官方登录不会写入对方。`,
        ),
        action('set_config', label, `写入 ${label} 的本机路由端点。`, loopback),
      ],
      limitations: [
        '会把目标 Agent 指到本机路由；上游 Codex 官方登录不会写入对方。',
        'AgentHub 需保持在托盘运行。',
        'Codex 登录过期后需重新同步；Hub 本轮不自动刷新。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: compatibilityEvidence,
      ruleId,
      gateKind: 'none',
    };
  }
  // Only Codex auth_json opens the experimental Responses → Claude bridge.
  // Bare Codex OAuth remains a closed subscription candidate.
  if (
    (source === 'codex_subscription' || source === 'codex_subscription_oauth_other')
    && request.targetAgentId === 'claude'
  ) {
    if (source === 'codex_subscription') {
      return {
        route: 'local_bridge',
        support: 'experimental',
        reason: CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
        actions: [
          action(
            'requires_local_bridge',
            'Claude Code',
            'Claude 和 Codex 说的话对不上，需要本机转发。',
          ),
          action(
            'set_env',
            'Claude Code',
            '写入 Claude Code 的本机地址 Base URL 与本机 bearer；不会写入上游 OAuth token。',
            'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
          ),
        ],
        limitations: [
          '会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机地址；上游 token 不进 Claude。',
          '实验性本机转发：Claude Messages → Codex Responses；AgentHub 需保持在托盘运行。',
          'Codex access token 过期后需重新同步 Codex 登录；Hub 本轮不自动 refresh。',
          '固定端口被占用时会尝试重新分配端口并写回配置。',
        ],
        evidence: compatibilityEvidence,
        ruleId: CODEX_CLAUDE_RULE_ID,
        gateKind: 'none',
      };
    }
    return unsupported(CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON, compatibilityEvidence, {
      gateKind: 'subscription_candidate',
      ruleId: null,
    });
  }
  if (source === 'kimi_non_membership') {
    return unsupported(KIMI_NON_MEMBERSHIP_REASON, compatibilityEvidence);
  }
  if (source === 'other') {
    return unsupported(
      'AgentHub 暂未提供此来源到所选目标的适配规则。当前不支持不等于连接失效。',
      compatibilityEvidence,
    );
  }
  return unsupported(unsupportedReasonFromGraph(source, request.targetAgentId), compatibilityEvidence);
}
