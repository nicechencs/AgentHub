import type { ConnectionKind } from '@/lib/connection-kind';
import type { AgentKey } from '@/lib/types';
import type { LoginBatchSaveResult, LoginSaveResult } from './types';

export function toLoginSaveResult(input: {
  kind: ConnectionKind;
  mutation: 'created' | 'updated';
  sourceKind: 'account' | 'provider';
  sourceId: string;
  agentId: AgentKey;
}): LoginSaveResult {
  return {
    kind: input.kind,
    mutation: input.mutation,
    source: {
      sourceKind: input.sourceKind,
      sourceId: input.sourceId,
      agentId: input.agentId,
    },
  };
}

/** One saved login per record. Each record stays separate and omits pool-only fields. */
export function toLoginBatchSaveResult(input: {
  records: ReadonlyArray<{
    kind: ConnectionKind;
    mutation: 'created' | 'updated';
    sourceKind: 'account' | 'provider';
    sourceId: string;
    agentId: AgentKey;
  }>;
  errors: readonly string[];
}): LoginBatchSaveResult {
  return {
    saved: input.records.map((record) => toLoginSaveResult(record)),
    errors: [...input.errors],
  };
}
