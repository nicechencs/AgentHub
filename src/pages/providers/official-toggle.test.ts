import { describe, expect, it } from 'vitest';
import { officialApiDefaults } from '@/config/official-api';
import {
  defaultConfigScaffold,
  EMPTY_FORM_VARS,
  REDACTED_MARKER,
} from '@/lib/provider-detect';
import { officialToggleNext, type OfficialToggleForm } from './official-toggle';

const OPENROUTER_JSON = JSON.stringify(
  {
    baseURL: 'https://openrouter.ai/api/v1',
    model: 'stealth/ox-alpha',
  },
  null,
  2,
);

const openRouterCustom: OfficialToggleForm = {
  vars: {
    ...EMPTY_FORM_VARS,
    baseUrl: '',
    model: 'stealth/ox-alpha',
    apiKey: REDACTED_MARKER,
  },
  configText: OPENROUTER_JSON,
  configFormat: 'json',
};

function officialForm(agentId: 'codex' | 'claude'): OfficialToggleForm {
  const off = officialApiDefaults(agentId);
  if (!off) throw new Error(`missing official defaults for ${agentId}`);
  return {
    vars: {
      ...EMPTY_FORM_VARS,
      baseUrl: off.baseUrl,
      model: off.model,
      apiKey: REDACTED_MARKER,
    },
    configText: off.scaffoldText,
    configFormat: off.format,
  };
}

describe('officialToggleNext', () => {
  it('check then uncheck restores OpenRouter JSON (Codex official defaults)', () => {
    const official = officialForm('codex');
    expect(official.vars.model).toBe('gpt-5.1-codex');
    expect(official.configFormat).toBe('toml');

    const afterOn = officialToggleNext({
      checked: true,
      current: openRouterCustom,
      snapshot: null,
      official,
    });
    expect(afterOn.snapshot?.configText).toBe(OPENROUTER_JSON);
    expect(afterOn.snapshot?.vars.model).toBe('stealth/ox-alpha');
    expect(afterOn.vars.model).toBe('gpt-5.1-codex');
    expect(afterOn.configFormat).toBe('toml');

    const afterOff = officialToggleNext({
      checked: false,
      current: afterOn,
      snapshot: afterOn.snapshot,
      official,
    });
    expect(afterOff.vars.model).toBe('stealth/ox-alpha');
    expect(afterOff.vars.model).not.toBe('gpt-5.1-codex');
    expect(afterOff.configText).toBe(OPENROUTER_JSON);
    expect(afterOff.configFormat).toBe('json');
    expect(afterOff.configText).toContain('https://openrouter.ai/api/v1');
    expect(afterOff.configText).not.toContain('your-relay.example.com');
    expect(afterOff.configText).not.toBe(defaultConfigScaffold('codex').text);
  });

  it('check then uncheck restores a generic Claude custom login', () => {
    const custom: OfficialToggleForm = {
      vars: {
        ...EMPTY_FORM_VARS,
        baseUrl: 'https://relay.example/anthropic',
        model: 'opus',
        apiKey: REDACTED_MARKER,
      },
      configText: JSON.stringify(
        {
          env: { ANTHROPIC_BASE_URL: 'https://relay.example/anthropic' },
          model: 'opus',
        },
        null,
        2,
      ),
      configFormat: 'json',
    };
    const official = officialForm('claude');
    expect(official.vars.model).toBe('sonnet');

    const afterOn = officialToggleNext({
      checked: true,
      current: custom,
      snapshot: null,
      official,
    });
    expect(afterOn.vars.model).toBe('sonnet');

    const afterOff = officialToggleNext({
      checked: false,
      current: afterOn,
      snapshot: afterOn.snapshot,
      official,
    });
    expect(afterOff.vars.model).toBe('opus');
    expect(afterOff.vars.baseUrl).toBe('https://relay.example/anthropic');
    expect(afterOff.configText).toBe(custom.configText);
    expect(afterOff.configText).not.toContain('your-relay.example.com');
  });

  it('uncheck with no snapshot keeps current (does not write the placeholder scaffold)', () => {
    const official = officialForm('codex');
    const afterOff = officialToggleNext({
      checked: false,
      current: official,
      snapshot: null,
      official,
    });
    expect(afterOff.vars.model).toBe('gpt-5.1-codex');
    expect(afterOff.configText).toBe(official.configText);
    expect(afterOff.configText).not.toContain('your-relay.example.com');
    expect(afterOff.configText).not.toBe(defaultConfigScaffold('codex').text);
    expect(afterOff.snapshot).toBeNull();
  });
});
