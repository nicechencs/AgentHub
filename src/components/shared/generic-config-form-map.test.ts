import { describe, expect, it } from 'vitest';
import type { AgentConfigSchemaDto } from '@/lib/backend/contracts/config-types';
import { SECRET_REDACTED } from '@/lib/backend/contracts/config-types';
import {
  emptyValuesFromSchema,
  fieldControlKind,
  isAdvancedProviderFormKey,
  isSecretUnchanged,
  mergeDocumentValues,
  issuesByField,
} from './generic-config-form-map';

const schema: AgentConfigSchemaDto = {
  agentKey: 'claude',
  schemaVersion: 1,
  nativeFormat: 'json',
  relativePath: 'settings.json',
  fields: [
    { key: 'baseUrl', label: 'Base URL', valueType: { kind: 'string' } },
    { key: 'apiKey', label: 'Key', valueType: { kind: 'secret' }, secret: true },
    {
      key: 'claudeAuthEnv',
      label: 'Auth',
      valueType: {
        kind: 'enum',
        options: ['ANTHROPIC_AUTH_TOKEN', 'ANTHROPIC_API_KEY'],
      },
    },
    { key: 'enabled', label: 'On', valueType: { kind: 'boolean' } },
  ],
};

describe('generic-config-form-map', () => {
  it('emptyValuesFromSchema fills defaults', () => {
    const v = emptyValuesFromSchema(schema);
    expect(v.baseUrl).toBe('');
    expect(v.apiKey).toBe('');
    expect(v.enabled).toBe(false);
  });

  it('mergeDocumentValues keeps schema keys only for known fields', () => {
    const v = mergeDocumentValues(schema, {
      baseUrl: 'https://x',
      apiKey: SECRET_REDACTED,
      extra: 'nope',
    });
    expect(v.baseUrl).toBe('https://x');
    expect(v.apiKey).toBe(SECRET_REDACTED);
    expect(v).not.toHaveProperty('extra');
  });

  it('isSecretUnchanged treats empty and redacted', () => {
    expect(isSecretUnchanged('')).toBe(true);
    expect(isSecretUnchanged(SECRET_REDACTED)).toBe(true);
    expect(isSecretUnchanged('sk-x')).toBe(false);
  });

  it('fieldControlKind maps secret and unsupported', () => {
    expect(fieldControlKind(schema.fields[0]!)).toBe('string');
    expect(fieldControlKind(schema.fields[1]!)).toBe('secret');
    expect(
      fieldControlKind({
        key: 'x',
        label: 'X',
        valueType: { kind: 'object' } as never,
      }),
    ).toBe('unsupported');
  });

  it('hides env names and per-role models from the API Key form', () => {
    expect(isAdvancedProviderFormKey('claudeAuthEnv')).toBe(true);
    expect(isAdvancedProviderFormKey('modelOpus')).toBe(true);
    expect(isAdvancedProviderFormKey('wireApi')).toBe(true);
    expect(isAdvancedProviderFormKey('baseUrl')).toBe(false);
    expect(isAdvancedProviderFormKey('apiKey')).toBe(false);
    expect(isAdvancedProviderFormKey('model')).toBe(false);
  });

  it('issuesByField indexes first message', () => {
    const m = issuesByField([
      { fieldKey: 'a', code: 'x', message: 'one' },
      { fieldKey: 'a', code: 'y', message: 'two' },
      { fieldKey: 'b', code: 'z', message: 'bee' },
    ]);
    expect(m.a).toBe('one');
    expect(m.b).toBe('bee');
  });
});
