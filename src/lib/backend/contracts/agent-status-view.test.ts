import { describe, expect, it } from 'vitest';
import { sliceAgentStatus } from './agent-status-view';
import type { AgentStatus } from '@/lib/types';

function row(partial: Partial<AgentStatus> = {}): Partial<AgentStatus> {
  return {
    agentId: 'claude',
    installed: true,
    authStatus: 'none',
    authLabel: '未检测登录态',
    running: false,
    ...partial,
  };
}

describe('sliceAgentStatus', () => {
  it('treats omitted optionals as unknown/unset, ignoring doctor placeholders', () => {
    const view = sliceAgentStatus(row());
    expect(view.hidden).toBe('unknown');
    expect(view.liveAuth.health).toBe('unset');
    expect(view.liveAuth.label).toBe('unset');
    expect(view.liveAuth.source).toBe('unset');
    expect(view.liveAuth.revision).toBe('unset');
    expect(view.effectiveConnection.kind).toBe('unset');
    expect(view.effectiveConnection.label).toBe('unset');
    expect(view.env.ready).toBe('unknown');
    expect(view.env.missing).toBe('unset');
    expect(view.capabilities).toBe('unknown');
  });

  it('projects explicit hidden and authHealth without inferring 未登录 from authStatus', () => {
    expect(sliceAgentStatus(row({ hidden: false })).hidden).toBe('visible');
    expect(sliceAgentStatus(row({ hidden: true })).hidden).toBe('hidden');
    expect(
      sliceAgentStatus(row({ authHealth: 'missing', authStatus: 'none' })).liveAuth.health,
    ).toBe('missing');
    expect(sliceAgentStatus(row({ authHealth: 'verified' })).liveAuth.health).toBe('verified');
  });

  it('keeps written none/empty as facts, not omissions', () => {
    expect(sliceAgentStatus(row({ effectiveKind: 'none' })).effectiveConnection.kind).toBe('none');
    expect(
      sliceAgentStatus(row({ effectiveLabel: 'me@example.com' })).effectiveConnection.label,
    ).toBe('me@example.com');
    expect(sliceAgentStatus(row({ capabilities: {} })).capabilities).toEqual({});
    expect(sliceAgentStatus(row({ envReady: true })).env.ready).toBe(true);
    expect(sliceAgentStatus(row({ envReady: false })).env.ready).toBe(false);
  });
});
