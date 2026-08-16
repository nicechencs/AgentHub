import {
  adapterCommandError,
  type AdapterRouteAnalysis,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { analysisFromFixture, findRuleFixture } from './rule-fixtures';
import { classify } from './classify';
import {
  AGENT_NO_WRITER_REASON,
  CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
  CODEX_CLAUDE_RULE_ID,
  CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON,
  CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
  GROK_CLAUDE_RULE_ID,
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
      reason: 'Kimi Code 会员到 Codex 需要本地协议桥接。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex Responses 与 Kimi Chat Completions 需要本地双向协议转换。')],
      limitations: [
        '将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '桥接为实验性协议覆盖；长流与工具调用可能受实现限制。',
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
      reason: '显式 Anthropic API Key 到 Codex 需要本地协议桥接。',
      actions: [action('requires_local_bridge', 'Codex', 'Codex Responses 与 Anthropic Messages 需要本地双向协议转换。')],
      limitations: [
        '将在本机 loopback 启动协议桥接，并切换 Codex 到该本地端点。',
        'AgentHub 需保持在托盘运行；退出前会尝试排空监听。',
        '桥接为实验性协议覆盖：下游 Responses，上游 Anthropic Messages。',
        '固定端口被占用时会尝试重新分配端口并写回配置。',
      ],
      evidence: [evidence('Anthropic Messages API', 'https://docs.anthropic.com/en/api/messages')],
      ruleId: 'anthropic-api-to-codex-v1',
      gateKind: 'none',
    };
  }

  if (source === 'claude_subscription' && request.targetAgentId === 'codex') {
    return unsupported(CLAUDE_SUBSCRIPTION_TO_CODEX_REASON, compatibilityEvidence);
  }
  if (source === 'grok_xai_subscription' && request.targetAgentId === 'claude') {
    return {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'Grok 订阅可通过本机路由到 Claude Code（Messages → xAI Chat Completions）。',
      actions: [
        action(
          'requires_local_bridge',
          'Claude Code',
          'Claude Messages 与 xAI Chat Completions 需要本地双向协议转换。',
        ),
        action(
          'set_env',
          'Claude Code',
          '写入 Claude Code 的 loopback Base URL 与本机 bearer；不会写入上游 OAuth token。',
          'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
        ),
      ],
      limitations: [
        'Claude 只写入本机 loopback URL 与本地 bearer；xAI OAuth token 不进入 Claude 配置、IPC 或日志。',
        '实验性协议桥接：Claude Messages → xAI Chat Completions；AgentHub 需保持在托盘运行。',
        'Grok access token 过期后需重新同步 Grok 登录；Hub 本轮不自动 refresh。',
      ],
      evidence: compatibilityEvidence,
      ruleId: GROK_CLAUDE_RULE_ID,
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
            'Claude Messages 与 Codex Responses 需要本地双向协议转换。',
          ),
          action(
            'set_env',
            'Claude Code',
            '写入 Claude Code 的 loopback Base URL 与本机 bearer；不会写入上游 OAuth token。',
            'ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN',
          ),
        ],
        limitations: [
          '会把 Claude 的 ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN 指向本机 loopback；上游 token 不进 Claude。',
          '实验性协议桥接：Claude Messages → Codex Responses；AgentHub 需保持在托盘运行。',
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
