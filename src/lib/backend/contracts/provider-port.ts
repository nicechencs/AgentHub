import type { AgentId, Provider, SwitchPreview } from '@/lib/types';
import type { CoreProviderPreset } from './skill-types';

export type DetectedApiEndpointType = 'messages' | 'responses' | 'chat_completions';

export interface ProviderPort {
  listProviders(agentId?: AgentId): Promise<Provider[]>;
  upsertProvider(p: Provider): Promise<Provider>;
  deleteProvider(agentId: AgentId, providerId: string): Promise<void>;
  importProviderLive(agentId: AgentId, name?: string): Promise<Provider>;
  switchPreview(agentId: AgentId, toProviderId: string): Promise<SwitchPreview>;
  switchProvider(agentId: AgentId, toProviderId: string): Promise<void>;
  undoSwitch(agentId: AgentId): Promise<boolean>;
  testLatency(agentId: AgentId, providerId: string): Promise<number>;
  listProviderPresets(agentId?: AgentId): Promise<CoreProviderPreset[]>;
  /** OpenAI-compatible GET {base}/v1/models. Unsaved paste is allowed. */
  listRemoteOpenAiModels(baseUrl: string, apiKey: string): Promise<string[]>;
  /**
   * GET {base}/v1/models using the hub's unredacted stored secret.
   * Frontend passes provider id + baseUrl only — never the raw key.
   */
  listRemoteOpenAiModelsForProvider(providerId: string, baseUrl: string): Promise<string[]>;
  /** Probes unsupported/valid endpoint responses without sending a model request. */
  detectApiEndpointTypes(
    baseUrl: string,
    apiKey: string,
  ): Promise<DetectedApiEndpointType[]>;
}
