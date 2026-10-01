import type { OAuthLoginOption } from '@/lib/backend/contracts/account-port';
import type { AuthHealth } from '@/lib/backend/contracts/auth-state';
import type {
  OfficialLoginPoll,
  OfficialLoginSession,
} from '@/lib/backend/contracts/official-login-session';
import type { ConnectionKind } from '@/lib/connection-kind';
import type { AgentKey, AuthStatus } from '@/lib/types';

/** Which saved login a page already knows. No secret and no pool membership. */
export type LoginSourceRef = {
  sourceKind: 'account' | 'provider';
  sourceId: string;
  agentId: AgentKey;
};

export type LoginIdentityInput = LoginSourceRef & {
  kind: ConnectionKind;
  label: string;
  identityLabel?: string;
  endpointHost?: string;
  endpointMode?: 'official' | 'custom';
};

export type LoginStatusInput = {
  authHealth?: AuthHealth;
  authStatus?: AuthStatus;
  credentialKind: ConnectionKind;
  /** Empty model list on this login / pool. */
  catalogEmpty?: boolean;
  /** Same source is sitting in the trash. */
  inTrash?: boolean;
  /** Pool member is enabled but not healthy. */
  memberUnhealthy?: boolean;
};

export type LoginPresentation = {
  identity: { primary: string; secondary?: string };
  status: {
    label: string;
    tone: 'success' | 'warning' | 'danger' | 'muted';
  };
  kind: ConnectionKind;
};

/** Controlled API Key field values. Empty secret means the caller keeps the stored key. */
export type ApiKeyFieldValue = {
  secret: string;
  endpoint: string;
};

export type LoginSaveResult = {
  kind: ConnectionKind;
  mutation: 'created' | 'updated';
  source: LoginSourceRef;
};

export type LoginBatchSaveResult = {
  saved: LoginSaveResult[];
  errors: string[];
};

/**
 * Page-owned official login save.
 * Connections and the route pool pass different adapters; the flow never sees pool ownership.
 */
export type OfficialLoginPersistence = {
  start: (
    agentId: AgentKey,
    option: OAuthLoginOption,
  ) => Promise<OfficialLoginSession>;
  finish: (
    session: OfficialLoginSession,
  ) => Promise<LoginSaveResult>;
  cancel: (
    session: OfficialLoginSession,
  ) => Promise<void>;
};

/** Read-only option and wait calls. The kernel does not import the API façade. */
export type OfficialLoginDiscovery = {
  supported: (agentId: AgentKey) => Promise<boolean>;
  listOptions: (agentId: AgentKey) => Promise<OAuthLoginOption[]>;
  wait: (
    session: OfficialLoginSession,
    isCurrent: () => boolean,
  ) => Promise<OfficialLoginPoll>;
};
