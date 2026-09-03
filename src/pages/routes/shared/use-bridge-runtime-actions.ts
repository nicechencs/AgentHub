/**
 * Routes page bridge runtime mutations: start / stop / remove / enroll.
 */
import { useCallback, useState } from 'react';
import {
  enrollNativeToGateway,
  startAdapterBridge,
  startLocalGateway,
  stopAdapterBridge,
  stopLocalGateway,
} from '@/lib/api/adapter';
import { guiErrorCode, logGuiEvent } from '@/lib/api/settings';
import { listTicketWallet, ticketIdFor, unbindTicket } from '@/lib/api/tickets';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { TranslateFn } from '@/lib/i18n';
import { localBridgeProfilesForSource } from './adapter-view-model';
import { surfaceAfterCompensation } from './create-route-flow';

type ToastFn = (input: { title: string; variant?: 'success' | 'danger' | 'default' }) => void;

export function useBridgeRuntimeActions(input: {
  profiles: readonly AdapterProfile[];
  hiddenTargetIds: ReadonlySet<string>;
  reloadProfiles: () => Promise<void>;
  updateBridgeStatus: (status: Awaited<ReturnType<typeof startAdapterBridge>>) => void;
  removeProfile: (profileId: string) => void;
  t: TranslateFn;
  toast: ToastFn;
  onEnrollDone?: () => void;
}) {
  const {
    profiles,
    hiddenTargetIds,
    reloadProfiles,
    updateBridgeStatus,
    removeProfile,
    t,
    toast,
    onEnrollDone,
  } = input;

  const [removeConfirm, setRemoveConfirm] = useState<AdapterProfile | null>(null);
  const [stopConfirm, setStopConfirm] = useState<AdapterProfile | null>(null);
  const [removingProfileId, setRemovingProfileId] = useState<string | null>(null);
  const [profileErrors, setProfileErrors] = useState<Record<string, unknown>>({});
  const [busyProfileIds, setBusyProfileIds] = useState<Record<string, boolean>>({});
  const [enrollingProfileId, setEnrollingProfileId] = useState<string | null>(null);

  const setProfileBusy = useCallback((profileId: string, busy: boolean) => {
    setBusyProfileIds((current) => ({ ...current, [profileId]: busy }));
  }, []);

  const clearProfileError = useCallback((profileId: string) => {
    setProfileErrors((current) => {
      const { [profileId]: _ignored, ...remaining } = current;
      return remaining;
    });
  }, []);

  const reloadThenClearProfileErrors = useCallback((profileIds: readonly string[]) => {
    void reloadProfiles().then(
      () => {
        if (profileIds.length === 0) return;
        const affected = new Set(profileIds);
        setProfileErrors((current) => Object.fromEntries(
          Object.entries(current).filter(([profileId]) => !affected.has(profileId)),
        ));
      },
      () => undefined,
    );
  }, [reloadProfiles]);

  const handleStartBridge = useCallback(async (profile: AdapterProfile) => {
    const members = localBridgeProfilesForSource(profiles, profile)
      .filter((member) => !hiddenTargetIds.has(member.targetAgentId));
    if (members.length === 0) return;
    for (const member of members) {
      setProfileBusy(member.id, true);
      clearProfileError(member.id);
    }
    const started: string[] = [];
    try {
      for (const member of members) {
        updateBridgeStatus(await startAdapterBridge(member.id));
        started.push(member.id);
        void logGuiEvent('bridge_start', {
          agent: member.targetAgentId,
          profileId: member.id,
          route: member.route,
        });
      }
      reloadThenClearProfileErrors(members.map((member) => member.id));
    } catch (error) {
      const compensationFailures: unknown[] = [];
      for (const id of [...started].reverse()) {
        try {
          updateBridgeStatus(await stopAdapterBridge(id));
        } catch (cause) {
          compensationFailures.push(cause);
        }
      }
      void logGuiEvent('bridge_start_fail', {
        agent: profile.targetAgentId,
        profileId: profile.id,
        route: profile.route,
        code: guiErrorCode(error),
      });
      setProfileErrors((current) => ({
        ...current,
        [profile.id]: surfaceAfterCompensation(error, compensationFailures),
      }));
    } finally {
      for (const member of members) setProfileBusy(member.id, false);
    }
  }, [
    profiles,
    hiddenTargetIds,
    setProfileBusy,
    clearProfileError,
    updateBridgeStatus,
    reloadThenClearProfileErrors,
  ]);

  const confirmStopBridge = useCallback(async () => {
    if (!stopConfirm) return;
    const profile = stopConfirm;
    const members = localBridgeProfilesForSource(profiles, profile);
    for (const member of members) {
      setProfileBusy(member.id, true);
      clearProfileError(member.id);
    }
    try {
      for (const member of members) {
        updateBridgeStatus(await stopAdapterBridge(member.id));
        void logGuiEvent('bridge_stop', {
          agent: member.targetAgentId,
          profileId: member.id,
          route: member.route,
        });
      }
      setStopConfirm(null);
      reloadThenClearProfileErrors(members.map((member) => member.id));
    } catch (error) {
      void logGuiEvent('bridge_stop_fail', {
        agent: profile.targetAgentId,
        profileId: profile.id,
        route: profile.route,
        code: guiErrorCode(error),
      });
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      for (const member of members) setProfileBusy(member.id, false);
    }
  }, [
    stopConfirm,
    profiles,
    setProfileBusy,
    clearProfileError,
    updateBridgeStatus,
    reloadThenClearProfileErrors,
  ]);

  const confirmRemove = useCallback(async () => {
    if (!removeConfirm || hiddenTargetIds.has(removeConfirm.targetAgentId)) return;
    const profile = removeConfirm;
    const members = localBridgeProfilesForSource(profiles, profile);
    const profileId = profile.id;
    setRemovingProfileId(profileId);
    clearProfileError(profileId);
    try {
      const wallet = await listTicketWallet();
      for (const member of members) {
        const binding = wallet.bindings.find((row) => row.profileId === member.id);
        const ticketId = binding?.ticketId ?? ticketIdFor(member.sourceKind, member.sourceId);
        const agentId = binding?.agentId ?? member.targetAgentId;
        await unbindTicket(ticketId, agentId);
        removeProfile(member.id);
        void logGuiEvent('bridge_remove', {
          agent: agentId,
          profileId: member.id,
          route: member.route,
        });
      }
      setRemoveConfirm(null);
      reloadThenClearProfileErrors(members.map((member) => member.id));
    } catch (error) {
      void logGuiEvent('bridge_remove_fail', {
        agent: profile.targetAgentId,
        profileId,
        route: profile.route,
        code: guiErrorCode(error),
      });
      setProfileErrors((errors) => ({ ...errors, [profileId]: error }));
    } finally {
      setRemovingProfileId(null);
    }
  }, [
    removeConfirm,
    hiddenTargetIds,
    profiles,
    clearProfileError,
    removeProfile,
    reloadThenClearProfileErrors,
  ]);

  const handleStartLocalGateway = useCallback(async () => {
    setProfileBusy('__local_gateway__', true);
    clearProfileError('__local_gateway__');
    try {
      const status = await startLocalGateway();
      for (const row of status.statuses) updateBridgeStatus(row);
      void logGuiEvent('bridge_start', { profileId: 'local-entry', route: 'local_bridge' });
      reloadThenClearProfileErrors(['__local_gateway__']);
      return status.running;
    } catch (error) {
      void logGuiEvent('bridge_start_fail', {
        profileId: 'local-entry',
        route: 'local_bridge',
        code: guiErrorCode(error),
      });
      setProfileErrors((current) => ({
        ...current,
        __local_gateway__: surfaceAfterCompensation(error, []),
      }));
      return false;
    } finally {
      setProfileBusy('__local_gateway__', false);
    }
  }, [
    clearProfileError,
    updateBridgeStatus,
    reloadThenClearProfileErrors,
  ]);

  const handleStopLocalGateway = useCallback(async () => {
    setProfileBusy('__local_gateway__', true);
    clearProfileError('__local_gateway__');
    try {
      const status = await stopLocalGateway();
      for (const row of status.statuses) updateBridgeStatus(row);
      void logGuiEvent('bridge_stop', { profileId: 'local-entry', route: 'local_bridge' });
      reloadThenClearProfileErrors(['__local_gateway__']);
      return !status.running;
    } catch (error) {
      void logGuiEvent('bridge_stop_fail', {
        profileId: 'local-entry',
        route: 'local_bridge',
        code: guiErrorCode(error),
      });
      setProfileErrors((current) => ({ ...current, __local_gateway__: error }));
      return false;
    } finally {
      setProfileBusy('__local_gateway__', false);
    }
  }, [
    setProfileBusy,
    clearProfileError,
    updateBridgeStatus,
    reloadThenClearProfileErrors,
  ]);

  const handleEnrollNative = useCallback(async (profile: AdapterProfile) => {
    setEnrollingProfileId(profile.id);
    clearProfileError(profile.id);
    try {
      await enrollNativeToGateway(profile.id);
      void logGuiEvent('bridge_enroll', {
        agent: profile.targetAgentId,
        profileId: profile.id,
        route: profile.route,
      });
      toast({ title: t('routes.pool.enrollSuccess'), variant: 'success' });
      onEnrollDone?.();
      reloadThenClearProfileErrors([profile.id]);
    } catch (error) {
      void logGuiEvent('bridge_enroll_fail', {
        agent: profile.targetAgentId,
        profileId: profile.id,
        route: profile.route,
        code: guiErrorCode(error),
      });
      setProfileErrors((current) => ({ ...current, [profile.id]: error }));
    } finally {
      setEnrollingProfileId(null);
    }
  }, [clearProfileError, toast, t, onEnrollDone, reloadThenClearProfileErrors]);

  return {
    removeConfirm,
    setRemoveConfirm,
    stopConfirm,
    setStopConfirm,
    removingProfileId,
    profileErrors,
    busyProfileIds,
    enrollingProfileId,
    handleStartBridge,
    handleStartLocalGateway,
    handleStopLocalGateway,
    confirmStopBridge,
    confirmRemove,
    handleEnrollNative,
  };
}
