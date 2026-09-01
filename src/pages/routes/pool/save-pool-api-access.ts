import type { AgentConfigSchemaDto, ConfigValidationResultDto } from '@/lib/api/config';
import { runProviderSaveFlow } from '@/lib/api/provider-save';
import type { AdapterSourceKind, RoutePoolSurface } from '@/lib/backend/contracts';
import type { AgentId, Provider } from '@/lib/types';
import { defaultConfigScaffold } from '@/lib/provider-detect/scaffold';
import { EMPTY_FORM_VARS, type ProviderFormVars } from '@/lib/provider-detect/types';
import {
  poolApiRecordName,
  poolSurfaceForApiChoice,
  type PoolAccessAgent,
  type PoolApiSaveItem,
} from './api-access-model';

export type SavePoolApiAccessDeps = {
  getAgentConfigSchema: (agentId: string) => Promise<AgentConfigSchemaDto>;
  validateAgentConfig: (
    agentId: string,
    values: Record<string, unknown>,
  ) => Promise<ConfigValidationResultDto>;
  materializeAgentConfig: (
    agentId: string,
    values: Record<string, unknown>,
    baseRaw?: unknown,
  ) => Promise<unknown>;
  applyFormVars: (
    agentId: AgentId,
    configText: string,
    format: 'json' | 'toml',
    vars: ProviderFormVars,
  ) => string;
  upsertProvider: (provider: Provider) => Promise<Provider>;
  attachAuthorization: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    targetAgentId: PoolAccessAgent,
    surface: RoutePoolSurface,
  ) => Promise<void>;
  setSourceCustomModels?: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    models: string[],
  ) => Promise<unknown>;
  setAuthorizationPriority?: (
    sourceKind: AdapterSourceKind,
    sourceId: string,
    priority: number,
  ) => Promise<number>;
};

export type SavePoolApiAccessResult = {
  saved: number;
  errors: string[];
};

function formVarsForItem(item: PoolApiSaveItem, apiKey: string): ProviderFormVars {
  return {
    ...EMPTY_FORM_VARS,
    baseUrl: item.baseUrl,
    apiKey,
    ...(item.choice.grokApiBackend ? { apiBackend: item.choice.grokApiBackend } : {}),
    ...(item.choice.type === 'openaiResponses' ? { wireApi: 'responses' } : {}),
  };
}

/** Save one pool login per API key and selected type, then attach each to the default pool. */
export async function savePoolApiAccess(
  input: {
    items: readonly PoolApiSaveItem[];
    apiKeys: readonly string[];
    models?: readonly string[];
    priority?: number | null;
    edit?: { provider: Provider };
  },
  deps: SavePoolApiAccessDeps,
): Promise<SavePoolApiAccessResult> {
  const errors: string[] = [];
  let saved = 0;
  const models = [...new Set((input.models ?? []).map((model) => model.trim()).filter(Boolean))];
  const priority = input.priority ?? null;
  const editProvider = input.edit?.provider;
  const items = editProvider ? input.items.slice(0, 1) : input.items;
  const apiKeys = editProvider
    ? [input.apiKeys[0] ?? '']
    : input.apiKeys.filter((key) => key.trim());
  let sequence = 0;

  for (const apiKey of apiKeys) {
    if (!editProvider && !apiKey.trim()) continue;
    for (const item of items) {
      const scaffold = defaultConfigScaffold(item.choice.agentId);
      const vars = formVarsForItem(item, apiKey);
      let configSchema: AgentConfigSchemaDto | null = null;
      let schemaStatus: 'ready' | 'unsupported' = 'unsupported';
      try {
        configSchema = await deps.getAgentConfigSchema(item.choice.agentId);
        schemaStatus = 'ready';
      } catch {
        schemaStatus = 'unsupported';
      }

      const id = editProvider?.id ?? `p-${Date.now()}-${item.choice.type}-${sequence}`;
      sequence += 1;
      try {
        const result = await runProviderSaveFlow(
          {
            agentId: item.choice.agentId,
            schemaStatus,
            configSchema,
            isEdit: Boolean(editProvider),
            existing: editProvider,
            id,
            name: editProvider?.name || poolApiRecordName(item.baseUrl, item.choice.endpoint),
            useOfficial: false,
            configText: editProvider?.configText || scaffold.text,
            configFormat: editProvider?.configFormat || scaffold.format,
            vars,
            saveVars: vars,
            finalFormat: editProvider?.configFormat || scaffold.format,
            baseText: editProvider?.configText || scaffold.text,
          },
          {
            validateAgentConfig: deps.validateAgentConfig,
            materializeAgentConfig: deps.materializeAgentConfig,
            applyFormVars: deps.applyFormVars,
            upsertProvider: deps.upsertProvider,
          },
        );
        if (!result.ok) {
          errors.push(result.message);
          continue;
        }
        if (!editProvider) {
          await deps.attachAuthorization(
            'provider',
            result.provider.id,
            item.choice.agentId,
            poolSurfaceForApiChoice(item.choice),
          );
        }
        if (models.length > 0 && deps.setSourceCustomModels) {
          await deps.setSourceCustomModels('provider', result.provider.id, models);
        }
        if (priority !== null && deps.setAuthorizationPriority) {
          await deps.setAuthorizationPriority('provider', result.provider.id, priority);
        }
        saved += 1;
      } catch (error) {
        errors.push(error instanceof Error ? error.message : String(error));
      }
    }
  }

  return { saved, errors };
}
