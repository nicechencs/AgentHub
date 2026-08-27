import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  configFieldHint,
  configFieldLabel,
  configFieldOptionLabel,
  configFieldSecretPlaceholder,
  configFieldSuggestionCustomLabel,
  configFieldSuggestionPickLabel,
  configFieldUnsupported,
} from './config-field-copy';

const t = createTranslator('zh');

describe('config-field-copy', () => {
  it('uses everyday labels instead of schema jargon', () => {
    expect(configFieldLabel('baseUrl', 'Base URL', t)).toBe('服务地址');
    expect(configFieldLabel('claudeAuthEnv', 'Auth env name', t)).toBe('密钥写入方式');
    expect(configFieldLabel('wireApi', 'Wire API', t)).toBe('接口格式');
    expect(configFieldLabel('unknown', 'Other', t)).toBe('Other');
  });

  it('prefers the extra hint and otherwise uses beginner field copy', () => {
    expect(configFieldHint('apiKey', '留空则保持不变。', t)).toBe('留空则保持不变。');
    expect(configFieldHint('baseUrl', undefined, t)).toMatch(/服务网址|官方/);
    expect(configFieldHint('baseUrl', undefined, t)).not.toMatch(/ANTHROPIC_|slug|env_key/);
    expect(configFieldHint('modelOpus', undefined, t)).toBeUndefined();
  });

  it('renames opaque enum values', () => {
    expect(configFieldOptionLabel('contextWindow', 'auto', t)).toBe('自动');
    expect(configFieldOptionLabel('contextWindow', '1048576', t)).toBe('100 万');
    expect(configFieldOptionLabel('claudeAuthEnv', 'ANTHROPIC_AUTH_TOKEN', t)).toMatch(/默认/);
    expect(configFieldOptionLabel('thinking', 'enabled', t)).toBe('开启');
    expect(configFieldOptionLabel('model', 'opus', t)).toBe('opus');
  });

  it('owns unsupported-field copy', () => {
    expect(configFieldUnsupported('自定义', t)).toBe('这个字段暂时不能编辑（自定义）');
  });

  it('owns secret placeholders for configured vs new keys', () => {
    expect(configFieldSecretPlaceholder(true, t)).toBe('已保存，留空表示不改');
    expect(configFieldSecretPlaceholder(false, t)).toBe('API Key');
  });

  it('owns model picker copy', () => {
    expect(configFieldSuggestionPickLabel(t)).toBe('从该地址的模型列表中选择');
    expect(configFieldSuggestionCustomLabel(t)).toBe('自己填写（可留空）');
  });
});
