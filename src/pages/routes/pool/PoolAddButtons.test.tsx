import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { AgentId } from '@/lib/types';
import { PoolAddButtons, poolApiChoices, poolOAuthChoices } from './PoolAddButtons';

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
  it('maps the three API choices to their Agent and endpoint', () => {
    const choices = poolApiChoices(AGENTS);
    expect(choices.map(({ agentId, endpoint }) => [agentId, endpoint])).toEqual([
      ['claude', '/v1/messages'],
      ['codex', '/v1/responses'],
      ['grok', '/v1/responses'],
    ]);
  });

  it('keeps unavailable API choices discoverable', () => {
    const choices = poolApiChoices(['claude']);
    expect(choices.map((choice) => choice.available)).toEqual([true, false, false]);
  });
});

describe('PoolAddButtons', () => {
  it('renders the two access buttons with their labels', () => {
    const markup = render(
      createElement(PoolAddButtons, { agents: [...AGENTS], oauthAgents: ['claude'] }),
    );
    expect(markup).toContain('OAuth 接入');
    expect(markup).toContain('API 接入');
    expect(markup).not.toContain('ChevronDown');
    expect(markup).not.toContain('data-radix-menu');
  });
});
