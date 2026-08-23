import { describe, expect, it, vi } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import type { BindTicketResult } from '@/lib/backend/contracts/ticket';
import type { Provider } from '@/lib/types';
import {
  canSubmitCreateRoute,
  createRouteProviderDraft,
  DEFAULT_CREATE_ROUTE_MODEL,
  isAlternateRouteRule,
  isCreateRouteUrlValid,
  isOpenRouterUrl,
  resolveCreateRouteTargets,
  submitCreateRoute,
} from './create-route-flow';

function plan(targetAgentId: AdapterApplyPlan['targetAgentId']): AdapterApplyPlan {
  return {
    analysis: {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'test',
      actions: [],
      limitations: [],
      evidence: [],
    },
    targetAgentId,
    canApply: true,
    serviceImpact: 'requires_local_bridge',
    changes: [],
  };
}

function bindResult(agentId: BindTicketResult['binding']['agentId']): BindTicketResult {
  return {
    binding: {
      ticketId: 'provider:test',
      agentId,
      route: 'bridge',
      active: true,
      profileId: 'profile-1',
      bridge: null,
    },
  };
}

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

  it('upserts then plans and binds Claude, Codex, and Grok', async () => {
    const upsertProvider = vi.fn(async (provider: Provider) => provider);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent as AdapterApplyPlan['targetAgentId']),
    );
    const bindTicket = vi.fn(async (_ticket: string, agent: string) =>
      bindResult(agent as BindTicketResult['binding']['agentId']),
    );

    const bound = await submitCreateRoute(
      {
        name: 'OpenRouter',
        url: 'https://openrouter.ai/api/v1',
        key: 'test-key',
        targets: [],
      },
      { upsertProvider, planTicket, bindTicket },
    );

    expect(upsertProvider).toHaveBeenCalledOnce();
    const saved = upsertProvider.mock.calls[0]?.[0];
    expect(saved?.preset).toBe('openrouter');
    expect(saved?.official).toBe(false);
    expect(saved?.configText).toContain('test-key');
    expect(saved?.configText).not.toMatch(/sk-or-/);
    expect(JSON.parse(saved?.configText ?? '{}').model).toBe(DEFAULT_CREATE_ROUTE_MODEL);
    expect(planTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
    expect(bindTicket.mock.calls.map((call) => call[1])).toEqual(['claude', 'codex', 'grok']);
    expect(bound).toEqual(['claude', 'codex', 'grok']);
    for (let i = 0; i < 3; i += 1) {
      const planOrder = planTicket.mock.invocationCallOrder[i];
      const bindOrder = bindTicket.mock.invocationCallOrder[i];
      expect(planOrder).toBeDefined();
      expect(bindOrder).toBeDefined();
      expect(planOrder!).toBeLessThan(bindOrder!);
    }
  });
});
