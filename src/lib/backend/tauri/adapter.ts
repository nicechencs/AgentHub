import type { AdapterPort } from '@/lib/backend/contracts/adapter';
import {
  mapAdapterApplyPlan,
  mapAdapterApplyResult,
  mapAdapterBridgeStatusDto,
  mapAdapterProfile,
  mapAdapterRouteAnalysis,
  type AdapterApplyPlanWire,
  type AdapterApplyResultWire,
  type AdapterBridgeStatusDtoWire,
  type AdapterProfileWire,
  type AdapterRouteAnalysisWire,
} from '@/lib/backend/contracts/adapter-wire';
import { invoke } from './invoke';

/** Tauri-only route preview transport. */
export function createTauriAdapterPort(): AdapterPort {
  return {
    async analyze(request) {
      const wire = await invoke<AdapterRouteAnalysisWire>('analyze_adapter', { ...request });
      return mapAdapterRouteAnalysis(wire);
    },
    async plan(request) {
      const wire = await invoke<AdapterApplyPlanWire>('plan_adapter', { ...request });
      return mapAdapterApplyPlan(wire);
    },
    async listProfiles(filter) {
      const wire = await invoke<AdapterProfileWire[]>('list_adapter_profiles', { ...filter });
      return wire.map(mapAdapterProfile);
    },
    async apply(request) {
      const wire = await invoke<AdapterApplyResultWire>('apply_adapter', { ...request });
      return mapAdapterApplyResult(wire);
    },
    async remove(profileId) {
      await invoke<void>('remove_adapter', { profileId });
    },
    async startBridge(profileId) {
      const wire = await invoke<AdapterBridgeStatusDtoWire>('start_adapter_bridge', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async stopBridge(profileId) {
      const wire = await invoke<AdapterBridgeStatusDtoWire>('stop_adapter_bridge', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async getBridgeStatus(profileId) {
      const wire = await invoke<AdapterBridgeStatusDtoWire>('get_adapter_bridge_status', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async setBridgeAutoStart(profileId, autoStart) {
      const wire = await invoke<AdapterProfileWire>('set_adapter_bridge_auto_start', {
        profileId,
        autoStart,
      });
      return mapAdapterProfile(wire);
    },
  };
}
