import { describe, expect, it } from 'vitest';
import { AGENTS } from '@/config/agents';
import type { AgentStatus, UsageRecord, UsageTrendPoint } from '@/lib/types';
import {
  filterVisibleTrend,
  filterVisibleUsage,
  firstVisibleAgentId,
  hiddenAgentIdSet,
  sortAgentsForManagePage,
  visibleCatalogAgents,
  visibleInstalledIds,
} from './agent-visibility';

function status(
  agentId: AgentStatus['agentId'],
  extra: Partial<AgentStatus> = {},
): AgentStatus {
  return {
    agentId,
    installed: true,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    ...extra,
  };
}

describe('agent-visibility', () => {
  it('keeps manage-page order: visible catalog order, then hidden catalog order', () => {
    const rows = [
      status('claude', { hidden: true }),
      status('codex'),
      status('kimi', { hidden: true }),
      status('grok'),
    ];
    expect(sortAgentsForManagePage(rows).map((row) => row.agentId)).toEqual([
      'codex',
      'grok',
      'claude',
      'kimi',
    ]);
  });

  it('treats empty hidden set as identity for usage and trend', () => {
    const usage: UsageRecord[] = [
      {
        id: '1',
        timestamp: '2026-01-01T00:00:00.000Z',
        agentId: 'claude',
        model: 'opus',
        inputTokens: 1,
        outputTokens: 2,
        cacheReadTokens: 0,
        costUsd: 0.1,
        sessionId: 's',
      },
    ];
    const trend: UsageTrendPoint[] = [{ date: '2026-01-01', claude: 3 }];
    expect(filterVisibleUsage(usage, [])).toEqual(usage);
    expect(filterVisibleTrend(trend, [])).toEqual(trend);
  });

  it('drops hidden agents from usage and trend keys', () => {
    const usage: UsageRecord[] = [
      {
        id: '1',
        timestamp: '2026-01-01T00:00:00.000Z',
        agentId: 'claude',
        model: 'opus',
        inputTokens: 1,
        outputTokens: 2,
        cacheReadTokens: 0,
        costUsd: 0.1,
        sessionId: 's',
      },
      {
        id: '2',
        timestamp: '2026-01-01T00:00:00.000Z',
        agentId: 'codex',
        model: 'gpt',
        inputTokens: 4,
        outputTokens: 5,
        cacheReadTokens: 0,
        costUsd: 0.2,
        sessionId: 's2',
      },
    ];
    const trend: UsageTrendPoint[] = [{ date: '2026-01-01', claude: 3, codex: 9 }];
    expect(filterVisibleUsage(usage, ['claude']).map((row) => row.agentId)).toEqual(['codex']);
    expect(filterVisibleTrend(trend, ['claude'])).toEqual([{ date: '2026-01-01', codex: 9 }]);
  });

  it('excludes hidden installed agents from installed ids', () => {
    expect(
      visibleInstalledIds([
        status('claude', { hidden: true }),
        status('codex'),
        status('kimi', { installed: false }),
      ]),
    ).toEqual(['codex']);
  });

  it('builds a hidden id set and visible catalog subset', () => {
    const hidden = hiddenAgentIdSet([status('claude', { hidden: true }), status('codex')]);
    expect([...hidden]).toEqual(['claude']);
    expect(visibleCatalogAgents(hidden).map((agent) => agent.id)).toEqual(
      AGENTS.filter((agent) => agent.id !== 'claude').map((agent) => agent.id),
    );
  });

  it('does not invent a hidden default when the allowed list is empty', () => {
    expect(firstVisibleAgentId('claude', [])).toBe('claude');
  });

  it('falls back when preferred agent is hidden or missing', () => {
    expect(firstVisibleAgentId('claude', ['codex', 'grok'])).toBe('codex');
    expect(firstVisibleAgentId('codex', ['codex', 'grok'])).toBe('codex');
    expect(firstVisibleAgentId(null, [])).toBe('claude');
  });
});
