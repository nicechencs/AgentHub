import { describe, expect, it } from 'vitest';
import {
  sub2apiImportDraft,
  sub2apiImportGate,
  sub2apiImportKind,
} from './sub2api-import-model';

describe('sub2api import model', () => {
  it('maps group platform onto the same endpoint kinds as entry API import', () => {
    expect(sub2apiImportKind('anthropic')).toBe('messages');
    expect(sub2apiImportKind('grok')).toBe('responses_grok');
    expect(sub2apiImportKind('openai')).toBe('chat_completions');
    expect(sub2apiImportKind('composite')).toBe('any');
    expect(sub2apiImportKind(null)).toBe('any');
  });

  it('builds a gateway draft instead of a local loopback URL', () => {
    expect(
      sub2apiImportDraft(
        'https://v2.pincc.ai/',
        { id: 1, key: 'sk-test', name: 'n', status: 'active', models: ['grok-4'] },
        'grok',
        'responses_grok',
      ),
    ).toEqual({
      baseUrl: 'https://v2.pincc.ai',
      apiKey: 'sk-test',
      model: 'grok-4',
      apiBackend: 'responses',
    });
  });

  it('opens the Agent menu when at least one Agent is installed', () => {
    const gate = sub2apiImportGate(
      { key: 'sk-test' },
      'messages',
      [{ id: 'claude', name: 'Claude' }, { id: 'codex', name: 'Codex' }],
    );
    expect(gate.enabled).toBe(true);
    expect(gate.agents.find((row) => row.id === 'claude')?.enabled).toBe(true);
    expect(gate.agents.find((row) => row.id === 'codex')?.enabled).toBe(false);
  });
});
