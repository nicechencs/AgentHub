/**
 * mock adapter helpers. Not a route planner: product decisions come from golden.
 */
import {
  type AdapterAction,
  type AdapterBridgeRuntimeStatus,
  type AdapterEvidence,
  type AdapterPlanChange,
  type AdapterProfile,
  type DefaultRoutePoolOverview,
  type SourceModelCatalog,
} from '@/lib/backend/contracts/adapter';
import type { Account, Provider } from '@/lib/types';
import {
  jsonString,
  type ClassifiableAccount,
} from '../source-classify';

export type { ClassifiableAccount };
export {
  DEEPSEEK_API_ENDPOINT_NEEDLE,
  GLM_CODING_ANTHROPIC_NEEDLE,
  GLM_CODING_CHAT_NEEDLE,
  GLM_CODING_RESPONSES_NEEDLE,
  KIMI_CODING_ENDPOINT_NEEDLE,
  KIMI_MEMBERSHIP_PRESET,
  OPENAI_API_ENDPOINT_NEEDLE,
  XAI_API_ENDPOINT_NEEDLE,
  isCodexAuthJson,
  isKimiMembershipAccount,
  isKimiMembershipProvider,
  textLooksLikeKimiCoding,
} from '../source-classify';
export { jsonString };

const verifiedAt = '2026-08-12';

export interface MockAdapterState {
  profiles: AdapterProfile[];
  bridgeStatuses: Map<string, AdapterBridgeRuntimeStatus>;
  generatedProviders: Map<string, Provider>;
  resolver: MockAdapterSourceResolver;
  removeGeneratedProvider?: (provider: Provider) => void;
  routePoolV2: boolean;
  shareChatCompletions: boolean;
  defaultPools: DefaultRoutePoolOverview[];
  localTokens: Map<string, string>;
  localTokenNames: Map<string, string>;
  extraLocalTokens: Array<{ id: string; poolId: string; name: string; token: string }>;
  hiddenPrimaryIds: Set<string>;
  localGatewayRunning: boolean;
  localGatewayPort: number | null;
  sourceModelCatalogs: Map<string, SourceModelCatalog>;
}

export interface MockAdapterSourceResolver {
  getAccountById(id: string): Account | undefined;
  getProviderById(id: string): Provider | undefined;
  upsertGeneratedProvider?(provider: Provider): Provider;
  removeGeneratedProvider?(provider: Provider): void;
}

export function evidence(label: string, url: string): AdapterEvidence {
  return { label, url, verifiedAt };
}

export function action(
  kind: AdapterAction['kind'],
  target: string,
  description: string,
  value?: string,
): AdapterAction {
  return { kind, target, description, value, secret: false };
}

export function secretAction(target: string, description: string): AdapterAction {
  return { kind: 'reference_connection_secret', target, description, secret: true };
}

export function change(target: string, field: string, value?: string): AdapterPlanChange {
  return { target, field, value, secret: false };
}

export function secretChange(target: string, field: string): AdapterPlanChange {
  return { target, field, secret: true };
}

export function hasAccountApiKey(account: ClassifiableAccount | undefined): boolean {
  if (!account || account.kind !== 'apikey') return false;
  const credentials = account.credentials;
  return !!credentials
    && jsonString(credentials, 'format')?.toLowerCase() === 'api_key'
    && !!jsonString(credentials, 'api_key');
}
