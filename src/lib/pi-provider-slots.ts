/**
 * Pi documented auth.json API-key slots + AgentHub models.json bind slots.
 * Auth table: https://pi.dev/docs/latest/providers
 * Custom providers: https://pi.dev/docs/latest/models
 */
import type { TranslateFn } from '@/lib/i18n';

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
  { id: 'xai', label: 'xAI (custom)', api: 'openai-completions' },
  { id: 'kimi-for-coding', label: 'Kimi For Coding (custom)', api: 'openai-completions' },
];

export const PI_PROVIDER_SLOT_OPTIONS: PiProviderSlot[] = [
  ...PI_AUTH_JSON_SLOTS,
  ...PI_MODELS_JSON_BIND_SLOTS,
  { id: 'custom', label: 'Custom service', api: 'openai-completions' },
];

/** i18n keys for slot labels not covered by the official auth.json table (English source data above). */
const PI_SLOT_LABEL_KEYS: Record<string, 'connections.pi.xaiCustomLabel' | 'connections.pi.kimiCustomLabel' | 'connections.pi.customServiceLabel'> = {
  xai: 'connections.pi.xaiCustomLabel',
  'kimi-for-coding': 'connections.pi.kimiCustomLabel',
  custom: 'connections.pi.customServiceLabel',
};

export function piProviderSlotById(id: string): PiProviderSlot | undefined {
  const key = id.trim();
  return PI_PROVIDER_SLOT_OPTIONS.find((slot) => slot.id === key);
}

/** Localized label for a slot id; falls back to the slot's English default label. */
export function piProviderSlotLabel(id: string, t?: TranslateFn): string {
  const slug = id.trim() || 'custom';
  const slot = piProviderSlotById(slug);
  const fallback = slot?.label ?? slug;
  const key = PI_SLOT_LABEL_KEYS[slug];
  if (!t || !key) return fallback;
  return t(key);
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

export function piProviderSlotHint(id: string, t?: TranslateFn): string {
  const slug = id.trim() || 'custom';
  if (isPiAuthJsonSlot(slug)) {
    return t
      ? t('connections.pi.authJsonHint', { slug })
      : `The key is written to Pi's official login file (auth.json / ${slug}). Leave the address blank to use the official service and models.`;
  }
  return t
    ? t('connections.pi.modelsJsonHint', { slug })
    : `This writes to Pi's custom service config (models.json / ${slug}). A service address is required.`;
}
