import { describe, expect, it } from 'vitest';
import { applyAgentUpdates } from './agent';
import type { AgentStatus, AgentUpdateInfo } from '@/lib/types';

function base(partial: Partial<AgentStatus> & Pick<AgentStatus, 'agentId'>): AgentStatus {
  return {
    installed: true,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    ...partial,
  };
}

describe('applyAgentUpdates', () => {
  it('merges latestVersion and update probe by agentId', () => {
    const agents = [
      base({ agentId: 'claude', version: '1.0.0' }),
      base({ agentId: 'codex', version: '0.1.0' }),
    ];
    const updates: AgentUpdateInfo[] = [
      {
        agentId: 'claude',
        state: 'update_available',
        currentVersion: '1.0.0',
        latestVersion: '1.2.0',
        source: 'npm',
      },
    ];
    const merged = applyAgentUpdates(agents, updates);
    expect(merged[0]?.latestVersion).toBe('1.2.0');
    expect(merged[0]?.update?.state).toBe('update_available');
    expect(merged[1]?.update).toBeUndefined();
  });
});
