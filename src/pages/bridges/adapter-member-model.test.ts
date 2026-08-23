import { describe, expect, it } from 'vitest';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { TicketSurfaceGroupView } from '@/lib/backend/contracts/ticket';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  bridgeMemberRows,
  memberPinTone,
  surfaceGroupForProfile,
} from './adapter-member-model';

function profile(partial: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'bridge-1',
    name: 'Kimi → Codex',
    sourceKind: 'provider',
    sourceId: 'kimi-1',
    targetAgentId: 'codex',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'bridge',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 43121,
    autoStart: true,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
    ...partial,
  };
}

function entry(
  partial: Partial<ConnectionEntry> & Pick<ConnectionEntry, 'id' | 'source' | 'title'>,
): ConnectionEntry {
  return {
    key: `${partial.source}:${partial.id}`,
    kind: partial.source === 'account' ? 'oauth' : 'apikey',
    agentId: 'kimi',
    subtitle: '',
    isCurrent: false,
    authStatus: 'valid',
    sortKey: '',
    ...partial,
  };
}

const twoMemberGroup: TicketSurfaceGroupView = {
  surface: 'kimi-code-membership',
  credentialClass: 'api_key',
  members: [
    {
      ticketId: 'account:kimi-stale',
      sourceKind: 'account',
      sourceId: 'kimi-stale',
      agentId: 'kimi',
      label: 'Kimi 会员（失效号）',
      health: 'needs_login',
    },
    {
      ticketId: 'provider:kimi-1',
      sourceKind: 'provider',
      sourceId: 'kimi-1',
      agentId: 'kimi',
      label: 'Kimi Code 会员',
      health: 'renewable',
    },
  ],
};

describe('bridgeMemberRows', () => {
  it('keeps both members visible and greys the NeedsLogin row with a reason', () => {
    const rows = bridgeMemberRows({
      profile: profile(),
      groups: [twoMemberGroup],
      entries: [
        entry({
          source: 'provider',
          id: 'kimi-1',
          title: 'Kimi Code 会员',
          authHealth: 'configured',
        }),
        entry({
          source: 'account',
          id: 'kimi-stale',
          title: 'Kimi 会员（失效号）',
          authStatus: 'expired',
          authHealth: 'needs_login',
          identityLabel: 'Kimi 会员（失效号）',
        }),
      ],
    });

    expect(rows).toHaveLength(2);
    expect(rows.map((row) => row.label)).toEqual([
      'Kimi 会员（失效号）',
      'Kimi Code 会员',
    ]);
    const failed = rows.find((row) => row.sourceId === 'kimi-stale');
    const lead = rows.find((row) => row.sourceId === 'kimi-1');
    expect(failed).toMatchObject({
      isolated: true,
      health: 'needs_login',
      reason: '需要重新登录',
      lead: false,
    });
    expect(lead).toMatchObject({
      isolated: false,
      health: 'renewable',
      reason: '可用',
      lead: true,
    });
    expect(memberPinTone(failed!)).toBe('danger');
    expect(memberPinTone(lead!)).toBe('success');
  });

  it('overlays AuthHealth when the C1 member has no wire health', () => {
    const group: TicketSurfaceGroupView = {
      ...twoMemberGroup,
      members: twoMemberGroup.members.map(({ health: _ignored, ...member }) => member),
    };
    const rows = bridgeMemberRows({
      profile: profile(),
      groups: [group],
      entries: [
        entry({
          source: 'provider',
          id: 'kimi-1',
          title: 'Kimi Code 会员',
          authHealth: 'configured',
        }),
        entry({
          source: 'account',
          id: 'kimi-stale',
          title: 'Kimi 会员（失效号）',
          authStatus: 'expired',
          authHealth: 'needs_login',
        }),
      ],
    });
    expect(rows.find((row) => row.sourceId === 'kimi-stale')?.health).toBe('needs_login');
    expect(rows.find((row) => row.sourceId === 'kimi-1')?.health).toBe('renewable');
  });

  it('greys a missing source instead of hiding the lead row', () => {
    const rows = bridgeMemberRows({
      profile: profile(),
      groups: [],
      entries: [],
    });
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      isolated: true,
      missing: true,
      lead: true,
      reason: '来源连接已删除',
    });
  });

  it('finds the C1 group for the profile lead ticket', () => {
    expect(surfaceGroupForProfile([twoMemberGroup], profile())?.members).toHaveLength(2);
    expect(surfaceGroupForProfile([twoMemberGroup], profile({ sourceId: 'other' }))).toBeUndefined();
  });
});
