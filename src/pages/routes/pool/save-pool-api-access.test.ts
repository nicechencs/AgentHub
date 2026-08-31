import { describe, expect, it, vi } from 'vitest';
import type { Provider } from '@/lib/types';
import { poolApiChoices } from './api-access-model';
import { savePoolApiAccess, type SavePoolApiAccessDeps } from './save-pool-api-access';

function provider(id: string, agentId: Provider['agentId']): Provider {
  return {
    id,
    agentId,
    name: id,
    preset: 'custom',
    configText: '',
    configFormat: 'json',
    isCurrent: false,
    official: false,
  };
}

describe('savePoolApiAccess', () => {
  it('saves and attaches one record per selected type with distinct ids', async () => {
    const upserted: Provider[] = [];
    const attached: Array<[string, string, string]> = [];
    const deps: SavePoolApiAccessDeps = {
      getAgentConfigSchema: vi.fn(async () => {
        throw new Error('schema unavailable');
      }),
      validateAgentConfig: vi.fn(async () => ({ ok: true, issues: [] })),
      materializeAgentConfig: vi.fn(async () => ({})),
      applyFormVars: vi.fn((_agentId, text) => text),
      upsertProvider: vi.fn(async (draft) => {
        upserted.push(draft);
        return draft;
      }),
      attachAuthorization: vi.fn(async (sourceKind, sourceId, targetAgentId) => {
        attached.push([sourceKind, sourceId, targetAgentId]);
      }),
    };

    const [messages, , , chat] = poolApiChoices(['claude', 'codex', 'grok']);
    const result = await savePoolApiAccess(
      {
        apiKey: 'sk-test',
        items: [
          { choice: messages!, baseUrl: 'https://api.deepseek.com/anthropic' },
          { choice: chat!, baseUrl: 'https://api.deepseek.com' },
        ],
      },
      deps,
    );

    expect(result).toEqual({ saved: 2, errors: [] });
    expect(upserted.map((item) => [item.agentId, item.name])).toEqual([
      ['claude', 'api.deepseek.com /v1/messages'],
      ['grok', 'api.deepseek.com /v1/chat/completions'],
    ]);
    expect(upserted[0]?.id).toMatch(/-claudeMessages$/);
    expect(upserted[1]?.id).toMatch(/-openaiChatCompletions$/);
    expect(new Set(upserted.map((item) => item.id)).size).toBe(2);
    expect(attached).toEqual([
      ['provider', upserted[0]?.id, 'claude'],
      ['provider', upserted[1]?.id, 'grok'],
    ]);
  });

  it('keeps going when one type fails', async () => {
    const deps: SavePoolApiAccessDeps = {
      getAgentConfigSchema: vi.fn(async () => {
        throw new Error('schema unavailable');
      }),
      validateAgentConfig: vi.fn(async () => ({ ok: true, issues: [] })),
      materializeAgentConfig: vi.fn(async () => ({})),
      applyFormVars: vi.fn((_agentId, text) => text),
      upsertProvider: vi.fn(async (draft) => {
        if (draft.agentId === 'claude') throw new Error('claude failed');
        return provider(draft.id, draft.agentId);
      }),
      attachAuthorization: vi.fn(async () => {}),
    };
    const [messages, responses] = poolApiChoices(['claude', 'codex', 'grok']);
    const result = await savePoolApiAccess(
      {
        apiKey: 'sk-test',
        items: [
          { choice: messages!, baseUrl: 'https://api.example.com' },
          { choice: responses!, baseUrl: 'https://api.example.com' },
        ],
      },
      deps,
    );
    expect(result.saved).toBe(1);
    expect(result.errors).toEqual(['claude failed']);
  });
});
