/**
 * 各 Agent「官方 API」默认 endpoint / 模型。
 * 用于 API Key 设置里勾选「官方」时一键带出，不再走中转占位模板。
 */
import type { AgentId } from '@/lib/types';

export type OfficialApiDefaults = {
  /** 展示名 */
  label: string;
  /**
   * 写入配置的 Base URL。
   * Claude / Codex 官方通常不写自定义 base_url（留空 = CLI 内置）。
   */
  baseUrl: string;
  /**
   * UI 展示用官方 URL（即便 baseUrl 为空也要显示给人看）。
   */
  displayBaseUrl: string;
  /** 主模型 */
  model: string;
  /** Claude 分档（可选） */
  modelOpus?: string;
  modelSonnet?: string;
  modelHaiku?: string;
  modelFable?: string;
  modelSubagent?: string;
  /** 写入 provider 池的预设 id */
  presetId: string;
  format: 'json' | 'toml';
  /** 官方模式配置骨架（不含密钥明文） */
  scaffoldText: string;
};

const OFFICIAL: Partial<Record<AgentId, OfficialApiDefaults>> = {
  claude: {
    label: 'Anthropic 官方',
    baseUrl: '', // 不写 ANTHROPIC_BASE_URL，走 CLI 默认
    displayBaseUrl: 'https://api.anthropic.com',
    model: 'sonnet',
    modelOpus: 'opus',
    modelSonnet: 'sonnet',
    modelHaiku: 'haiku',
    modelFable: 'sonnet',
    modelSubagent: 'haiku',
    presetId: 'anthropic',
    format: 'json',
    scaffoldText: JSON.stringify(
      {
        env: {},
        model: 'sonnet',
      },
      null,
      2,
    ),
  },
  codex: {
    label: 'OpenAI 官方',
    baseUrl: '', // 无 custom model_provider 即官方
    displayBaseUrl: 'https://api.openai.com/v1',
    model: 'gpt-5.1-codex',
    presetId: 'openai',
    format: 'toml',
    scaffoldText: ['model = "gpt-5.1-codex"', ''].join('\n'),
  },
  kimi: {
    label: 'Moonshot 官方',
    baseUrl: 'https://api.moonshot.cn/v1',
    displayBaseUrl: 'https://api.moonshot.cn/v1',
    model: 'kimi-k2',
    presetId: 'moonshot',
    format: 'toml',
    scaffoldText: [
      'default_model = "kimi-k2"',
      'default_provider = "moonshot"',
      '',
      '[providers.moonshot]',
      'type = "openai"',
      'base_url = "https://api.moonshot.cn/v1"',
      'api_key = ""',
      '',
      '[models."kimi-k2"]',
      'provider = "moonshot"',
      'model = "kimi-k2"',
      'max_context_size = 131072',
      '',
    ].join('\n'),
  },
  grok: {
    label: 'xAI 官方',
    baseUrl: 'https://api.x.ai/v1',
    displayBaseUrl: 'https://api.x.ai/v1',
    model: 'grok-code-fast-1',
    presetId: 'xai',
    format: 'toml',
    scaffoldText: [
      'model = "grok-code-fast-1"',
      'base_url = "https://api.x.ai/v1"',
      'api_key = ""',
      '',
    ].join('\n'),
  },
};

export function officialApiDefaults(agentId: AgentId): OfficialApiDefaults | null {
  return OFFICIAL[agentId] ?? null;
}

/** Agents that have a real official URL/model template (Pi does not). */
export function agentHasOfficialApiTemplate(agentId: AgentId): boolean {
  return officialApiDefaults(agentId) != null;
}

/** 是否像官方 endpoint（用于旧数据推断） */
export function looksLikeOfficialEndpoint(
  agentId: AgentId,
  baseUrl: string | undefined,
): boolean {
  const off = officialApiDefaults(agentId);
  if (!off) return false;
  const u = (baseUrl ?? '').trim().replace(/\/$/, '');
  if (!off.baseUrl) {
    // Claude/Codex 官方常不写 base_url
    return !u || /api\.anthropic\.com|api\.openai\.com/i.test(u);
  }
  const official = off.baseUrl.replace(/\/$/, '');
  return u === official || u.startsWith(official);
}
