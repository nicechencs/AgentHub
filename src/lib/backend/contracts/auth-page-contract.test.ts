import { describe, expect, it } from 'vitest';
import { mergeLiveAuthIntoAgentStatus } from '@/app/runtime/agent-status-store';
import { AGENT_MAP } from '@/config/agents';
import { applyEffectiveConnection } from '@/lib/api/agent-connection';
import { buildAgentCardView } from '@/pages/dashboard/agentOverviewModel';
import { mapCoreAccount, type CoreAccount } from './account-map';
import { authDisplayForAgentStatus } from './auth-state';
import { normalizeAuthState, type AuthState } from './ports';
import type { AgentStatus } from '@/lib/types';

function status(overrides: Partial<AgentStatus> = {}): AgentStatus {
  return {
    agentId: 'claude',
    installed: true,
    version: '1.0.0',
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    ...overrides,
  };
}

function coreAccount(extra: Record<string, unknown>): CoreAccount {
  return {
    id: 'acc-1',
    agentId: 'claude',
    kind: 'oauth',
    label: 'user@example.test',
    status: 'active',
    isCurrent: true,
    createdAt: '2026-08-09 12:00:00.000000',
    updatedAt: '2026-08-09 12:00:00.000000',
    extra,
  };
}

describe('Tauri auth payload to page contract', () => {
  it('keeps real health/source/revision through normalization, status merge, and Dashboard model', () => {
    const tauriPayload: AuthState = {
      agent: 'claude',
      kind: 'oauth',
      summary: 'Claude live credentials verified',
      hasCredentials: true,
      health: 'verified',
      source: '.credentials.json',
      revision: 'rev-42',
    };
    const probe = normalizeAuthState(tauriPayload, 'claude');
    const live = mergeLiveAuthIntoAgentStatus(status(), probe);
    const savedAccount = mapCoreAccount(
      coreAccount({
        health: 'renewable',
        authHealth: 'renewable',
        authSource: 'account-pool',
        liveRevision: 'pool-rev',
      }),
    );
    const effective = applyEffectiveConnection(live, savedAccount, undefined);
    const card = buildAgentCardView(AGENT_MAP.claude, effective);

    expect(probe).toMatchObject({
      agentId: 'claude',
      health: 'verified',
      source: '.credentials.json',
      revision: 'rev-42',
    });
    expect(live).toMatchObject({
      authHealth: 'verified',
      authSource: '.credentials.json',
      authRevision: 'rev-42',
    });
    expect(savedAccount).toMatchObject({
      authHealth: 'renewable',
      liveAuthHealth: 'renewable',
      liveAuthSource: 'account-pool',
      liveAuthRevision: 'pool-rev',
    });
    // A stored account can be renewable while the live probe is verified;
    // effective/page state must retain the latter.
    expect(effective.authHealth).toBe('verified');
    expect(card.authHealth).toBe('verified');
    expect(card.statusDotTitle).toBe('已验证');
  });

  it('falls back to legacy status rows when an old payload has no semantic health', () => {
    const legacy = status({ authStatus: 'expired', authLabel: '登录已过期' });
    expect(authDisplayForAgentStatus(legacy)).toMatchObject({
      health: 'needs_login',
      legacyStatus: 'expired',
    });
  });
});
