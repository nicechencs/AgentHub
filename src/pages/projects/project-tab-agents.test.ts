import { describe, expect, it } from 'vitest';
import {
  resolveInitialProjectAgentId,
  resolveProjectFetchAgentId,
  resolveProjectTabAgents,
} from './project-tab-agents';

const claude = { id: 'claude', name: 'Claude' };
const codex = { id: 'codex', name: 'Codex' };
const kimi = { id: 'kimi', name: 'Kimi' };

describe('resolveProjectTabAgents', () => {
  it('keeps visible installed agents and drops hidden ones', () => {
    expect(
      resolveProjectTabAgents([claude, codex], ['claude']).map((agent) => agent.id),
    ).toEqual(['codex']);
  });

  it('does not fall back to the catalog when nobody is installed', () => {
    expect(resolveProjectTabAgents([], ['claude'])).toEqual([]);
    expect(resolveProjectTabAgents([])).toEqual([]);
  });

  it('does not resurrect a hidden installed agent via an empty hidden set', () => {
    // Caller must pass already-visible installed rows; an empty hidden set
    // must not add catalog agents back.
    expect(resolveProjectTabAgents([codex], []).map((agent) => agent.id)).toEqual([
      'codex',
    ]);
    expect(resolveProjectTabAgents([claude, kimi], ['claude', 'kimi'])).toEqual([]);
  });
});

describe('resolveProjectFetchAgentId', () => {
  it('returns null when the tab list is empty', () => {
    expect(resolveProjectFetchAgentId([], 'claude')).toBeNull();
    expect(resolveProjectFetchAgentId([], '')).toBeNull();
  });

  it('returns null when the selected id is not on the strip', () => {
    expect(resolveProjectFetchAgentId([codex, kimi], 'claude')).toBeNull();
    expect(resolveProjectFetchAgentId([codex], '')).toBeNull();
  });

  it('returns the selected id when it is on the strip', () => {
    expect(resolveProjectFetchAgentId([claude, kimi], 'kimi')).toBe('kimi');
    expect(resolveProjectFetchAgentId([codex], 'codex')).toBe('codex');
  });
});

describe('resolveInitialProjectAgentId', () => {
  it('prefers a url agent that is on the strip', () => {
    expect(resolveInitialProjectAgentId('kimi', [claude, kimi], 'claude')).toBe('kimi');
  });

  it('uses the remembered agent when the url is missing or not on the strip', () => {
    expect(resolveInitialProjectAgentId(null, [claude, kimi], 'kimi')).toBe('kimi');
    expect(resolveInitialProjectAgentId('codex', [claude, kimi], 'kimi')).toBe('kimi');
  });

  it('falls back to the first tab when url and remembered are both unusable', () => {
    expect(resolveInitialProjectAgentId(null, [claude, kimi], 'codex')).toBe('claude');
    expect(resolveInitialProjectAgentId(null, [claude, kimi], null)).toBe('claude');
  });

  it('keeps url or remembered while tabs are still empty', () => {
    expect(resolveInitialProjectAgentId('kimi', [], 'claude')).toBe('kimi');
    expect(resolveInitialProjectAgentId(null, [], 'claude')).toBe('claude');
    expect(resolveInitialProjectAgentId(null, [], null)).toBe('');
  });
});
