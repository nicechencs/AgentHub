import { describe, expect, it } from 'vitest';
import {
  canSubmitCreateRoute,
  createRouteProviderDraft,
  DEFAULT_CREATE_ROUTE_MODEL,
  isAlternateRouteRule,
  isCreateRouteUrlValid,
  isOpenRouterUrl,
  resolveCreateRouteTargets,
} from './create-route-flow';

describe('create-route-flow', () => {
  it('classifies OpenRouter URL and never treats sk-or- as ahb_', () => {
    expect(isOpenRouterUrl('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('https://openrouter.ai/api/v1')).toBe(true);
    expect(isCreateRouteUrlValid('openrouter.ai/api/v1')).toBe(false);
    const draft = createRouteProviderDraft({
      name: 'OpenRouter',
      url: 'https://openrouter.ai/api/v1/',
      key: 'test-key',
      targets: ['claude'],
      model: 'stealth/ox-alpha',
    });
    expect(draft.preset).toBe('openrouter');
    expect(draft.official).toBe(false);
    expect(draft.configText).toContain('https://openrouter.ai/api/v1');
    expect(draft.configText).toContain('test-key');
    expect(draft.configText).not.toContain('ahb_');
    expect(draft.configText).not.toMatch(/sk-or-/);
    expect(JSON.parse(draft.configText).model).toBe('stealth/ox-alpha');
  });

  it('binds all three clients when no target is selected', () => {
    expect(resolveCreateRouteTargets([])).toEqual(['claude', 'codex', 'grok']);
  });

  it('rejects a URL that is not http(s)', () => {
    const { canSubmitCreateRoute } = require('./create-route-flow') as typeof import('./create-route-flow');
    expect(canSubmitCreateRoute({
      name: 'x',
      url: 'openrouter.ai/api/v1',
      key: 'k',
      targets: [],
    })).toBe(false);
  });

  it('defaults omitted model to stealth/ox-alpha', () => {
    expect(canSubmitCreateRoute({
      name: 'OpenRouter',
      url: 'https://openrouter.ai/api/v1',
      key: 'test-key',
      model: '',
      targets: ['claude'],
    })).toBe(true);
    const draft = createRouteProviderDraft({
      name: 'OpenRouter',
      url: 'https://openrouter.ai/api/v1',
      key: 'test-key',
      targets: [],
    });
    expect(JSON.parse(draft.configText).model).toBe(DEFAULT_CREATE_ROUTE_MODEL);
    expect(DEFAULT_CREATE_ROUTE_MODEL).toBe('stealth/ox-alpha');
  });

  it('marks openai-compat local-bridge rules as alternate', () => {
    expect(isAlternateRouteRule('openai-api-to-claude-v1')).toBe(true);
    expect(isAlternateRouteRule('openai-api-to-codex-v1')).toBe(true);
    expect(isAlternateRouteRule('openai-api-to-grok-bridge-v1')).toBe(true);
    expect(isAlternateRouteRule('kimi-membership-to-codex-v1')).toBe(false);
  });
});
