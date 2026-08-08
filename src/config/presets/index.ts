// 各 agent 供应商预设模板(对应 docs/architecture.md §4 config/presets/)
// 字段结构对齐常见 CLI 配置（Claude env + model；Codex model_providers + wire_api）。
import type { AgentId } from '@/lib/types';

export interface ProviderPreset {
  id: string;
  label: string;
  format: 'json' | 'toml';
  template: string;
}

const CLAUDE_COMPAT = JSON.stringify(
  {
    env: {
      ANTHROPIC_BASE_URL: 'https://your-relay.example.com',
      ANTHROPIC_AUTH_TOKEN: 'sk-xxxxxxxx',
    },
    model: 'sonnet',
  },
  null,
  2,
);

const CODEX_COMPAT = [
  'model_provider = "custom"',
  'model = "gpt-5.1-codex"',
  'model_reasoning_effort = "high"',
  'disable_response_storage = true',
  'preferred_auth_method = "apikey"',
  '',
  '[model_providers.custom]',
  'name = "custom"',
  'base_url = "https://your-relay.example.com/v1"',
  'wire_api = "responses"',
  '',
].join('\n');

const KIMI_COMPAT = [
  'default_model = "kimi-k2"',
  '',
  '[providers.custom]',
  'base_url = "https://your-relay.example.com/v1"',
  'api_key = "sk-xxxxxxxx"',
  '',
].join('\n');

const GROK_COMPAT = [
  'model = "grok-code-fast-1"',
  'base_url = "https://your-relay.example.com/v1"',
  'api_key = "sk-xxxxxxxx"',
  '',
].join('\n');

export const PRESETS: Record<AgentId, ProviderPreset[]> = {
  claude: [
    {
      id: 'anthropic',
      label: 'Anthropic 官方',
      format: 'json',
      // 官方登录态一般由账号池管理；此处仅保留空 env 占位
      template: JSON.stringify({ env: {} }, null, 2),
    },
    {
      id: 'anthropic-compatible',
      label: 'Anthropic 兼容',
      format: 'json',
      template: CLAUDE_COMPAT,
    },
  ],
  codex: [
    {
      id: 'openai',
      label: 'OpenAI 官方',
      format: 'toml',
      template: 'model = "gpt-5.1-codex"\n',
    },
    {
      id: 'openai-compatible',
      label: 'OpenAI 兼容',
      format: 'toml',
      template: CODEX_COMPAT,
    },
  ],
  kimi: [
    {
      id: 'moonshot',
      label: 'Moonshot 官方',
      format: 'toml',
      template: 'default_model = "kimi-k2"\n',
    },
    {
      id: 'openai-compatible',
      label: 'OpenAI 兼容',
      format: 'toml',
      template: KIMI_COMPAT,
    },
  ],
  grok: [
    {
      id: 'xai',
      label: 'xAI 官方',
      format: 'toml',
      template: 'model = "grok-code-fast-1"\napi_key = "xai-xxxxxxxx"\n',
    },
    {
      id: 'openai-compatible',
      label: 'OpenAI 兼容',
      format: 'toml',
      template: GROK_COMPAT,
    },
  ],
  // write_config fail-closed：无内置 provider 模板，待 models.json 合并规则锁定后再补
  pi: [],
  // WorkBuddy: write_config fail-closed；无供应商模板
  workbuddy: [],
  // Cursor Agent：半套接入，无 models.json/config.toml 供应商契约
  cursor: [],
};
