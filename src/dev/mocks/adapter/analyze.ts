import {
  adapterCommandError,
  type AdapterRouteAnalysis,
  type AdapterRouteRequest,
} from '@/lib/backend/contracts/adapter';
import { classify } from './classify';
import {
  AGENT_NO_WRITER_REASON,
  CLAUDE_SUBSCRIPTION_TO_CODEX_REASON,
  CODEX_CLAUDE_RULE_ID,
  CODEX_SUBSCRIPTION_TO_CLAUDE_CANDIDATE_REASON,
  CODEX_SUBSCRIPTION_TO_CLAUDE_REASON,
  DEEPSEEK_CLAUDE_BASE_URL,
  DEEPSEEK_CLAUDE_RULE_ID,
  DEEPSEEK_CODEX_BASE_URL,
  DEEPSEEK_CODEX_RULE_ID,
  GLM_CLAUDE_BASE_URL,
  GLM_CLAUDE_RULE_ID,
  GLM_CODEX_BASE_URL,
  GLM_CODEX_RULE_ID,
  GROK_CLAUDE_RULE_ID,
  KIMI_GROK_BASE_URL,
  KIMI_GROK_RULE_ID,
  KIMI_NON_MEMBERSHIP_REASON,
  OPENAI_GROK_BASE_URL,
  OPENAI_GROK_RULE_ID,
  action,
  agentBindCapability,
  evidence,
  secretAction,
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
  if (source === 'kimi_membership' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'stable',
      reason: 'Kimi Code 会员可预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 Kimi Code 官方 Anthropic-compatible Base URL。', 'https://api.kimi.com/coding/'),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Kimi Code: Claude Code integration', 'https://www.kimi.com/code/docs/en/third-party-tools/claude-code.html')],
      ruleId: 'kimi-membership-to-claude-v1',
      gateKind: 'none',
    };
  }
  if (source === 'kimi_membership' && request.targetAgentId === 'grok') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'Kimi Code 会员可实验写入 Grok 的 OpenAI Chat Completions 配置。',
      actions: [
        action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', KIMI_GROK_BASE_URL),
        action('set_config', 'Grok', '使用 Grok Chat Completions 与 kimi-k2.5。', 'api_backend=chat_completions; model=kimi-k2.5'),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '只修改 Grok ~/.grok/config.toml 的官方 TOML provider；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: KIMI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'grok') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'OpenAI API 可实验写入 Grok 的官方 OpenAI Chat Completions 配置。',
      actions: [
        action('set_config', 'Grok', '写入 Grok 官方 OpenAI Chat Completions TOML。', OPENAI_GROK_BASE_URL),
        action('set_config', 'Grok', '使用 Grok Chat Completions 与 gpt-4o。', 'api_backend=chat_completions; model=gpt-4o'),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '只修改 Grok ~/.grok/config.toml 的官方 TOML provider；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: OPENAI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'glm_coding_plan' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'GLM Coding Plan 可实验预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 GLM Coding Plan 官方 Anthropic-compatible Base URL。', GLM_CLAUDE_BASE_URL),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 GLM Coding Plan Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
        '实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。',
      ],
      evidence: [evidence('GLM Coding Plan 接入工具与双协议端点', 'https://docs.bigmodel.cn/cn/coding-plan/tool/others')],
      ruleId: GLM_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  if (source === 'deepseek_api' && request.targetAgentId === 'claude') {
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: 'DeepSeek API 可实验预览为 Claude 的原生 Anthropic Messages 端点。',
      actions: [
        action('set_config', 'Claude Code', '设置 DeepSeek 官方 Anthropic-compatible Base URL。', DEEPSEEK_CLAUDE_BASE_URL),
        action('set_env', 'Claude Code', '使用 Claude Code 的认证环境变量名。', 'ANTHROPIC_AUTH_TOKEN'),
        secretAction('Claude Code', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Claude 的 DeepSeek Anthropic 兼容 Base URL 与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会切换当前 Claude Connection；请确认无其他进行中的配置写入。',
        '实验性：官方 Anthropic 兼容入口；部分扩展字段可能被忽略或不支持。',
      ],
      evidence: [evidence('DeepSeek 接入 Claude Code', 'https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/')],
      ruleId: DEEPSEEK_CLAUDE_RULE_ID,
      gateKind: 'none',
    };
  }
  if (
    (source === 'kimi_membership' || source === 'openai_api_key')
    && request.targetAgentId === 'grok'
  ) {
    const kimi = source === 'kimi_membership';
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: kimi
        ? 'Kimi Code 会员可实验写入 Grok 的 OpenAI Chat Completions 配置。'
        : 'OpenAI API 可实验写入 Grok 的官方 OpenAI Chat Completions 配置。',
      actions: [
        action(
          'set_config',
          'Grok',
          `写入 Grok 的${kimi ? ' Kimi Code' : ' OpenAI'} 官方 OpenAI Chat Completions 配置。`,
          kimi ? KIMI_GROK_BASE_URL : OPENAI_GROK_BASE_URL,
        ),
        action(
          'set_config',
          'Grok',
          '设置 Grok 模型与 Chat Completions backend。',
          `model=${kimi ? 'kimi-k2.5' : 'gpt-4o'}; api_backend=chat_completions`,
        ),
        secretAction('Grok', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '写入 Grok config.toml 的官方 OpenAI Chat Completions model 槽；不会启动本机桥接。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: compatibilityEvidence,
      ruleId: kimi ? KIMI_GROK_RULE_ID : OPENAI_GROK_RULE_ID,
      gateKind: 'none',
    };
  }
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
  if (source === 'kimi_membership' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: 'Kimi Code 会员可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 Kimi For Coding provider。', 'kimi-for-coding'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 对应 provider 槽；凭据只引用已保存的 Connection，不会读取或显示明文 Key。',
        '应用后会切换当前 Pi Connection。',
      ],
      evidence: [evidence('Kimi Code CLI provider configuration', 'https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/providers.html')],
      ruleId: 'kimi-membership-to-pi-v1',
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
  if (
    (source === 'glm_coding_plan' || source === 'deepseek_api')
    && request.targetAgentId === 'codex'
  ) {
    const glm = source === 'glm_coding_plan';
    const baseUrl = glm ? GLM_CODEX_BASE_URL : DEEPSEEK_CODEX_BASE_URL;
    const ruleId = glm ? GLM_CODEX_RULE_ID : DEEPSEEK_CODEX_RULE_ID;
    const model = glm ? 'glm-5.3' : 'deepseek-v4-flash';
    return {
      route: 'native_endpoint',
      support: 'experimental',
      reason: `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 官方 Responses 端点可实验直连 Codex。`,
      actions: [
        action('set_config', 'Codex', `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 官方 Responses Base URL；不会启动本机桥接。`, baseUrl),
        action('set_config', 'Codex', `使用 Codex Responses wire_api 与默认模型 ${model}。`, `wire_api=responses; model=${model}`),
        secretAction('Codex', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将把 Codex 配置为官方 Responses 端点；不会启动本机 loopback Bridge。',
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
        '当前未写入官方 ~/.codex/models.json；使用默认 model 与显式 Provider 配置。',
      ],
      evidence: [evidence(
        glm ? 'GLM Coding Plan Codex Responses integration' : 'DeepSeek API Codex Responses integration',
        glm ? 'https://docs.bigmodel.cn/cn/coding-plan/tool/codex' : 'https://api-docs.deepseek.com/quick_start/agent_integrations/codex/',
      )],
      ruleId,
      gateKind: 'none',
    };
  }
  if (source === 'deepseek_api' && request.targetAgentId === 'dsh') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: 'DeepSeek API Key 可预览为 DeepSeek Harness 的配置同步。',
      actions: [
        action('set_config', 'DeepSeek Harness', '选择 DSH 的官方 DeepSeek provider。', 'deepseek-official'),
        secretAction('DeepSeek Harness', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 DeepSeek Harness 的 home 级 provider 引用与凭据文件；不会把 API Key 写入 cordis.patch.yml。',
        '应用后会把该生成 Provider 设为 DSH 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('DeepSeek Harness LLM / credentials', 'https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/credentials')],
      ruleId: 'deepseek-api-to-dsh-v1',
      gateKind: 'none',
    };
  }
  if (source === 'anthropic_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 Anthropic API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 Anthropic provider。', 'anthropic'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 对应 provider 槽；凭据只引用已保存的 Connection，不会读取或显示明文 Key。',
        '应用后会切换当前 Pi Connection。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'anthropic-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (source === 'openai_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 OpenAI API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 OpenAI provider。', 'openai'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 的 openai 槽与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会把该生成 Provider 设为 Pi 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'openai-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (source === 'xai_api_key' && request.targetAgentId === 'pi') {
    return {
      route: 'config_sync',
      support: 'stable',
      reason: '显式 xAI API Key 可预览为 Pi 的配置同步。',
      actions: [
        action('set_config', 'Pi', '选择 Pi 的 xAI provider。', 'xai'),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        '将写入 Pi models.json 的 xai 槽与凭据引用标记；不会在预览中传输明文 Key。',
        '应用后会把该生成 Provider 设为 Pi 当前连接；请确认无其他进行中的配置写入。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId: 'xai-api-to-pi-v1',
      gateKind: 'none',
    };
  }
  if (
    (source === 'glm_coding_plan' || source === 'deepseek_api')
    && request.targetAgentId === 'pi'
  ) {
    const glm = source === 'glm_coding_plan';
    const slot = glm ? 'glm-coding-plan' : 'deepseek';
    const ruleId = glm ? 'glm-coding-plan-to-pi-v1' : 'deepseek-api-to-pi-v1';
    return {
      route: 'config_sync',
      support: 'experimental',
      reason: `${glm ? 'GLM Coding Plan' : 'DeepSeek API'} 可实验预览为 Pi 的配置同步。`,
      actions: [
        action(
          'set_config',
          'Pi',
          `写入 Pi 的 ${glm ? 'GLM Coding Plan' : 'DeepSeek'} 自定义 provider 槽。`,
          slot,
        ),
        secretAction('Pi', '从已选 Connection 引用 API Key；不会读取或显示它。'),
      ],
      limitations: [
        `将写入 Pi models.json 的 ${slot} 自定义槽（baseUrl、api、models）与凭据引用标记；不会在预览中传输明文 Key。`,
        '生成 Provider 只保存凭据引用；live 写入时才 materialize，回填前会 scrub 明文。',
      ],
      evidence: [evidence('Pi custom provider and model configuration', 'https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md')],
      ruleId,
      gateKind: 'none',
    };
  }
  if (
    (source === 'claude_subscription'
      || source === 'codex_subscription'
      || source === 'codex_subscription_oauth_other'
      || source === 'grok_xai_subscription')
    && request.targetAgentId === 'pi'
  ) {
    const subscription = source === 'claude_subscription'
      ? {
          value: 'anthropic',
          ruleId: 'claude-subscription-to-pi-v1',
          reason: 'Claude 订阅可写入 Pi 的 anthropic 登录槽（原生订阅复用）。',
        }
      : source === 'grok_xai_subscription'
        ? {
            value: 'xai',
            ruleId: 'grok-subscription-to-pi-v1',
            reason: 'Grok / xAI 订阅可写入 Pi 的 xai 登录槽（原生订阅复用）。',
          }
        : {
            value: 'openai-codex',
            ruleId: 'codex-subscription-to-pi-v1',
            reason: 'Codex / ChatGPT 订阅可写入 Pi 的 openai-codex 登录槽（原生订阅复用）。',
          };
    return {
      route: 'config_sync',
      support: 'experimental',
      reason: subscription.reason,
      actions: [
        action('set_config', 'Pi', '选择 Pi 的订阅登录槽。', subscription.value),
        secretAction('Pi', '从已选 Connection 引用授权（OAuth）；不会读取或显示 token。'),
      ],
      limitations: [
        '会把 OAuth access/refresh 写入 Pi auth.json 对应槽；预览、IPC、日志不传输明文 token。',
        '写入后由 Pi 刷新该槽；Hub 不双刷同一 refresh token。原 Agent 与 Pi 同时刷新可能互相打翻。',
        '实验性：应用后会把生成 Provider 设为 Pi 当前连接。',
      ],
      evidence: compatibilityEvidence,
      ruleId: subscription.ruleId,
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
      reason: 'Grok 订阅可通过本机桥接到 Claude Code（Messages → xAI Chat Completions）。',
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
