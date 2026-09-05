/**
 * Write a Sub2API gateway key into Connections as a custom upstream Provider.
 * Does NOT set `home: route_pool` — stays on the Connections list.
 */
import {
  getAgentConfigSchema,
  materializeAgentConfig,
  validateAgentConfig,
} from '@/lib/api/config';
import { runProviderSaveFlow } from '@/lib/api/provider-save';
import { upsertProvider } from '@/lib/api/provider';
import { applyFormVars } from '@/lib/provider-detect/fields';
import { defaultConfigScaffold } from '@/lib/provider-detect/scaffold';
import { EMPTY_FORM_VARS } from '@/lib/provider-detect/types';
import type { Provider } from '@/lib/types';

export type SyncSub2ApiKeyInput = {
  gatewayBaseUrl: string;
  apiKey: string;
  name: string;
  agentId?: string;
};

export type SyncSub2ApiKeyResult =
  | { ok: true; provider: Provider }
  | { ok: false; message: string };

export async function syncSub2ApiKeyToConnections(
  input: SyncSub2ApiKeyInput,
): Promise<SyncSub2ApiKeyResult> {
  const apiKey = input.apiKey.trim();
  if (!apiKey) return { ok: false, message: 'Missing API Key' };
  const agentId = (input.agentId || 'claude').trim() || 'claude';
  const baseUrl = input.gatewayBaseUrl.replace(/\/$/, '');
  const scaffold = defaultConfigScaffold(agentId);
  const vars = { ...EMPTY_FORM_VARS, baseUrl, apiKey };
  let configSchema = null;
  let schemaStatus: 'ready' | 'unsupported' = 'unsupported';
  try {
    configSchema = await getAgentConfigSchema(agentId);
    schemaStatus = 'ready';
  } catch {
    schemaStatus = 'unsupported';
  }
  const id = `p-sub2api-${Date.now()}`;
  const result = await runProviderSaveFlow(
    {
      agentId,
      schemaStatus,
      configSchema,
      isEdit: false,
      id,
      name: input.name.trim() || 'Sub2API',
      useOfficial: false,
      configText: scaffold.text,
      configFormat: scaffold.format,
      vars,
      saveVars: vars,
      finalFormat: scaffold.format,
      baseText: scaffold.text,
    },
    {
      validateAgentConfig,
      materializeAgentConfig,
      applyFormVars,
      upsertProvider,
    },
  );
  if (!result.ok) {
    return { ok: false, message: result.message };
  }
  if (result.provider.home === 'route_pool') {
    const fixed = await upsertProvider({ ...result.provider, home: undefined });
    return { ok: true, provider: fixed };
  }
  return { ok: true, provider: result.provider };
}
