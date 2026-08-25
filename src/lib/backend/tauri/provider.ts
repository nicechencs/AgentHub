import type { ProviderPort } from '@/lib/backend/contracts';
import {
  mapCoreProvider,
  toCoreInput,
  type CoreProvider,
  type CoreSwitchResult,
} from '@/lib/backend/contracts/provider-map';
import type { CoreProviderPreset } from '@/lib/backend/contracts/skill-types';
import type { SwitchPreview } from '@/lib/types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:provider');

export function createTauriProviderPort(): ProviderPort {
  return {
    async listProviders(agentId) {
      try {
        const rows = await invoke<CoreProvider[]>('list_providers', {
          agentId: agentId ?? null,
        });
        return rows.map(mapCoreProvider);
      } catch (e) {
        log.error('list_providers failed', e);
        throw e;
      }
    },

    async upsertProvider(p) {
      try {
        const input = toCoreInput(p);
        const row = await invoke<CoreProvider>('upsert_provider', { input });
        return mapCoreProvider(row);
      } catch (e) {
        log.error('upsert_provider failed', e);
        throw e;
      }
    },

    async deleteProvider(agentId, providerId) {
      try {
        await invoke('delete_provider', { agentId, providerId });
      } catch (e) {
        log.error('delete_provider failed', e);
        throw e;
      }
    },

    async importProviderLive(agentId, name) {
      try {
        const row = await invoke<CoreProvider>('import_provider_live', {
          agentId,
          name: name ?? null,
        });
        return mapCoreProvider(row);
      } catch (e) {
        log.error('import_provider_live failed', e);
        throw e;
      }
    },

    async switchPreview(agentId, toProviderId) {
      try {
        return await invoke<SwitchPreview>('switch_provider_preview', {
          agentId,
          idOrName: toProviderId,
        });
      } catch (e) {
        log.error('switch_provider_preview failed', e);
        throw e;
      }
    },

    async switchProvider(agentId, toProviderId) {
      try {
        await invoke<CoreSwitchResult>('switch_provider', {
          agentId,
          idOrName: toProviderId,
        });
      } catch (e) {
        log.error('switch_provider failed', e);
        throw e;
      }
    },

    async undoSwitch(agentId) {
      try {
        return await invoke<boolean>('undo_switch_provider', { agentId });
      } catch (e) {
        log.error('undo_switch_provider failed', e);
        throw e;
      }
    },

    async testLatency(agentId, providerId) {
      try {
        return await invoke<number>('test_provider_latency', {
          agentId,
          providerId,
        });
      } catch (e) {
        log.error('test_provider_latency failed', e);
        throw e;
      }
    },

    async listProviderPresets(agentId) {
      try {
        return await invoke<CoreProviderPreset[]>('list_provider_presets', {
          agentId: agentId ?? null,
        });
      } catch (e) {
        log.error('list_provider_presets failed', e);
        throw e;
      }
    },

    async listRemoteOpenAiModels(baseUrl, apiKey) {
      try {
        return await invoke<string[]>('list_remote_openai_models', {
          baseUrl,
          apiKey,
        });
      } catch (e) {
        log.error('list_remote_openai_models failed', e);
        throw e;
      }
    },

    async listRemoteOpenAiModelsForProvider(providerId, baseUrl) {
      try {
        return await invoke<string[]>('list_remote_openai_models_for_provider', {
          providerId,
          baseUrl,
        });
      } catch (e) {
        log.error('list_remote_openai_models_for_provider failed', e);
        throw e;
      }
    },
  };
}
