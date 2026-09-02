import type { AgentKey, Provider, SwitchPreview } from '@/lib/types';
import type { CoreProviderPreset } from './skill-types';

export type DetectedApiEndpointType = 'messages' | 'responses' | 'chat_completions';

export interface ProviderPort {
  listProviders(agentId?: AgentKey): Promise<Provider[]>;
  upsertProvider(p: Provider): Promise<Provider>;
  deleteProvider(agentId: AgentKey, providerId: string): Promise<void>;
  importProviderLive(agentId: AgentKey, name?: string): Promise<Provider>;
  switchPreview(agentId: AgentKey, toProviderId: string): Promise<SwitchPreview>;
  switchProvider(agentId: AgentKey, toProviderId: string): Promise<void>;
  undoSwitch(agentId: AgentKey): Promise<boolean>;
  testLatency(agentId: AgentKey, providerId: string): Promise<number>;
  listProviderPresets(agentId?: AgentKey): Promise<CoreProviderPreset[]>;
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
