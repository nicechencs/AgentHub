import { describe, expect, it } from 'vitest';
import { hiddenAgentIdSet, omittedAgentIds, visibleInstalledIds } from '@/lib/agent-visibility';
import type { AgentStatus } from '@/lib/types';

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

describe('useInstalledAgents selection', () => {
  it('drops hidden installed agents from installed ids and exposes hiddenIds', () => {
    const statuses = [
      status('claude', { hidden: true }),
      status('codex'),
      status('kimi', { installed: false }),
    ];
    expect(visibleInstalledIds(statuses)).toEqual(['codex']);
    expect([...hiddenAgentIdSet(statuses)]).toEqual(['claude']);
    expect(omittedAgentIds(statuses)).toEqual(['claude', 'kimi']);
  });

  it('keeps a stable empty hidden set when nobody is hidden', () => {
    const statuses = [status('claude'), status('codex')];
    expect(visibleInstalledIds(statuses)).toEqual(['claude', 'codex']);
    expect(hiddenAgentIdSet(statuses).size).toBe(0);
  });
});
