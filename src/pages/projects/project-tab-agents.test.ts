import { describe, expect, it } from 'vitest';
import { resolveProjectTabAgents } from './project-tab-agents';

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
