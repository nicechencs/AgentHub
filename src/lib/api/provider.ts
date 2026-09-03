/**
 * Provider API façade — delegates to app runtime backend.
 */
import {
  beginConnectionInventoryMutation,
  endConnectionInventoryMutation,
  getBackend,
  markConnectionCurrent,
  refreshRuntimeReadModels,
} from '@/app/runtime';
import type { AgentKey, Provider, SwitchPreview } from '@/lib/types';

export type {
  CoreProvider,
  CoreProviderInput,
  CoreSwitchResult,
} from '@/lib/backend/contracts/provider-map';
export { mapCoreProvider, toCoreInput } from '@/lib/backend/contracts/provider-map';
export type { CoreProviderPreset } from '@/lib/backend/contracts/skill-types';

export async function listProviderPresets(agentId?: AgentKey) {
  return getBackend().provider.listProviderPresets(agentId);
}

export async function listProviders(agentId?: AgentKey): Promise<Provider[]> {
  return getBackend().provider.listProviders(agentId);
}

function providerAuthStateChanged(agentId: AgentKey): void {
  void refreshRuntimeReadModels(getBackend(), {
    agentId,
    clearProbe: true,
  });
}

export async function upsertProvider(p: Provider): Promise<Provider> {
  const provider = await getBackend().provider.upsertProvider(p);
  providerAuthStateChanged(p.agentId);
  return provider;
}

export async function deleteProvider(agentId: AgentKey, providerId: string): Promise<void> {
  await getBackend().provider.deleteProvider(agentId, providerId);
  providerAuthStateChanged(agentId);
}

export async function importProviderLive(agentId: AgentKey, name?: string): Promise<Provider> {
  const imported = await getBackend().provider.importProviderLive(agentId, name);
  providerAuthStateChanged(agentId);
  return imported;
}

export async function switchPreview(agentId: AgentKey, toProviderId: string): Promise<SwitchPreview> {
  return getBackend().provider.switchPreview(agentId, toProviderId);
}

export async function switchProvider(agentId: AgentKey, toProviderId: string): Promise<void> {
  await getBackend().provider.switchProvider(agentId, toProviderId);
  markConnectionCurrent(agentId, 'provider', toProviderId);
  providerAuthStateChanged(agentId);
}

/** Deletes every listed Provider and refreshes shared read models once. */
export async function deleteProviders(agentId: AgentKey, providerIds: readonly string[]): Promise<void> {
  if (providerIds.length === 0) return;
  const backend = getBackend();
  beginConnectionInventoryMutation();
  try {
    for (const providerId of providerIds) {
      await backend.provider.deleteProvider(agentId, providerId);
    }
  } finally {
    await endConnectionInventoryMutation(backend);
  }
  void refreshRuntimeReadModels(backend, {
    agentId,
    clearProbe: true,
    models: ['agentStatus', 'ticketWallet'],
  });
}

export async function undoSwitch(agentId: AgentKey): Promise<boolean> {
  const undone = await getBackend().provider.undoSwitch(agentId);
  if (undone) providerAuthStateChanged(agentId);
  return undone;
}

export async function testLatency(agentId: AgentKey, providerId: string): Promise<number> {
  return getBackend().provider.testLatency(agentId, providerId);
}

/** OpenAI-compatible GET {base}/v1/models via the desktop HTTP command. */
export async function listRemoteOpenAiModels(
  baseUrl: string,
  apiKey: string,
): Promise<string[]> {
  return getBackend().provider.listRemoteOpenAiModels(baseUrl, apiKey);
}

/** GET {base}/v1/models using the stored provider secret. Never send a raw key. */
export async function listRemoteOpenAiModelsForProvider(
  providerId: string,
  baseUrl: string,
): Promise<string[]> {
  return getBackend().provider.listRemoteOpenAiModelsForProvider(providerId, baseUrl);
}

export async function detectApiEndpointTypes(baseUrl: string, apiKey: string) {
  return getBackend().provider.detectApiEndpointTypes(baseUrl, apiKey);
}
