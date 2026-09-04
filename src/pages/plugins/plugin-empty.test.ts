import { describe, expect, it } from 'vitest';
import type { PluginAgentStatus } from '@/lib/backend/contracts/plugin-types';
import type { TranslateFn } from '@/lib/i18n';
import { pluginEmptyCopy } from './plugin-empty';

const t = ((key: string, params?: { name?: string | number }) =>
  params?.name != null ? `${key}:${params.name}` : key) as TranslateFn;

const agents: PluginAgentStatus[] = [
  { agent: 'claude', support: 'listed', pluginCount: 0 },
  { agent: 'codex', support: 'planned', errorCode: 'planned', pluginCount: 0 },
  { agent: 'cursor', support: 'unsupported', errorCode: 'unsupported-cursor', pluginCount: 0 },
  { agent: 'dsh', support: 'unsupported', errorCode: 'unsupported-dsh', pluginCount: 0 },
  { agent: 'zcode', support: 'unsupported', errorCode: 'unsupported-zcode', pluginCount: 0 },
  { agent: 'kimi', support: 'unsupported', errorCode: 'unsupported-no-cli', pluginCount: 0 },
  { agent: 'grok', support: 'listed', errorCode: 'cli-failed', pluginCount: 0 },
];

describe('pluginEmptyCopy', () => {
  it('tells the all-tab empty state to install in Claude or Grok, then refresh', () => {
    const copy = pluginEmptyCopy('all', agents, '', t);
    expect(copy).toEqual({
      title: 'plugins.empty.title',
      description: 'plugins.empty.all',
      showRefresh: true,
    });
  });

  it('keeps a refresh action when a wired tool simply has no packs', () => {
    const copy = pluginEmptyCopy('claude', agents, 'Claude', t);
    expect(copy).toEqual({
      title: 'plugins.empty.title',
      description: 'plugins.empty.agent:Claude',
      showRefresh: true,
    });
  });

  it('explains planned tools instead of pretending packs are missing', () => {
    const copy = pluginEmptyCopy('codex', agents, 'Codex', t);
    expect(copy.title).toBe('plugins.empty.plannedTitle');
    expect(copy.description).toBe('plugins.support.planned');
    expect(copy.showRefresh).toBe(false);
  });

  it('names why unsupported tools have no pack system', () => {
    expect(pluginEmptyCopy('cursor', agents, 'Cursor', t).description).toBe(
      'plugins.support.unsupportedCursor',
    );
    expect(pluginEmptyCopy('dsh', agents, 'DSH', t).description).toBe(
      'plugins.support.unsupportedDsh',
    );
    expect(pluginEmptyCopy('zcode', agents, 'ZCode', t).description).toBe(
      'plugins.support.unsupportedZcode',
    );
    expect(pluginEmptyCopy('kimi', agents, 'Kimi', t).description).toBe(
      'plugins.support.unsupportedNoCli',
    );
    expect(pluginEmptyCopy('cursor', agents, 'Cursor', t).showRefresh).toBe(false);
  });

  it('keeps refresh when the official command failed to list packs', () => {
    const copy = pluginEmptyCopy('grok', agents, 'Grok', t);
    expect(copy.description).toBe('plugins.support.cliFailed');
    expect(copy.showRefresh).toBe(true);
  });
});
