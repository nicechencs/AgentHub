import type { MessageKey, TranslateFn } from '@/lib/i18n';

const FIELD_LABELS: Record<string, MessageKey> = {
  baseUrl: 'connections.providerDialog.endpoint',
  apiKey: 'connections.apiKeyDialog.key',
  model: 'connections.providerDialog.model',
  claudeAuthEnv: 'connections.providerDialog.fields.claudeAuthEnv',
  modelOpus: 'connections.providerDialog.fields.modelOpus',
  modelSonnet: 'connections.providerDialog.fields.modelSonnet',
  modelHaiku: 'connections.providerDialog.fields.modelHaiku',
  modelFable: 'connections.providerDialog.fields.modelFable',
  modelSubagent: 'connections.providerDialog.fields.modelSubagent',
  contextWindow: 'connections.providerDialog.fields.contextWindow',
  reasoningEffort: 'connections.providerDialog.fields.reasoningEffort',
  wireApi: 'connections.providerDialog.fields.wireApi',
  providerSlug: 'connections.providerDialog.fields.providerSlug',
  provider: 'connections.providerDialog.fields.provider',
  thinking: 'connections.providerDialog.fields.thinking',
  maxTokens: 'connections.providerDialog.fields.maxTokens',
  apiKeyEnv: 'connections.providerDialog.fields.apiKeyEnv',
};

const FIELD_HINTS: Record<string, MessageKey> = {
  baseUrl: 'connections.providerDialog.fieldHints.baseUrl',
  apiKey: 'connections.providerDialog.fieldHints.apiKey',
  model: 'connections.providerDialog.fieldHints.model',
  claudeAuthEnv: 'connections.providerDialog.fieldHints.claudeAuthEnv',
  contextWindow: 'connections.providerDialog.fieldHints.contextWindow',
  reasoningEffort: 'connections.providerDialog.fieldHints.reasoningEffort',
  wireApi: 'connections.providerDialog.fieldHints.wireApi',
  providerSlug: 'connections.providerDialog.fieldHints.providerSlug',
  provider: 'connections.providerDialog.fieldHints.provider',
  thinking: 'connections.providerDialog.fieldHints.thinking',
  maxTokens: 'connections.providerDialog.fieldHints.maxTokens',
  apiKeyEnv: 'connections.providerDialog.fieldHints.apiKeyEnv',
};

export function configFieldLabel(key: string, fallback: string, t: TranslateFn): string {
  const mapped = FIELD_LABELS[key];
  return mapped ? t(mapped) : fallback;
}

export function configFieldHint(
  key: string,
  extraHint: string | undefined,
  t: TranslateFn,
): string | undefined {
  const extra = extraHint?.trim();
  if (extra) return extra;
  const mapped = FIELD_HINTS[key];
  return mapped ? t(mapped) : undefined;
}

export function configFieldOptionLabel(fieldKey: string, option: string, t: TranslateFn): string {
  if (fieldKey === 'contextWindow') {
    if (option === 'auto') return t('connections.providerDialog.fieldOptions.contextAuto');
    if (option === '200000') return t('connections.providerDialog.fieldOptions.context200k');
    if (option === '1048576') return t('connections.providerDialog.fieldOptions.context1m');
  }
  if (fieldKey === 'claudeAuthEnv') {
    if (option === 'ANTHROPIC_AUTH_TOKEN') return t('connections.apiKeyDialog.envAuthToken');
    if (option === 'ANTHROPIC_API_KEY') return t('connections.apiKeyDialog.envApiKey');
  }
  if (fieldKey === 'thinking') {
    if (option === 'enabled') return t('connections.providerDialog.fieldOptions.thinkingOn');
    if (option === 'disabled') return t('connections.providerDialog.fieldOptions.thinkingOff');
  }
  return option;
}
