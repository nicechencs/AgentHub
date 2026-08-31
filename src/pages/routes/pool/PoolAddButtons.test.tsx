import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { MemoryRouter } from 'react-router-dom';
import { describe, expect, it } from 'vitest';
import type { AgentId } from '@/lib/types';
import { PoolAddButtons, poolAddTargets } from './PoolAddButtons';

const AGENTS = ['claude', 'codex', 'kimi'] as const satisfies readonly AgentId[];

function render(node: ReactElement): string {
  return renderToStaticMarkup(createElement(MemoryRouter, null, node));
}

describe('poolAddTargets', () => {
  it('keeps only oauth-capable agents for the oauth intent', () => {
    const targets = poolAddTargets(AGENTS, ['claude'], 'oauth');
    expect(targets.map((target) => target.agentId)).toEqual(['claude']);
    expect(targets[0].url).toBe('/connections?agent=claude&intent=oauth');
  });

  it('keeps every agent for the api key intent and deep-links add-key', () => {
    const targets = poolAddTargets(AGENTS, [], 'add-key');
    expect(targets.map((target) => target.agentId)).toEqual(['claude', 'codex', 'kimi']);
    expect(targets[0].url).toBe('/connections?agent=claude&mode=providers&intent=add-key');
    expect(targets[2].url).toBe('/connections?agent=kimi&mode=providers&intent=add-key');
  });

  it('keeps the installed agent order for oauth targets', () => {
    const targets = poolAddTargets(['kimi', 'claude'] as readonly AgentId[], ['claude', 'kimi'], 'oauth');
    expect(targets.map((target) => target.agentId)).toEqual(['kimi', 'claude']);
  });

  it('returns no targets when no agent supports oauth', () => {
    expect(poolAddTargets(AGENTS, [], 'oauth')).toEqual([]);
  });
});

describe('PoolAddButtons', () => {
  it('renders the two access buttons with their labels', () => {
    const markup = render(
      createElement(PoolAddButtons, { agents: [...AGENTS], oauthAgents: ['claude'] }),
    );
    expect(markup).toContain('OAuth 接入');
    expect(markup).toContain('API 接入');
  });
});
