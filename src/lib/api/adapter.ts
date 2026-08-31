/** Adapter route preview and the narrow, supported apply façade. */
import { getBackend, refreshRuntimeReadModels } from '@/app/runtime';
import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterProfileFilter,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
  AdapterSourceKind,
  AttachPoolOwnedAuthorizationRequest,
  DefaultRoutePoolList,
  DefaultRoutePoolOverview,
  SyncConnectionAuthorizationsRequest,
  SyncConnectionAuthorizationsResult,
} from '@/lib/backend/contracts/adapter';

export type {
  AdapterAction,
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterBridgeRuntimeState,
  AdapterBridgeRuntimeStatus,
  AdapterEvidence,
  AdapterPlanChange,
  AdapterProfile,
  AdapterProfileFilter,
  AdapterProfileMode,
  AdapterProfileStatus,
  AdapterRoute,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
  AdapterServiceImpact,
  AdapterSourceKind,
  AdapterSupport,
  AttachPoolOwnedAuthorizationRequest,
  SyncConnectionAuthorizationsRequest,
  SyncConnectionAuthorizationsResult,
} from '@/lib/backend/contracts/adapter';

export async function analyzeAdapter(request: AdapterRouteRequest): Promise<AdapterRouteAnalysis> {
  return getBackend().adapter.analyze(request);
}

/** Read-only Phase 0 config/service preview; it never returns a credential. */
export async function planAdapter(request: AdapterRouteRequest): Promise<AdapterApplyPlan> {
  return getBackend().adapter.plan(request);
}

/** Lists credential-free adapter profiles. */
export async function listAdapterProfiles(filter?: AdapterProfileFilter): Promise<AdapterProfile[]> {
  return getBackend().adapter.listProfiles(filter);
}

/** Default-pool overview for Routes. Flag off returns `{ enabled: false, pools: [] }`. */
export async function listDefaultRoutePools(): Promise<DefaultRoutePoolList> {
  return getBackend().adapter.listDefaultRoutePools();
}

/** Enroll an authorization into the default auth pool and keep it off Connections. */
export async function attachPoolOwnedAuthorization(
  request: AttachPoolOwnedAuthorizationRequest,
): Promise<DefaultRoutePoolOverview> {
  const result = await getBackend().adapter.attachPoolOwnedAuthorization(request);
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['connectionPool', 'ticketWallet'] });
  } catch {
    // Write succeeded; the pool store keeps previous rows if refresh fails.
  }
  return result;
}

/** Enable or disable this login in every default pool it belongs to. */
export async function setRouteAuthorizationEnabled(
  sourceKind: AdapterSourceKind,
  sourceId: string,
  enabled: boolean,
): Promise<number> {
  return getBackend().adapter.setRouteAuthorizationEnabled(sourceKind, sourceId, enabled);
}

/** Enroll Connections authorizations into the auth pool without removing them from Connections. */
export async function syncConnectionAuthorizations(
  request?: SyncConnectionAuthorizationsRequest,
): Promise<SyncConnectionAuthorizationsResult> {
  const result = await getBackend().adapter.syncConnectionAuthorizations(request);
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['connectionPool', 'ticketWallet'] });
  } catch {
    // Write succeeded; the pool store keeps previous rows if refresh fails.
  }
  return result;
}

/** Convert a direct login into the target Agent default local route. */
export async function enrollNativeToGateway(profileId: string): Promise<DefaultRoutePoolOverview> {
  const result = await getBackend().adapter.enrollNativeToGateway(profileId);
  await refreshConnectionPoolAfterAdapterMutation();
  return result;
}

async function refreshConnectionPoolAfterAdapterMutation(): Promise<void> {
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['connectionPool'] });
  } catch {
    // The mutation itself succeeded. The pool store keeps previous rows and
    // exposes the refresh error instead of pretending the list is current.
  }
}

/**
 * Host-only apply transport. Product writes go through `bindTicket`.
 *
 * @deprecated Use `bindTicket` from `@/lib/api/tickets`. Pages must not call this.
 */
export async function applyAdapter(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
  const result = await getBackend().adapter.apply(request);
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['connectionPool', 'ticketWallet'] });
  } catch {
    // Same as bindTicket: write succeeded; read models keep the previous snapshot.
  }
  return result;
}

/** Removes the generated projection when it is not the active Connection. */
export async function removeAdapter(profileId: string): Promise<void> {
  await getBackend().adapter.remove(profileId);
  await refreshConnectionPoolAfterAdapterMutation();
}

/** Starts a previously-created local bridge on this machine. */
export async function startAdapterBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus> {
  return getBackend().adapter.startBridge(profileId);
}

/** Stops the bridge listener without deleting its generated Connection. */
export async function stopAdapterBridge(profileId: string): Promise<AdapterBridgeRuntimeStatus> {
  return getBackend().adapter.stopBridge(profileId);
}

/** Reads one credential-free local bridge status. */
export async function getAdapterBridgeStatus(profileId: string): Promise<AdapterBridgeRuntimeStatus> {
  return getBackend().adapter.getBridgeStatus(profileId);
}

/** Persists whether this bridge should restore with the AgentHub desktop host. */
export async function setAdapterBridgeAutoStart(
  profileId: string,
  autoStart: boolean,
): Promise<AdapterProfile> {
  return getBackend().adapter.setBridgeAutoStart(profileId, autoStart);
}
