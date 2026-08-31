import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentId } from '@/lib/types';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  PoolAddButtons,
  poolApiChoices,
  poolOAuthChoices,
  poolSyncCandidates,
  poolSurfaceForApiChoice,
  poolSurfaceForOAuth,
} from './PoolAddButtons';

const AGENTS = ['claude', 'codex', 'grok'] as const satisfies readonly AgentId[];

function render(node: ReactElement): string {
  return renderToStaticMarkup(node);
}

describe('poolOAuthChoices', () => {
  it('always exposes the three supported OAuth choices', () => {
    const choices = poolOAuthChoices(AGENTS, ['claude', 'grok']);
    expect(choices.map((choice) => choice.agentId)).toEqual(['claude', 'codex', 'grok']);
    expect(choices.map((choice) => choice.available)).toEqual([true, false, true]);
  });

  it('marks an OAuth choice unavailable when the Agent is not installed', () => {
    const choices = poolOAuthChoices(['claude', 'grok'], ['claude', 'grok']);
    expect(choices.map((choice) => choice.available)).toEqual([true, false, true]);
  });
});

describe('poolApiChoices', () => {
  it('maps API choices to their Agent, endpoint, and API format', () => {
    const choices = poolApiChoices(AGENTS);
    expect(choices.map(({ agentId, endpoint, grokApiBackend }) => [agentId, endpoint, grokApiBackend])).toEqual([
      ['claude', '/v1/messages', undefined],
      ['codex', '/v1/responses', undefined],
      ['grok', '/v1/responses', 'responses'],
      ['grok', '/v1/chat/completions', 'chat_completions'],
    ]);
  });

  it('keeps unavailable API choices discoverable', () => {
    const choices = poolApiChoices(['claude']);
    expect(choices.map((choice) => choice.available)).toEqual([true, false, false, false]);
  });
});

describe('poolSurfaceForOAuth', () => {
  it('maps each OAuth Agent to its local entry surface', () => {
    expect(poolSurfaceForOAuth('claude')).toBe('messages');
    expect(poolSurfaceForOAuth('codex')).toBe('responses');
    expect(poolSurfaceForOAuth('grok')).toBe('responses');
  });
});

describe('poolSurfaceForApiChoice', () => {
  it('maps each API endpoint to its local entry surface', () => {
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/messages' })).toBe('messages');
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/responses' })).toBe('responses');
    expect(poolSurfaceForApiChoice({ endpoint: '/v1/chat/completions' })).toBe('chat_completions');
  });
});

describe('poolSyncCandidates', () => {
  it('marks existing source memberships disabled and keeps account/provider identity', () => {
    const entry = (source: 'account' | 'provider', id: string, agentId: AgentId, home?: 'route_pool') => ({
      key: `${source}:${id}`,
      source,
      kind: source === 'account' ? 'oauth' : 'apikey',
      id,
      agentId,
      title: id,
      subtitle: '',
      isCurrent: false,
      authStatus: 'none',
      sortKey: '',
      ...(source === 'account' ? { account: { home } } : { provider: { home } }),
    } as ConnectionEntry);

    const candidates = poolSyncCandidates(
      [
        entry('account', 'account-synced', 'codex'),
        entry('provider', 'provider-new', 'grok'),
        entry('account', 'pool-owned', 'claude', 'route_pool'),
        entry('provider', 'unsupported', 'pi'),
      ],
      [{
        id: 'pool-codex',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        v2Enrolled: false,
        members: [{ sourceKind: 'account', sourceId: 'account-synced', enabled: true }],
      }],
    );

    expect(candidates.map((candidate) => [candidate.sourceKind, candidate.sourceId])).toEqual([
      ['account', 'account-synced'],
      ['provider', 'provider-new'],
    ]);
    expect(candidates[0]?.alreadySynced).toBe(true);
    expect(candidates[1]?.alreadySynced).toBe(false);
  });
});

describe('PoolAddButtons', () => {
  it('renders the two access buttons with their labels', () => {
    const markup = render(
      createElement(PoolAddButtons, { agents: [...AGENTS], oauthAgents: ['claude'] }),
    );
    expect(markup).toContain('OAuth 接入');
    expect(markup).toContain('API 接入');
    expect(markup).toContain('从连接同步');
    expect(markup).not.toContain('ChevronDown');
    expect(markup).not.toContain('data-radix-menu');
  });
});
