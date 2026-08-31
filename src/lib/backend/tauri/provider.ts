import type { DetectedApiEndpointType, ProviderPort } from '@/lib/backend/contracts';
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

export const CURSOR_LIVE_WRITE_UNSUPPORTED =
  'Cursor 暂时不能把这份登录写到本机配置。请用 Cursor 自己的登录，或设置 CURSOR_API_KEY。';

function errorPayloadText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error && error.message.trim()) return error.message;
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === 'string' && message.trim()) return message;
  }
  return '';
}

function isUnsupportedProviderSwitch(text: string): boolean {
  return /provider\.switch\.rollback|\bunsupported\b|\[unsupported\]/i.test(text)
    || text.includes('暂时不能把这份登录写到本机配置')
    || text.includes('live config writes are not supported for cursor');
}

/** Map a switch rejection onto a Chinese Error the UI can toast. */
export function mapProviderSwitchError(agentId: string, error: unknown): Error {
  const text = errorPayloadText(error);
  if (agentId === 'cursor' && isUnsupportedProviderSwitch(text)) {
    return new Error(CURSOR_LIVE_WRITE_UNSUPPORTED);
  }
  if (error instanceof Error && error.message.trim()) return error;
  return new Error(text || '切换失败');
}

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
        throw mapProviderSwitchError(agentId, e);
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

    async detectApiEndpointTypes(baseUrl, apiKey) {
      try {
        return await invoke<DetectedApiEndpointType[]>('detect_api_endpoint_types', { baseUrl, apiKey });
      } catch (e) {
        log.error('detect_api_endpoint_types failed', e);
        throw e;
      }
    },
  };
}
