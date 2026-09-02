import type { Account, AgentId, Provider } from '@/lib/types';

export type ConnectionTrashKind = 'account' | 'provider' | 'membership';
export type ConnectionTrashHome = 'connections' | 'route_pool';

export interface RouteMembershipTrashMember {
  routePoolId: string;
  enabled: boolean;
  priority: number;
  position: number;
}

export interface RouteMembershipTrashPayload {
  sourceKind: 'account' | 'provider';
  sourceId: string;
  members: RouteMembershipTrashMember[];
}

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
  home?: ConnectionTrashHome;
  /** Redacted account/provider payload returned by the backend. */
  account?: Account;
  provider?: Provider;
  membership?: RouteMembershipTrashPayload;
}

export interface TrashPort {
  list(agentId?: AgentId, home?: ConnectionTrashHome): Promise<ConnectionTrashItem[]>;
  restore(id: string): Promise<void>;
  permanentlyDelete(id: string): Promise<void>;
}
