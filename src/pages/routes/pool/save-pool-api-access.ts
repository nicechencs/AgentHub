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

/** Save one pool login per selected API type, then attach each to the default pool. */
export async function savePoolApiAccess(
  input: { items: readonly PoolApiSaveItem[]; apiKey: string },
  deps: SavePoolApiAccessDeps,
): Promise<SavePoolApiAccessResult> {
  const errors: string[] = [];
  let saved = 0;

  for (const item of input.items) {
    const scaffold = defaultConfigScaffold(item.choice.agentId);
    const vars = formVarsForItem(item, input.apiKey);
    let configSchema: AgentConfigSchemaDto | null = null;
    let schemaStatus: 'ready' | 'unsupported' = 'unsupported';
    try {
      configSchema = await deps.getAgentConfigSchema(item.choice.agentId);
      schemaStatus = 'ready';
    } catch {
      schemaStatus = 'unsupported';
    }

    try {
      const result = await runProviderSaveFlow(
        {
          agentId: item.choice.agentId,
          schemaStatus,
          configSchema,
          isEdit: false,
          id: `p-${Date.now()}-${item.choice.type}`,
          name: poolApiRecordName(item.baseUrl, item.choice.endpoint),
          useOfficial: false,
          configText: scaffold.text,
          configFormat: scaffold.format,
          vars,
          saveVars: vars,
          finalFormat: scaffold.format,
          baseText: scaffold.text,
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
      await deps.attachAuthorization(
        'provider',
        result.provider.id,
        item.choice.agentId,
        poolSurfaceForApiChoice(item.choice),
      );
      saved += 1;
    } catch (error) {
      errors.push(error instanceof Error ? error.message : String(error));
    }
  }

  return { saved, errors };
}
