import { describe, expect, it } from 'vitest';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import {
  buildPoolWorkbenchRows,
  defaultPoolEntryUrl,
  directProfilesForRoutePoolV2,
  leadProfileForPool,
  localBridgesNotInPools,
  matchDefaultPoolForProfile,
  nativeEnrollCtaVisible,
  routePoolMemberLabels,
  routePoolMembersSectionVisible,
  routePoolSurfaceLabel,
} from './route-pool-view-model';

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
    ruleId: 'kimi-membership-to-codex-v1',
    ruleVersion: '1',
    generatedProviderId: 'codex-bridge-1',
    localPort: 43121,
    autoStart: true,
    createdAt: '2026-08-12T00:00:00Z',
    updatedAt: '2026-08-12T00:00:00Z',
    ...partial,
  };
}

function pool(partial: Partial<DefaultRoutePoolOverview> = {}): DefaultRoutePoolOverview {
  return {
    id: 'bridge-1',
    targetAgentId: 'codex',
    surface: 'responses',
    dialect: 'codex',
    v2Enrolled: true,
    gatewayPort: 43121,
    members: [{ sourceKind: 'provider', sourceId: 'kimi-1', enabled: true }],
    listedModels: ['kimi-k2.5'],
    ...partial,
  };
}

describe('route pool v2 view-model', () => {
  it('hides enroll CTA and members when the flag is off', () => {
    expect(routePoolMembersSectionVisible(false, pool())).toBe(false);
    expect(nativeEnrollCtaVisible({
      flagOn: false,
      route: 'native_endpoint',
      canApplyLocalBridge: true,
    })).toBe(false);
    expect(directProfilesForRoutePoolV2(false, [
      profile({ id: 'native-1', route: 'native_endpoint' }),
    ])).toEqual([]);
  });

  it('shows entry URL, surface, and member titles for a default pool when the flag is on', () => {
    const overview = pool();
    expect(routePoolMembersSectionVisible(true, overview)).toBe(true);
    expect(defaultPoolEntryUrl(overview.gatewayPort)).toEqual({
      url: 'http://127.0.0.1:43121',
      pending: false,
    });
    expect(routePoolSurfaceLabel(overview.surface)).toBe('回复接口');
    const entries: Pick<ConnectionEntry, 'source' | 'id' | 'title'>[] = [
      { source: 'provider', id: 'kimi-1', title: 'Kimi 会员' },
    ];
    expect(routePoolMemberLabels(overview.members, entries)).toEqual([
      {
        title: 'Kimi 会员',
        enabled: true,
        availability: undefined,
        sourceKind: 'provider',
        sourceId: 'kimi-1',
      },
    ]);
    expect(JSON.stringify(overview)).not.toContain('hubToken');
    expect(JSON.stringify(overview)).not.toContain('ahb_');
  });

  it('uses pending copy when the gateway port is not allocated', () => {
    expect(defaultPoolEntryUrl(null)).toEqual({ url: null, pending: true });
    expect(defaultPoolEntryUrl(0)).toEqual({ url: null, pending: true });
  });

  it('shows enroll CTA only for native/config_sync when plan allows local_bridge', () => {
    expect(nativeEnrollCtaVisible({
      flagOn: true,
      route: 'native_endpoint',
      canApplyLocalBridge: true,
    })).toBe(true);
    expect(nativeEnrollCtaVisible({
      flagOn: true,
      route: 'config_sync',
      canApplyLocalBridge: true,
    })).toBe(true);
    expect(nativeEnrollCtaVisible({
      flagOn: true,
      route: 'native_endpoint',
      canApplyLocalBridge: false,
    })).toBe(false);
    expect(nativeEnrollCtaVisible({
      flagOn: true,
      route: 'local_bridge',
      canApplyLocalBridge: true,
    })).toBe(false);
  });

  it('lists direct profiles when the flag is on and hides ones already on a local route', () => {
    const native = profile({ id: 'native-1', route: 'native_endpoint', sourceId: 'acc-1' });
    const converted = profile({ id: 'native-2', route: 'native_endpoint', sourceId: 'acc-2' });
    const sibling = profile({ id: 'bridge-2', sourceId: 'acc-2', route: 'local_bridge' });
    expect(directProfilesForRoutePoolV2(true, [native, converted, sibling]).map((item) => item.id))
      .toEqual(['native-1']);
  });

  it('picks the pool lead by id, then by matching member', () => {
    const overview = pool();
    expect(leadProfileForPool(overview, [profile()])?.id).toBe('bridge-1');
    const memberOnly = profile({ id: 'other', sourceId: 'kimi-1', targetAgentId: 'codex' });
    expect(leadProfileForPool({ ...overview, id: 'pool-x' }, [memberOnly])?.id).toBe('other');
    expect(leadProfileForPool(overview, [profile({ id: 'miss', sourceId: 'other' })])).toBeNull();
  });

  it('builds workbench rows from default pools plus unmatched local routes', () => {
    const enrolled = profile();
    const extra = profile({
      id: 'bridge-extra',
      sourceId: 'openai-1',
      targetAgentId: 'claude',
      localPort: 43122,
    });
    const rows = buildPoolWorkbenchRows({
      flagOn: true,
      pools: [pool()],
      profiles: [enrolled, extra],
    });
    expect(rows.map((row) => row.key)).toEqual(['bridge-1', 'bridge-extra']);
    expect(rows[0]?.pool?.id).toBe('bridge-1');
    expect(rows[1]?.pool).toBeNull();
    expect(localBridgesNotInPools([pool()], [enrolled, extra]).map((item) => item.id))
      .toEqual(['bridge-extra']);
  });

  it('falls back to one card per local route when the pool flag is off', () => {
    const rows = buildPoolWorkbenchRows({
      flagOn: false,
      pools: [pool()],
      profiles: [profile(), profile({ id: 'native-1', route: 'native_endpoint' })],
    });
    expect(rows.map((row) => row.key)).toEqual(['bridge-1']);
    expect(rows[0]?.pool).toBeNull();
  });

  it('collapses multiple native profiles from one source into one direct row', () => {
    const claude = profile({
      id: 'native-claude',
      route: 'native_endpoint',
      sourceId: 'ds-1',
      targetAgentId: 'claude',
      createdAt: '2026-08-12T00:00:02Z',
    });
    const codex = profile({
      id: 'native-codex',
      route: 'native_endpoint',
      sourceId: 'ds-1',
      targetAgentId: 'codex',
      createdAt: '2026-08-12T00:00:00Z',
    });
    expect(directProfilesForRoutePoolV2(true, [claude, codex]).map((item) => item.id))
      .toEqual(['native-codex']);
  });

  it('matches a default pool by profile id', () => {
    expect(matchDefaultPoolForProfile([pool()], profile())?.id).toBe('bridge-1');
    expect(matchDefaultPoolForProfile(
      [pool()],
      profile({ id: 'other', sourceId: 'other-src', targetAgentId: 'claude' }),
    )).toBeNull();
  });
});
