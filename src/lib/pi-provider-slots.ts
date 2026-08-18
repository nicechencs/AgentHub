/**
 * Pi documented auth.json API-key slots + AgentHub models.json bind slots.
 * Auth table: https://pi.dev/docs/latest/providers
 * Custom providers: https://pi.dev/docs/latest/models
 */

export const PI_PLACEHOLDER_BASE_URL = 'https://your-relay.example.com/v1';

export type PiProviderSlot = {
  id: string;
  label: string;
  envVar?: string;
  /** Default `models.json` `api` when this slot writes a custom / relay provider. */
  api?: string;
};

/** Official `~/.pi/agent/auth.json` API-key keys from the Pi providers table. */
export const PI_AUTH_JSON_SLOTS: PiProviderSlot[] = [
  { id: 'anthropic', label: 'Anthropic', envVar: 'ANTHROPIC_API_KEY', api: 'anthropic-messages' },
  { id: 'ant-ling', label: 'Ant Ling', envVar: 'ANT_LING_API_KEY', api: 'openai-completions' },
  {
    id: 'azure-openai-responses',
    label: 'Azure OpenAI Responses',
    envVar: 'AZURE_OPENAI_API_KEY',
    api: 'openai-responses',
  },
  { id: 'openai', label: 'OpenAI', envVar: 'OPENAI_API_KEY', api: 'openai-completions' },
  { id: 'deepseek', label: 'DeepSeek', envVar: 'DEEPSEEK_API_KEY', api: 'openai-completions' },
  { id: 'nvidia', label: 'NVIDIA NIM', envVar: 'NVIDIA_API_KEY', api: 'openai-completions' },
  { id: 'google', label: 'Google Gemini', envVar: 'GEMINI_API_KEY', api: 'google-generative-ai' },
  {
    id: 'amazon-bedrock',
    label: 'Amazon Bedrock',
    envVar: 'AWS_BEARER_TOKEN_BEDROCK',
    api: 'openai-completions',
  },
];

const PI_AUTH_JSON_SLOT_IDS = new Set(PI_AUTH_JSON_SLOTS.map((slot) => slot.id));

/** True only for the official auth.json API-key table (not models.json bind slots). */
export function isPiAuthJsonSlot(id: string): boolean {
  return PI_AUTH_JSON_SLOT_IDS.has(id.trim());
}

/** AgentHub models.json bind slots — not auth.json builtins. */
const PI_MODELS_JSON_BIND_SLOTS: PiProviderSlot[] = [
  { id: 'xai', label: 'xAI (models.json)', api: 'openai-completions' },
  { id: 'kimi-for-coding', label: 'Kimi For Coding (models.json)', api: 'openai-completions' },
];

export const PI_PROVIDER_SLOT_OPTIONS: PiProviderSlot[] = [
  ...PI_AUTH_JSON_SLOTS,
  ...PI_MODELS_JSON_BIND_SLOTS,
  { id: 'custom', label: '自定义 (models.json)', api: 'openai-completions' },
];

export function piProviderSlotById(id: string): PiProviderSlot | undefined {
  const key = id.trim();
  return PI_PROVIDER_SLOT_OPTIONS.find((slot) => slot.id === key);
}

export function defaultPiProviderApi(id: string): string {
  return piProviderSlotById(id)?.api ?? 'openai-completions';
}

export function isPiPlaceholderBaseUrl(url: string): boolean {
  return url.trim().replace(/\/$/, '') === PI_PLACEHOLDER_BASE_URL.replace(/\/$/, '');
}

/** Custom / bind slots need a URL; official auth.json slots use Pi builtins unless a relay URL is set. */
export function piFormRequiresBaseUrl(slug: string): boolean {
  return !isPiAuthJsonSlot(slug.trim() || 'custom');
}

export function piProviderSlotHint(id: string): string {
  const slug = id.trim() || 'custom';
  if (isPiAuthJsonSlot(slug)) {
    const env = piProviderSlotById(slug)?.envVar;
    const envBit = env ? `（${env}）` : '';
    return `Key 写入 ~/.pi/agent/auth.json 的 ${slug}${envBit}。不填 URL 时用 Pi 内置端点与模型。`;
  }
  return `写入 ~/.pi/agent/models.json 的 ${slug} 槽。需要 Endpoint URL。`;
}
