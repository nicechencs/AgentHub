/**
 * Connections preference: auto-import this computer's login (default on).
 * Manual import UI is shown only when the preference is off.
 */
import type { AgentKey } from '@/lib/types';
import {
  liveApiKeyImportGate,
  liveAuthDiscoveryKind,
  liveAuthImportGate,
  liveImportDialogMode,
  type ConnectionInventoryDiscoveryState,
  type DiscoveredAuthKind,
  type DiscoveryProviderLike,
  type LiveAuthProbeLike,
} from './connection-model';

export function resolveAutoImportLocalLogin(value?: boolean | null): boolean {
  return value !== false;
}

export function showConnectionsImportLoginAction(autoImportLocalLogin?: boolean | null): boolean {
  return !resolveAutoImportLocalLogin(autoImportLocalLogin);
}

export function shouldAutoImportDiscoveredLogin(
  autoImportLocalLogin: boolean | null | undefined,
  discoveryKind: DiscoveredAuthKind | null,
): boolean {
  return resolveAutoImportLocalLogin(autoImportLocalLogin) && discoveryKind !== null;
}

export function planLocalLoginAutoImport(input: {
  autoImportLocalLogin?: boolean | null;
  agentIds: readonly AgentKey[];
  alreadyTried: ReadonlySet<string>;
}): AgentKey[] {
  if (!resolveAutoImportLocalLogin(input.autoImportLocalLogin)) return [];
  return input.agentIds.filter((id) => !input.alreadyTried.has(id));
}

export function canAutoImportProbe(input: {
  agentId: AgentKey;
  poolState: ConnectionInventoryDiscoveryState;
  probe?: LiveAuthProbeLike | null;
  accounts: readonly { kind: string; secretHash?: string | null }[];
  providers: readonly DiscoveryProviderLike[];
  accountsFailed?: boolean;
  providersFailed?: boolean;
}): boolean {
  const discoveryKind = liveAuthDiscoveryKind({
    poolState: input.poolState,
    probe: input.probe,
    accounts: input.accounts,
    providers: input.providers,
    accountsFailed: input.accountsFailed,
    providersFailed: input.providersFailed,
  });
  if (!shouldAutoImportDiscoveredLogin(true, discoveryKind)) return false;
  const mode = liveImportDialogMode(input.probe);
  const gate = mode === 'api-key'
    ? liveApiKeyImportGate(input.probe, false, input.agentId)
    : liveAuthImportGate(input.probe, false, input.agentId);
  return gate.enabled;
}
