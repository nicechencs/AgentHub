import type { Account, AgentId, Provider } from '@/lib/types';

export type ConnectionTrashKind = 'account' | 'provider';

/** A deleted connection retained for 30 days and available for restore. */
export interface ConnectionTrashItem {
  id: string;
  agentId: AgentId;
  kind: ConnectionTrashKind;
  sourceId: string;
  label: string;
  wasCurrent: boolean;
  deletedAt: string;
  expiresAt: string;
  /** Redacted account/provider payload returned by the backend. */
  account?: Account;
  provider?: Provider;
}

export interface TrashPort {
  list(agentId?: AgentId): Promise<ConnectionTrashItem[]>;
  restore(id: string): Promise<void>;
  permanentlyDelete(id: string): Promise<void>;
}
