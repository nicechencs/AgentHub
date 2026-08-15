/** Adapter route preview and the narrow, supported apply façade. */
import { getBackend, notifyConnectionPoolChanged } from '@/app/runtime';
import type {
  AdapterApplyPlan,
  AdapterApplyRequest,
  AdapterApplyResult,
  AdapterBridgeRuntimeStatus,
  AdapterProfile,
  AdapterProfileFilter,
  AdapterRouteAnalysis,
  AdapterRouteRequest,
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

async function refreshConnectionPoolAfterAdapterMutation(): Promise<void> {
  try {
    await notifyConnectionPoolChanged(getBackend());
  } catch {
    // The mutation itself succeeded. The pool store keeps previous rows and
    // exposes the refresh error instead of pretending the list is current.
  }
}

/** Applies only a stable adapter route supported by the active backend. */
export async function applyAdapter(request: AdapterApplyRequest): Promise<AdapterApplyResult> {
  const result = await getBackend().adapter.apply(request);
  await refreshConnectionPoolAfterAdapterMutation();
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
