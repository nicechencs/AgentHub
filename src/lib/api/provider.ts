/**
 * Provider API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
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

export async function upsertProvider(p: Provider): Promise<Provider> {
  return getBackend().provider.upsertProvider(p);
}

export async function deleteProvider(agentId: AgentId, providerId: string): Promise<void> {
  return getBackend().provider.deleteProvider(agentId, providerId);
}

export async function importProviderLive(agentId: AgentId, name?: string): Promise<Provider> {
  return getBackend().provider.importProviderLive(agentId, name);
}

export async function switchPreview(agentId: AgentId, toProviderId: string): Promise<SwitchPreview> {
  return getBackend().provider.switchPreview(agentId, toProviderId);
}

export async function switchProvider(agentId: AgentId, toProviderId: string): Promise<void> {
  return getBackend().provider.switchProvider(agentId, toProviderId);
}

export async function undoSwitch(agentId: AgentId): Promise<boolean> {
  return getBackend().provider.undoSwitch(agentId);
}

export async function testLatency(agentId: AgentId, providerId: string): Promise<number> {
  return getBackend().provider.testLatency(agentId, providerId);
}
