/**
 * Routes default-pool listing and native→local_bridge enroll eligibility.
 */
import { useEffect, useState } from 'react';
import { listDefaultRoutePools } from '@/lib/api/adapter';
import { planTicket, ticketIdFor } from '@/lib/api/tickets';
import type { AdapterProfile, DefaultRoutePoolOverview } from '@/lib/backend/contracts/adapter';

export function useRoutePoolState(input: {
  profiles: readonly AdapterProfile[];
  detailTarget: AdapterProfile | null;
  reloadKey?: number;
}) {
  const { profiles, detailTarget, reloadKey = 0 } = input;
  const [routePoolV2, setRoutePoolV2] = useState(false);
  const [chatCompletionsShared, setChatCompletionsShared] = useState(false);
  const [defaultPools, setDefaultPools] = useState<DefaultRoutePoolOverview[]>([]);
  const [loading, setLoading] = useState(true);
  const [nativeCanApplyById, setNativeCanApplyById] = useState<Record<string, boolean>>({});

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void listDefaultRoutePools()
      .then((listed) => {
        if (cancelled) return;
        setRoutePoolV2(listed.enabled);
        setChatCompletionsShared(listed.chatCompletionsShared === true);
        setDefaultPools(listed.pools);
      })
      .catch(() => {
        if (cancelled) return;
        setRoutePoolV2(false);
        setChatCompletionsShared(false);
        setDefaultPools([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [profiles, reloadKey]);

  useEffect(() => {
    if (!routePoolV2 || !detailTarget) return;
    if (detailTarget.route !== 'native_endpoint' && detailTarget.route !== 'config_sync') return;
    const profileId = detailTarget.id;
    let cancelled = false;
    void planTicket(
      ticketIdFor(detailTarget.sourceKind, detailTarget.sourceId),
      detailTarget.targetAgentId,
    )
      .then((plan) => {
        if (cancelled) return;
        setNativeCanApplyById((current) => ({
          ...current,
          [profileId]: plan.canApply && plan.analysis.route === 'local_bridge',
        }));
      })
      .catch(() => {
        if (cancelled) return;
        setNativeCanApplyById((current) => ({ ...current, [profileId]: false }));
      });
    return () => {
      cancelled = true;
    };
  }, [routePoolV2, detailTarget]);

  return {
    routePoolV2,
    chatCompletionsShared,
    defaultPools,
    loading,
    nativeCanApplyById,
  };
}
