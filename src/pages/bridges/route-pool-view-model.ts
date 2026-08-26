/**
 * Pure view-model for Routes pool v2 (flag-gated). No React, no IO.
 */
import type {
  AdapterProfile,
  DefaultRoutePoolOverview,
  RouteMemberOverview,
  RoutePoolSurface,
} from '@/lib/backend/contracts/adapter';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { TranslateFn } from '@/lib/i18n';

export function defaultPoolEntryUrl(port?: number | null): { url: string | null; pending: boolean } {
  if (typeof port === 'number' && port > 0) {
    return { url: `http://127.0.0.1:${port}`, pending: false };
  }
  return { url: null, pending: true };
}

export function routePoolMembersSectionVisible(
  flagOn: boolean,
  pool: DefaultRoutePoolOverview | null | undefined,
): boolean {
  return flagOn === true && pool != null;
}

export function nativeEnrollCtaVisible(input: {
  flagOn: boolean;
  route: AdapterProfile['route'];
  canApplyLocalBridge: boolean;
}): boolean {
  if (!input.flagOn) return false;
  if (input.route !== 'native_endpoint' && input.route !== 'config_sync') return false;
  return input.canApplyLocalBridge === true;
}

export function matchDefaultPoolForProfile(
  pools: readonly DefaultRoutePoolOverview[],
  profile: Pick<AdapterProfile, 'id' | 'sourceKind' | 'sourceId' | 'targetAgentId'>,
): DefaultRoutePoolOverview | null {
  const byId = pools.find((pool) => pool.id === profile.id);
  if (byId) return byId;
  return pools.find((pool) => (
    pool.targetAgentId === profile.targetAgentId
    && pool.members.some((member) => (
      member.sourceKind === profile.sourceKind && member.sourceId === profile.sourceId
    ))
  )) ?? null;
}

export function directProfilesForRoutePoolV2<T extends AdapterProfile>(
  flagOn: boolean,
  profiles: readonly T[],
): T[] {
  if (!flagOn) return [];
  return profiles.filter((profile) => {
    if (profile.route !== 'native_endpoint' && profile.route !== 'config_sync') return false;
    return !profiles.some((sibling) => (
      sibling.route === 'local_bridge'
      && sibling.sourceKind === profile.sourceKind
      && sibling.sourceId === profile.sourceId
      && sibling.targetAgentId === profile.targetAgentId
    ));
  });
}

export function routePoolMemberLabels(
  members: readonly RouteMemberOverview[],
  entries: readonly Pick<ConnectionEntry, 'source' | 'id' | 'title'>[],
): { title: string; enabled: boolean; sourceKind: RouteMemberOverview['sourceKind']; sourceId: string }[] {
  return members.map((member) => {
    const match = entries.find(
      (entry) => entry.source === member.sourceKind && entry.id === member.sourceId,
    );
    return {
      title: match?.title?.trim() || member.sourceId,
      enabled: member.enabled,
      sourceKind: member.sourceKind,
      sourceId: member.sourceId,
    };
  });
}

export function routePoolSurfaceLabel(surface: RoutePoolSurface, t?: TranslateFn): string {
  if (surface === 'messages') {
    return t ? t('routes.pool.surface.messages') : '对话接口';
  }
  if (surface === 'responses') {
    return t ? t('routes.pool.surface.responses') : '回复接口';
  }
  return t ? t('routes.pool.surface.chatCompletions') : '对话补全';
}
