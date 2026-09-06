import { describe, expect, it } from 'vitest';
import type { RuntimeOptions } from '@/lib/api/chat';
import { retainRuntimeCatalog } from './chat-runtime-ops-model';

const options = (
  partial: Partial<RuntimeOptions> & Pick<RuntimeOptions, 'settingsFrozen' | 'models' | 'extensions'>,
): RuntimeOptions => ({
  conversationId: 'c1',
  settings: {},
  modelsFromCodex: false,
  ...partial,
});

describe('retainRuntimeCatalog', () => {
  it('keeps prior lists when a frozen read returns empty for the same conversation', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'gpt-a', efforts: ['low'], defaultEffort: 'low' }],
      extensions: [
        {
          id: '/s/a',
          name: 'a',
          kind: 'skill' as const,
          installed: true,
          enabled: true,
          loaded: false,
          callable: true,
          path: '/s/a',
        },
      ],
    };
    const next = options({
      settingsFrozen: true,
      models: [],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1')).toEqual({
      models: prior.models,
      extensions: prior.extensions,
    });
  });

  it('does not keep prior lists across conversations', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'gpt-a', efforts: [], defaultEffort: null }],
      extensions: [],
    };
    const next = options({
      conversationId: 'c2',
      settingsFrozen: true,
      models: [],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c2')).toEqual({
      models: [],
      extensions: [],
    });
  });

  it('accepts a fresh non-empty catalog even while frozen', () => {
    const prior = {
      conversationId: 'c1',
      models: [{ id: 'old', efforts: [], defaultEffort: null }],
      extensions: [],
    };
    const next = options({
      settingsFrozen: true,
      models: [{ id: 'new', efforts: ['high'], defaultEffort: 'high' }],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1').models[0]?.id).toBe('new');
  });

  it('replaces prior empty memory when idle returns a catalog', () => {
    const prior = { conversationId: 'c1', models: [], extensions: [] };
    const next = options({
      settingsFrozen: false,
      models: [{ id: 'gpt-b', efforts: [], defaultEffort: null }],
      extensions: [],
    });
    expect(retainRuntimeCatalog(prior, next, 'c1').models[0]?.id).toBe('gpt-b');
  });
});
