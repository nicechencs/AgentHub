/**
 * Provider API façade — delegates to app runtime backend.
 */
import { getBackend, loadAgentStatuses } from '@/app/runtime';
import { clearLiveAuthProbeCache } from '@/lib/backend/contracts/live-auth-probe-cache';
import type { AgentId, Provider, SwitchPreview } from '@/lib/types';

export type {
  CoreProvider,
  CoreProviderInput,
  CoreSwitchResult,
} from '@/lib/backend/contracts/provider-map';
export { mapCoreProvider, toCoreInput } from '@/lib/backend/contracts/provider-map';
export type { CoreProviderPreset } from '@/lib/backend/contracts/skill-types';

export async function listProviderPresets(agentId?: AgentId) {
  return getBackend().provider.listProviderPresets(agentId);
}

export async function listProviders(agentId?: AgentId): Promise<Provider[]> {
  return getBackend().provider.listProviders(agentId);
}

function providerAuthStateChanged(agentId: AgentId): void {
  clearLiveAuthProbeCache(agentId);
  void loadAgentStatuses(getBackend(), { force: true }).catch(() => {});
}

export async function upsertProvider(p: Provider): Promise<Provider> {
  const provider = await getBackend().provider.upsertProvider(p);
  providerAuthStateChanged(p.agentId);
  return provider;
}

export async function deleteProvider(agentId: AgentId, providerId: string): Promise<void> {
  await getBackend().provider.deleteProvider(agentId, providerId);
  providerAuthStateChanged(agentId);
}

export async function importProviderLive(agentId: AgentId, name?: string): Promise<Provider> {
  return getBackend().provider.importProviderLive(agentId, name);
}

export async function switchPreview(agentId: AgentId, toProviderId: string): Promise<SwitchPreview> {
  return getBackend().provider.switchPreview(agentId, toProviderId);
}

export async function switchProvider(agentId: AgentId, toProviderId: string): Promise<void> {
  await getBackend().provider.switchProvider(agentId, toProviderId);
  providerAuthStateChanged(agentId);
}

export async function undoSwitch(agentId: AgentId): Promise<boolean> {
  const undone = await getBackend().provider.undoSwitch(agentId);
  if (undone) providerAuthStateChanged(agentId);
  return undone;
}

export async function testLatency(agentId: AgentId, providerId: string): Promise<number> {
  return getBackend().provider.testLatency(agentId, providerId);
}
