/**
 * Read-model slices of the wide AgentStatus wire row.
 *
 * Omitted optional fields become `unknown` / `unset`. This mapper never
 * infers 未安装 / 未登录 / 没有连接 / 环境已就绪, never reads doctor
 * placeholder `authStatus: 'none'`, and never looks up catalog.
 */
import type { AgentCapabilities } from '@/lib/capability';
import type { AgentStatus, EffectiveConnectionKind, RuntimeId } from '@/lib/types';
import { normalizeAuthHealth, type AuthHealth } from './auth-state';

export type HiddenSlice = 'unknown' | 'visible' | 'hidden';
export type UnsetOr<T> = T | 'unset';
export type UnknownOr<T> = T | 'unknown';

export interface AgentStatusView {
  hidden: HiddenSlice;
  liveAuth: {
    health: UnsetOr<AuthHealth>;
    label: UnsetOr<string>;
    source: UnsetOr<string>;
    revision: UnsetOr<string>;
  };
  effectiveConnection: {
    kind: UnsetOr<EffectiveConnectionKind>;
    label: UnsetOr<string>;
    currentProvider: UnsetOr<string>;
  };
  env: {
    ready: UnknownOr<boolean>;
    missing: UnsetOr<RuntimeId[]>;
  };
  capabilities: UnknownOr<AgentCapabilities>;
}

/** Tests feed partial objects; production rows are full AgentStatus. */
export function sliceAgentStatus(status: Partial<AgentStatus>): AgentStatusView {
  return {
    hidden: hiddenSlice(status.hidden),
    liveAuth: {
      health: optionalHealth(status.authHealth),
      label: optionalString(status.authHealthLabel),
      source: optionalString(status.authSource),
      revision: optionalString(status.authRevision),
    },
    effectiveConnection: {
      kind: optionalKind(status.effectiveKind),
      label: optionalString(status.effectiveLabel),
      currentProvider: optionalString(status.currentProvider),
    },
    env: {
      ready: optionalEnvReady(status.envReady),
      missing: status.envMissing === undefined ? 'unset' : status.envMissing,
    },
    capabilities: optionalCapabilities(status.capabilities),
  };
}

function hiddenSlice(hidden: boolean | undefined): HiddenSlice {
  if (hidden === undefined) return 'unknown';
  return hidden ? 'hidden' : 'visible';
}

function optionalHealth(value: unknown): UnsetOr<AuthHealth> {
  return normalizeAuthHealth(value) ?? 'unset';
}

function optionalString(value: string | undefined): UnsetOr<string> {
  return value === undefined ? 'unset' : value;
}

function optionalKind(
  value: EffectiveConnectionKind | undefined,
): UnsetOr<EffectiveConnectionKind> {
  return value === undefined ? 'unset' : value;
}

function optionalEnvReady(value: boolean | undefined): UnknownOr<boolean> {
  return value === undefined ? 'unknown' : value;
}

function optionalCapabilities(
  value: AgentCapabilities | undefined,
): UnknownOr<AgentCapabilities> {
  return value === undefined ? 'unknown' : value;
}
