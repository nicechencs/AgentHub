import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import {
  createRouteProviderDraft,
  liveCreateRouteTargets,
  normalizeCreateRouteUrl,
  OPENROUTER_BACKUP_MODEL,
  submitCreateRoute,
} from './create-route-flow';

const PLACEHOLDER_KEY = 'sk-or-placeholder-test-key';

function plan(canApply: boolean): AdapterApplyPlan {
  return {
    analysis: {
      route: 'local_bridge',
      support: 'experimental',
      reason: 'test',
      actions: [],
      limitations: [],
      evidence: [],
    },
    targetAgentId: 'codex',
    canApply,
    serviceImpact: 'requires_local_bridge',
    changes: [],
  };
}

describe('create-route-flow', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-08-23T00:00:00Z'));
  });

  it('normalizes trailing slashes on the URL', () => {
    expect(normalizeCreateRouteUrl(' https://openrouter.ai/api/v1/ ')).toBe(
      'https://openrouter.ai/api/v1',
    );
  });

  it('binds only installed Codex, Grok, and Pi', () => {
    expect(liveCreateRouteTargets(['codex', 'claude', 'pi'])).toEqual(['codex', 'pi']);
    expect(liveCreateRouteTargets([])).toEqual([]);
  });

  it('drafts an unofficial openai-compatible provider with placeholder key only', () => {
    const draft = createRouteProviderDraft({
      name: 'OpenRouter backup',
      url: 'https://openrouter.ai/api/v1/',
      key: PLACEHOLDER_KEY,
      liveAgents: ['codex'],
    });
    expect(draft.preset).toBe('openai-compatible');
    expect(draft.official).toBe(false);
    expect(draft.agentId).toBe('codex');
    expect(draft.isCurrent).toBe(false);
    const parsed = JSON.parse(draft.configText) as {
      baseURL: string;
      apiKey: string;
      model: string;
    };
    expect(parsed.baseURL).toBe('https://openrouter.ai/api/v1');
    expect(parsed.apiKey).toBe(PLACEHOLDER_KEY);
    expect(parsed.model).toBe(OPENROUTER_BACKUP_MODEL);
    expect(draft.authApiKey).toBe(PLACEHOLDER_KEY);
  });

  it('upserts then plans and binds live agents that can apply', async () => {
    const upsertProvider = vi.fn(async (p: { id: string }) => p);
    const planTicket = vi.fn(async (_ticket: string, agent: string) =>
      plan(agent !== 'pi'),
    );
    const bindTicket = vi.fn(async () => ({
      binding: {
        ticketId: 'provider:p-1',
        agentId: 'codex',
        route: 'bridge' as const,
        active: true,
        profileId: 'profile-1',
        bridge: null,
      },
    }));

    const bound = await submitCreateRoute(
      {
        name: 'Backup',
        url: 'https://openrouter.ai/api/v1',
        key: PLACEHOLDER_KEY,
        liveAgents: ['codex', 'grok', 'pi', 'claude'],
      },
      { upsertProvider, planTicket, bindTicket },
    );

    expect(upsertProvider).toHaveBeenCalledOnce();
    const saved = upsertProvider.mock.calls[0]?.[0] as {
      preset: string;
      official: boolean;
      configText: string;
    };
    expect(saved.preset).toBe('openai-compatible');
    expect(saved.official).toBe(false);
    expect(saved.configText).toContain(PLACEHOLDER_KEY);
    expect(saved.configText).not.toMatch(/sk-or-v1-/);
    expect(planTicket).toHaveBeenCalledTimes(3);
    expect(planTicket.mock.calls.map((call) => call[1])).toEqual(['codex', 'grok', 'pi']);
    expect(bindTicket).toHaveBeenCalledTimes(2);
    expect(bound).toEqual(['codex', 'grok']);
  });

  it('skips agents that are not live and fails when none bind', async () => {
    await expect(
      submitCreateRoute({
        name: 'Backup',
        url: 'https://openrouter.ai/api/v1',
        key: PLACEHOLDER_KEY,
        liveAgents: ['claude'],
      }),
    ).rejects.toThrow('none-live');
  });
});
