import {
  adapterCommandError,
  isAdapterErrorCodeRetryable,
  type AdapterPort,
  type SourceModelCatalog,
  type SourceModelCatalogSource,
  type SyncConnectionAuthorizationsRequest,
  type SyncConnectionAuthorizationsResult,
} from '@/lib/backend/contracts/adapter';
import {
  mapAdapterApplyPlan,
  mapAdapterApplyResult,
  mapAdapterBridgeStatusDto,
  mapAdapterProfile,
  mapAdapterRouteAnalysis,
  mapDefaultRoutePoolList,
  mapDefaultRoutePoolOverview,
  mapLocalEntryStatus,
  mapLocalTokenProbeResult,
  mapLocalTokenRecord,
  type AdapterApplyPlanWire,
  type AdapterApplyResultWireInput,
  type AdapterBridgeStatusDtoWire,
  type AdapterProfileWire,
  type LocalEntryStatusWire,
  type LocalTokenProbeResultWire,
  type LocalTokenRecordWire,
  type AdapterRouteAnalysisWire,
  type DefaultRoutePoolListWire,
  type DefaultRoutePoolOverviewWire,
} from '@/lib/backend/contracts/adapter-wire';
import { invoke } from './invoke';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

type SourceModelCatalogWire = {
  models?: unknown;
  source?: unknown;
  canCustomize?: unknown;
};

function mapSourceModelCatalog(wire: SourceModelCatalogWire | null | undefined): SourceModelCatalog {
  const models = Array.isArray(wire?.models)
    ? wire.models.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
    : [];
  const source: SourceModelCatalogSource =
    wire?.source === 'live' || wire?.source === 'custom' || wire?.source === 'empty'
      ? wire.source
      : models.length > 0
        ? 'live'
        : 'empty';
  return {
    models,
    source,
    canCustomize: wire?.canCustomize === true || source !== 'live',
  };
}

function payloadFromUnknown(error: unknown): unknown {
  if (!isRecord(error)) return error;
  if ('code' in error && 'message' in error) return error;
  if ('payload' in error) return error.payload;
  if (isRecord(error.error)) return error.error;
  return error;
}

/** Map a Tauri Adapter command rejection onto the shared structured error. */
export function mapAdapterInvokeError(error: unknown): never {
  const payload = payloadFromUnknown(error);
  if (isRecord(payload) && typeof payload.code === 'string' && typeof payload.message === 'string') {
    throw adapterCommandError({
      code: payload.code,
      message: payload.message,
      details: typeof payload.details === 'string' ? payload.details : null,
      retryable: typeof payload.retryable === 'boolean'
        ? payload.retryable
        : isAdapterErrorCodeRetryable(payload.code),
    });
  }
  if (typeof payload === 'string') {
    const match = payload.match(/^(.*)\s\[([^\]]+)\]\s*$/);
    if (match) {
      throw adapterCommandError({
        code: match[2],
        message: match[1].trim(),
      });
    }
    throw adapterCommandError({
      code: 'adapter.command',
      message: payload,
      retryable: false,
    });
  }
  throw adapterCommandError({
    code: 'adapter.command',
    message: error instanceof Error ? error.message : '操作失败',
    retryable: false,
  });
}

async function invokeAdapter<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    mapAdapterInvokeError(error);
  }
}

/** Tauri-only route preview transport. */
export function createTauriAdapterPort(): AdapterPort {
  return {
    async analyze(request) {
      const wire = await invokeAdapter<AdapterRouteAnalysisWire>('analyze_adapter', { ...request });
      return mapAdapterRouteAnalysis(wire);
    },
    async plan(request) {
      const wire = await invokeAdapter<AdapterApplyPlanWire>('plan_adapter', { ...request });
      return mapAdapterApplyPlan(wire);
    },
    async listProfiles(filter) {
      const wire = await invokeAdapter<AdapterProfileWire[]>('list_adapter_profiles', { ...filter });
      return wire.map(mapAdapterProfile);
    },
    async listDefaultRoutePools() {
      const wire = await invokeAdapter<DefaultRoutePoolListWire>('list_default_route_pools', {});
      return mapDefaultRoutePoolList(wire);
    },
    async listLocalTokens() {
      const wire = await invokeAdapter<LocalTokenRecordWire[]>('list_local_tokens', {});
      return Array.isArray(wire) ? wire.map(mapLocalTokenRecord) : [];
    },
    async listLocalTokenModels(token) {
      const wire = await invokeAdapter<string[]>('list_local_token_models', { token });
      return Array.isArray(wire)
        ? wire.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
        : [];
    },
    async setLocalTokenCustomModels(token, models) {
      const wire = await invokeAdapter<string[]>('set_local_token_custom_models', { token, models });
      return Array.isArray(wire)
        ? wire.filter((item): item is string => typeof item === 'string' && item.trim().length > 0)
        : [];
    },
    async ensureSourceModelCatalog(sourceKind, sourceId) {
      return mapSourceModelCatalog(
        await invokeAdapter<SourceModelCatalogWire>('ensure_source_model_catalog', {
          sourceKind,
          sourceId,
        }),
      );
    },
    async setSourceCustomModels(sourceKind, sourceId, models) {
      return mapSourceModelCatalog(
        await invokeAdapter<SourceModelCatalogWire>('set_source_custom_models', {
          sourceKind,
          sourceId,
          models,
        }),
      );
    },
    async testLocalToken(endpoint, token, path, model) {
      const trimmedModel = model?.trim();
      const wire = await invokeAdapter<LocalTokenProbeResultWire>('test_local_token', {
        endpoint,
        token,
        path,
        ...(trimmedModel ? { model: trimmedModel } : {}),
      });
      return mapLocalTokenProbeResult(wire ?? {});
    },
    async setLocalToken(poolId, token) {
      const wire = await invokeAdapter<LocalTokenRecordWire>('set_local_token', { poolId, token });
      return mapLocalTokenRecord(wire);
    },
    async setChatCompletionsShared(shared: boolean) {
      const wire = await invokeAdapter<DefaultRoutePoolListWire>('set_chat_completions_shared', { shared });
      return mapDefaultRoutePoolList(wire);
    },
    async attachPoolOwnedAuthorization(request) {
      const wire = await invokeAdapter<DefaultRoutePoolOverviewWire>('attach_pool_owned_authorization', {
        ...request,
      });
      return mapDefaultRoutePoolOverview(wire);
    },
    async setRouteAuthorizationEnabled(sourceKind, sourceId, enabled) {
      return invokeAdapter<number>('set_route_authorization_enabled', {
        sourceKind,
        sourceId,
        enabled,
      });
    },
    async removeRouteAuthorization(sourceKind, sourceId) {
      return invokeAdapter<number>('remove_route_authorization', {
        sourceKind,
        sourceId,
      });
    },
    async recycleRouteMembership(sourceKind, sourceId) {
      return invokeAdapter<number>('recycle_route_membership', {
        sourceKind,
        sourceId,
      });
    },
    async syncConnectionAuthorizations(request?: SyncConnectionAuthorizationsRequest) {
      return invokeAdapter<SyncConnectionAuthorizationsResult>('sync_connection_authorizations', request
        ? { request: { sources: request.sources.map((source) => ({ ...source })) } }
        : {});
    },
    async enrollNativeToGateway(profileId) {
      const wire = await invokeAdapter<DefaultRoutePoolOverviewWire>('enroll_native_to_gateway', {
        profileId,
      });
      return mapDefaultRoutePoolOverview(wire);
    },
    async apply(request) {
      // Thin host command: core `apply_adapter` delegates to bind_ticket_inner.
      // Product UI must write via bindTicket, not this transport.
      const wire = await invokeAdapter<AdapterApplyResultWireInput>('apply_adapter', { ...request });
      return mapAdapterApplyResult(wire);
    },
    async remove(profileId) {
      await invokeAdapter<void>('remove_adapter', { profileId });
    },
    async startBridge(profileId) {
      const wire = await invokeAdapter<AdapterBridgeStatusDtoWire>('start_adapter_bridge', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async stopBridge(profileId) {
      const wire = await invokeAdapter<AdapterBridgeStatusDtoWire>('stop_adapter_bridge', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async getBridgeStatus(profileId) {
      const wire = await invokeAdapter<AdapterBridgeStatusDtoWire>('get_adapter_bridge_status', { profileId });
      return mapAdapterBridgeStatusDto(wire);
    },
    async setBridgeAutoStart(profileId, autoStart) {
      const wire = await invokeAdapter<AdapterProfileWire>('set_adapter_bridge_auto_start', {
        profileId,
        autoStart,
      });
      return mapAdapterProfile(wire);
    },
    async startLocalEntry() {
      const wire = await invokeAdapter<LocalEntryStatusWire>('start_local_entry', {});
      return mapLocalEntryStatus(wire);
    },
    async stopLocalEntry() {
      const wire = await invokeAdapter<LocalEntryStatusWire>('stop_local_entry', {});
      return mapLocalEntryStatus(wire);
    },
    async getLocalEntryStatus() {
      const wire = await invokeAdapter<LocalEntryStatusWire>('get_local_entry_status', {});
      return mapLocalEntryStatus(wire);
    },
  };
}
