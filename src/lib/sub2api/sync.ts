/**
 * Map a Sub2API key into Connections「添加 API Key」draft / navigation.
 */
import {
  buildConnectionsGuideUrl,
  connectApiKeyDraftState,
  type ConnectApiKeyDraft,
} from '@/lib/connect-flow/connect-intent';
import type { AgentKey } from '@/lib/types';
import type { Sub2ApiKey } from './types';

export function sub2apiKeyToConnectDraft(
  gatewayBaseUrl: string,
  key: Pick<Sub2ApiKey, 'key' | 'name'>,
): ConnectApiKeyDraft {
  return {
    baseUrl: gatewayBaseUrl.replace(/\/$/, ''),
    apiKey: key.key,
  };
}

export function sub2apiSyncNavigation(
  agentId: AgentKey,
  gatewayBaseUrl: string,
  key: Pick<Sub2ApiKey, 'key' | 'name'>,
): { pathname: string; state: ReturnType<typeof connectApiKeyDraftState> } {
  return {
    pathname: buildConnectionsGuideUrl({ agentId, intent: 'add-key' }),
    state: connectApiKeyDraftState(sub2apiKeyToConnectDraft(gatewayBaseUrl, key)),
  };
}
