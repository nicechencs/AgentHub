import { describe, expect, it, vi } from 'vitest';
import type { Provider } from '@/lib/types';
import {
  canSubmitEditRoute,
  editRouteFormFromProvider,
  editRouteProviderDraft,
  isEditableRouteSource,
  submitEditRoute,
  type EditRouteInput,
} from './create-route-flow';

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: 'openai-compat-1',
    agentId: 'codex',
    name: 'OpenRouter',
    preset: 'openrouter',
    configFormat: 'json',
    configText: JSON.stringify({
      baseURL: 'https://openrouter.ai/api/v1',
      baseUrl: 'https://openrouter.ai/api/v1',
      apiKey: 'stored-key',
      api_key: 'stored-key',
      vendor: 'openrouter',
      endpoints: [
        { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
        { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
        { target: 'grok', enabled: false, url: 'https://openrouter.ai/api/v1' },
      ],
      listedModels: ['stealth/ox-alpha'],
      model: 'stealth/ox-alpha',
    }),
    isCurrent: false,
    official: false,
    ...overrides,
  };
}

function editInput(overrides: Partial<EditRouteInput> = {}): EditRouteInput {
  return {
    name: 'OpenRouter',
    url: 'https://openrouter.ai/api/v1',
    key: '',
    endpoints: ['claude', 'codex'],
    models: 'stealth/ox-alpha',
    ...overrides,
  };
}

describe('isEditableRouteSource', () => {
  it('accepts a JSON provider source', () => {
    expect(isEditableRouteSource({ sourceKind: 'provider', provider: provider() })).toBe(true);
    expect(isEditableRouteSource({
      sourceKind: 'provider',
      provider: { configText: '{}', configFormat: 'json' },
    })).toBe(true);
  });

  it('rejects accounts, TOML providers, unparseable text, and a missing provider', () => {
    expect(isEditableRouteSource({ sourceKind: 'account', provider: provider() })).toBe(false);
    expect(isEditableRouteSource({
      sourceKind: 'provider',
      provider: { configText: 'base_url = "https://api.openai.com/v1"', configFormat: 'toml' },
    })).toBe(false);
    expect(isEditableRouteSource({
      sourceKind: 'provider',
      provider: { configText: 'not json at all', configFormat: 'json' },
    })).toBe(false);
    expect(isEditableRouteSource({ sourceKind: 'provider', provider: null })).toBe(false);
    expect(isEditableRouteSource({ sourceKind: 'provider' })).toBe(false);
  });
});

describe('editRouteFormFromProvider', () => {
  it('never seeds a key and returns enabled endpoints plus formatted models', () => {
    const form = editRouteFormFromProvider(provider());
    expect(form.key).toBe('');
    expect(form.name).toBe('OpenRouter');
    expect(form.url).toBe('https://openrouter.ai/api/v1');
    expect(form.endpoints).toEqual(['claude', 'codex']);
    expect(form.models).toBe('stealth/ox-alpha');
    expect(form.contextWindow).toBe('auto');
    expect(JSON.stringify(form)).not.toContain('stored-key');
  });

  it('round-trips a stored window and does not infer one from the model id', () => {
    const withWindow = editRouteFormFromProvider(provider({
      configText: JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        vendor: 'openrouter',
        listedModels: ['stealth/ox-alpha'],
        model: 'stealth/ox-alpha',
        contextWindowTokens: 1_048_576,
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      }),
    }));
    expect(withWindow.contextWindow).toBe('1048576');
    expect(withWindow.models).toBe('stealth/ox-alpha');

    const kept = JSON.parse(editRouteProviderDraft(
      provider({
        configText: JSON.stringify({
          baseURL: 'https://openrouter.ai/api/v1',
          listedModels: ['stealth/ox-alpha'],
          contextWindowTokens: 1_048_576,
        }),
      }),
      editInput(),
    ).configText);
    expect(kept.contextWindowTokens).toBe(1_048_576);

    const written = JSON.parse(editRouteProviderDraft(
      provider(),
      editInput({ contextWindow: '1048576' }),
    ).configText);
    expect(written.contextWindowTokens).toBe(1_048_576);

    const cleared = JSON.parse(editRouteProviderDraft(
      provider({
        configText: JSON.stringify({
          baseURL: 'https://openrouter.ai/api/v1',
          contextWindowTokens: 1_048_576,
        }),
      }),
      editInput({ contextWindow: 'auto' }),
    ).configText);
    expect('contextWindowTokens' in cleared).toBe(false);
  });

  it('reads the base URL from baseURL, baseUrl, or base_url', () => {
    const read = (config: Record<string, unknown>) =>
      editRouteFormFromProvider({ name: 'x', configText: JSON.stringify(config) }).url;
    expect(read({ baseURL: ' https://a.example/v1 ' })).toBe('https://a.example/v1');
    expect(read({ baseUrl: 'https://b.example/v1' })).toBe('https://b.example/v1');
    expect(read({ base_url: 'https://c.example/v1' })).toBe('https://c.example/v1');
    expect(read({})).toBe('');
    expect(editRouteFormFromProvider({ name: 'x', configText: 'nope' }).url).toBe('');
  });

  it('falls back to the local route surfaces when the config declares no endpoints', () => {
    const custom = editRouteFormFromProvider({
      name: 'Custom',
      configText: JSON.stringify({ baseURL: 'https://api.example.com/v1' }),
    });
    expect(custom.endpoints).toEqual(['codex']);

    const openrouter = editRouteFormFromProvider({
      name: 'OpenRouter',
      configText: JSON.stringify({ vendor: 'openrouter', baseURL: 'https://openrouter.ai/api/v1' }),
    });
    expect(openrouter.endpoints).toEqual(['claude', 'codex', 'grok']);
  });
});

describe('canSubmitEditRoute', () => {
  it('allows a blank key but requires name, http(s) url, and one endpoint', () => {
    expect(canSubmitEditRoute(editInput({ key: '' }))).toBe(true);
    expect(canSubmitEditRoute(editInput({ name: '  ' }))).toBe(false);
    expect(canSubmitEditRoute(editInput({ url: 'openrouter.ai/api/v1' }))).toBe(false);
    expect(canSubmitEditRoute(editInput({ endpoints: [] }))).toBe(false);
  });
});

describe('editRouteProviderDraft', () => {
  it('keeps the stored key when the input key is blank and replaces it when supplied', () => {
    const kept = JSON.parse(editRouteProviderDraft(provider(), editInput({ key: '  ' })).configText);
    expect(kept.apiKey).toBe('stored-key');
    expect(kept.api_key).toBe('stored-key');

    const replaced = JSON.parse(editRouteProviderDraft(provider(), editInput({ key: ' next-key ' })).configText);
    expect(replaced.apiKey).toBe('next-key');
    expect(replaced.api_key).toBe('next-key');
  });

  it('leaves the key absent when neither the config nor the input has one', () => {
    const draft = editRouteProviderDraft(
      provider({ configText: JSON.stringify({ baseURL: 'https://api.example.com/v1' }) }),
      editInput({ key: '' }),
    );
    const parsed = JSON.parse(draft.configText);
    expect('apiKey' in parsed).toBe(false);
    expect('api_key' in parsed).toBe(false);
  });

  it('refreshes a stored snake_case base_url and never invents one', () => {
    const withSnake = editRouteProviderDraft(
      provider({ configText: JSON.stringify({ base_url: 'https://old.example/v1' }) }),
      editInput({ url: 'https://new.example/v1' }),
    );
    expect(JSON.parse(withSnake.configText).base_url).toBe('https://new.example/v1');

    const withoutSnake = editRouteProviderDraft(provider(), editInput());
    expect('base_url' in JSON.parse(withoutSnake.configText)).toBe(false);
  });

  it('preserves identity fields, unknown config fields, and the stored vendor', () => {
    const source = provider({
      id: 'openai-compat-keep',
      agentId: 'claude',
      preset: 'openrouter',
      isCurrent: true,
      official: true,
      configText: JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        apiKey: 'stored-key',
        vendor: 'openrouter',
        someUnknownField: { keep: 'me' },
      }),
    });
    const draft = editRouteProviderDraft(source, editInput({ name: '  Renamed  ' }));
    expect(draft.id).toBe('openai-compat-keep');
    expect(draft.agentId).toBe('claude');
    expect(draft.preset).toBe('openrouter');
    expect(draft.isCurrent).toBe(true);
    expect(draft.official).toBe(true);
    expect(draft.name).toBe('Renamed');
    expect(draft.configFormat).toBe('json');
    const parsed = JSON.parse(draft.configText);
    expect(parsed.someUnknownField).toEqual({ keep: 'me' });
    expect(parsed.vendor).toBe('openrouter');
    expect(draft.configText).toBe(JSON.stringify(parsed, null, 2));
  });

  it('seeds endpointUrls from stored per-client upstream URLs', () => {
    const form = editRouteFormFromProvider(provider({
      configText: JSON.stringify({
        baseURL: 'https://openrouter.ai/api/v1',
        vendor: 'openrouter',
        endpoints: [
          { target: 'claude', enabled: true, url: 'https://open.bigmodel.cn/api/anthropic' },
          { target: 'codex', enabled: true, url: 'https://openrouter.ai/api/v1' },
        ],
      }),
    }));
    expect(form.endpointUrls).toEqual({
      claude: 'https://open.bigmodel.cn/api/anthropic',
      codex: 'https://openrouter.ai/api/v1',
    });
  });

  it('rewrites endpoints to the new selection and drops a stale model', () => {
    const draft = editRouteProviderDraft(
      provider(),
      editInput({ endpoints: ['grok'], models: '', url: 'https://openrouter.ai/api/v1/' }),
    );
    const parsed = JSON.parse(draft.configText);
    expect(parsed.endpoints).toEqual([
      { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
    ]);
    expect(parsed.baseURL).toBe('https://openrouter.ai/api/v1');
    expect(parsed.baseUrl).toBe('https://openrouter.ai/api/v1');
    expect(parsed.listedModels).toEqual([]);
    expect('model' in parsed).toBe(false);
  });

  it('persists per-client upstream URL overrides on edit', () => {
    const draft = editRouteProviderDraft(
      provider(),
      editInput({
        endpoints: ['claude', 'grok'],
        endpointUrls: { claude: 'https://open.bigmodel.cn/api/anthropic' },
      }),
    );
    const parsed = JSON.parse(draft.configText);
    expect(parsed.endpoints).toEqual([
      { target: 'claude', enabled: true, url: 'https://open.bigmodel.cn/api/anthropic' },
      { target: 'grok', enabled: true, url: 'https://openrouter.ai/api/v1' },
    ]);
  });
});

describe('submitEditRoute', () => {
  it('rejects with required on invalid input', async () => {
    const upsertProvider = vi.fn(async (row: Provider) => row);
    await expect(submitEditRoute(provider(), editInput({ name: '' }), { upsertProvider }))
      .rejects.toThrow('required');
    expect(upsertProvider).not.toHaveBeenCalled();
  });

  it('upserts the merged draft exactly once', async () => {
    const upsertProvider = vi.fn(async (row: Provider) => row);
    const source = provider();
    const input = editInput({ key: 'next-key', endpoints: ['claude'] });
    const saved = await submitEditRoute(source, input, { upsertProvider });
    expect(upsertProvider).toHaveBeenCalledOnce();
    expect(upsertProvider.mock.calls[0]?.[0]).toEqual(editRouteProviderDraft(source, input));
    expect(saved).toEqual(editRouteProviderDraft(source, input));
  });
});
