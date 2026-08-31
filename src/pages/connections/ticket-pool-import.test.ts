import { describe, expect, it } from 'vitest';
import type { DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import {
  importedSourceKeys,
  POOL_IMPORTABLE_AGENTS,
  resolveTicketPoolImportAction,
  ticketPoolImportKey,
} from './ticket-pool-import';

describe('ticketPoolImportKey', () => {
  it('joins source kind and id the same way pool membership does', () => {
    expect(ticketPoolImportKey({ sourceKind: 'account', sourceId: 'acc-1' })).toBe('account:acc-1');
    expect(ticketPoolImportKey({ sourceKind: 'provider', sourceId: 'p-1' })).toBe('provider:p-1');
  });
});

describe('importedSourceKeys', () => {
  it('collects membership across default pools', () => {
    const pools: DefaultRoutePoolOverview[] = [
      {
        id: 'pool-codex',
        targetAgentId: 'codex',
        surface: 'responses',
        dialect: 'codex',
        v2Enrolled: false,
        members: [{ sourceKind: 'account', sourceId: 'acc-1', enabled: true }],
      },
      {
        id: 'pool-claude',
        targetAgentId: 'claude',
        surface: 'messages',
        dialect: 'claude',
        v2Enrolled: false,
        members: [{ sourceKind: 'provider', sourceId: 'p-1', enabled: true }],
      },
    ];
    expect([...importedSourceKeys(pools)].sort()).toEqual(['account:acc-1', 'provider:p-1']);
  });
});

describe('resolveTicketPoolImportAction', () => {
  it('disables when the connection pool is off', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi' },
      { poolEnabled: false, alreadyImported: false },
    )).toEqual({ disabled: true, reason: '连接池还没开启' });
  });

  it('disables agents that cannot join a default pool', () => {
    expect(POOL_IMPORTABLE_AGENTS.has('pi')).toBe(false);
    expect(resolveTicketPoolImportAction(
      { agentId: 'pi' },
      { poolEnabled: true, alreadyImported: false },
    )).toEqual({ disabled: true, reason: '这份登录目前不能分享至连接池' });
  });

  it('disables a login already in the pool', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi' },
      { poolEnabled: true, alreadyImported: true },
    )).toEqual({ disabled: true, reason: '已在连接池' });
  });

  it('enables an eligible login that is not in the pool', () => {
    expect(resolveTicketPoolImportAction(
      { agentId: 'kimi' },
      { poolEnabled: true, alreadyImported: false },
    )).toEqual({ disabled: false });
  });
});
