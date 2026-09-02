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
        apiKeys: ['sk-test'],
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
    expect(upserted[0]?.id).toMatch(/-claudeMessages-0$/);
    expect(upserted[1]?.id).toMatch(/-openaiChatCompletions-1$/);
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
        apiKeys: ['sk-test'],
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

  it('saves one login per API key and writes models plus priority', async () => {
    const catalogs: Array<[string, string[]]> = [];
    const priorities: Array<[string, number]> = [];
    const deps: SavePoolApiAccessDeps = {
      getAgentConfigSchema: vi.fn(async () => {
        throw new Error('schema unavailable');
      }),
      validateAgentConfig: vi.fn(async () => ({ ok: true, issues: [] })),
      materializeAgentConfig: vi.fn(async () => ({})),
      applyFormVars: vi.fn((_agentId, text) => text),
      upsertProvider: vi.fn(async (draft) => draft),
      attachAuthorization: vi.fn(async () => {}),
      setSourceCustomModels: vi.fn(async (_kind, sourceId, models) => {
        catalogs.push([sourceId, models]);
      }),
      setAuthorizationPriority: vi.fn(async (_kind, sourceId, priority) => {
        priorities.push([sourceId, priority]);
        return 1;
      }),
    };
    const [messages] = poolApiChoices(['claude', 'codex', 'grok']);
    const result = await savePoolApiAccess(
      {
        apiKeys: ['sk-a', 'sk-b'],
        models: ['gpt-4o', 'custom-1'],
        priority: 3,
        items: [{ choice: messages!, baseUrl: 'https://api.example.com' }],
      },
      deps,
    );
    expect(result).toEqual({ saved: 2, errors: [] });
    expect(catalogs).toHaveLength(2);
    expect(catalogs[0]?.[1]).toEqual(['gpt-4o', 'custom-1']);
    expect(priorities.map((item) => item[1])).toEqual([3, 3]);
  });

  it('updates the existing provider on edit and does not attach again', async () => {
    const attached = vi.fn(async () => {});
    const upserted: string[] = [];
    const deps: SavePoolApiAccessDeps = {
      getAgentConfigSchema: vi.fn(async () => {
        throw new Error('schema unavailable');
      }),
      validateAgentConfig: vi.fn(async () => ({ ok: true, issues: [] })),
      materializeAgentConfig: vi.fn(async () => ({})),
      applyFormVars: vi.fn((_agentId, text) => text),
      upsertProvider: vi.fn(async (draft) => {
        upserted.push(draft.id);
        return draft;
      }),
      attachAuthorization: attached,
    };
    const [messages] = poolApiChoices(['claude', 'codex', 'grok']);
    const existing = provider('prov-1', 'claude');
    const result = await savePoolApiAccess(
      {
        apiKeys: [''],
        edit: { provider: existing },
        items: [{ choice: messages!, baseUrl: 'https://api.anthropic.com' }],
      },
      deps,
    );
    expect(result).toEqual({ saved: 1, errors: [] });
    expect(upserted).toEqual(['prov-1']);
    expect(attached).not.toHaveBeenCalled();
  });
});
