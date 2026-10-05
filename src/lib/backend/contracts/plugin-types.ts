import type { AgentKey } from '@/lib/types';

export interface PluginComponent {
  kind: string;
  name: string;
  description?: string | null;
}

export interface PluginEntry {
  id: string;
  agent: AgentKey;
  name: string;
  marketplace?: string | null;
  version?: string | null;
  /** Pi configured npm selector or git ref. Only an exact npm semver is a pin. */
  requestedVersion?: string | null;
  scope?: string | null;
  enabled?: boolean | null;
  trusted?: boolean | null;
  path?: string | null;
  description?: string | null;
  /** Exact vendor CLI source used to install/remove this pack. Required for Pi removal. */
  installSource?: string | null;
  /** cli | live */
  source: string;
  components: PluginComponent[];
}

export interface PluginAgentStatus {
  agent: AgentKey;
  /** listed | planned | unsupported */
  support: string;
  source?: string | null;
  errorCode?: string | null;
  error?: string | null;
  pluginCount: number;
}

export interface PluginSourceFile {
  agent: AgentKey;
  path: string;
  exists: boolean;
  readable: boolean;
  error?: string | null;
  /** plugin-tree | config | skills | mcp | cordis */
  sourceKind: string;
  itemCount: number;
  label: string;
}

export interface PluginInventory {
  agents: PluginAgentStatus[];
  plugins: PluginEntry[];
  sources?: PluginSourceFile[];
}

export interface PluginInstallOptions {
  confirmed: boolean;
}

export interface PluginUninstallOptions {
  /** Default true where the vendor exposes separate plugin-data removal. */
  keepData: boolean;
}

export interface PluginUpdateOptions {
  confirmed: boolean;
}

/** Fixed vendor operations exposed by the Plugins page; this is not a plugin SDK. */
export type PluginMutationAction =
  | 'install'
  | 'uninstall'
  | 'enable'
  | 'disable'
  | 'marketplaceRefresh'
  | 'update'
  | 'piUpdate';

export interface PluginMutationTarget {
  agent: AgentKey;
  name: string;
  marketplace?: string | null;
  installSource?: string | null;
}

export interface PluginMutationReinventory {
  /** target | marketplace | agent. Pi updates intentionally use `agent`. */
  scope: 'target' | 'marketplace' | 'agent';
  /** Actual rows scanned for this outcome, never guessed affected packages. */
  scannedPluginIds: string[];
  /** Present only after the marketplace catalog was re-scanned. */
  marketplaceEntries?: PluginEntry[] | null;
}

export type PluginMutationUnconfirmedReason =
  | 'inventoryUnavailable'
  | 'ambiguousTarget'
  | 'targetNotListed'
  | 'targetStillListed'
  | 'enabledStateMismatch'
  | 'versionUnchangedOrUnknown'
  | 'marketplaceSnapshotUnavailable'
  | 'marketplaceEntriesUnchanged'
  | 'piScopeUnchanged';

/**
 * An official command may exit successfully without producing provable local
 * state. UI must only present success for the `confirmed` branch.
 */
export type PluginMutationOutcome =
  | {
      status: 'confirmed';
      action: PluginMutationAction;
      agent: AgentKey;
      target?: PluginMutationTarget | null;
      inventory: PluginInventory;
      reinventory: PluginMutationReinventory;
    }
  | {
      status: 'unconfirmed';
      action: PluginMutationAction;
      agent: AgentKey;
      target?: PluginMutationTarget | null;
      reason: PluginMutationUnconfirmedReason;
      inventory: PluginInventory;
      reinventory: PluginMutationReinventory;
    };

export interface PluginPort {
  listInventory(): Promise<PluginInventory>;
  listAvailable(agent: AgentKey): Promise<PluginEntry[]>;
  previewInstall(agent: AgentKey, source: string): Promise<PluginEntry>;
  install(
    agent: AgentKey,
    source: string,
    options: PluginInstallOptions,
  ): Promise<PluginMutationOutcome>;
  uninstall(
    agent: AgentKey,
    name: string,
    marketplace: string | null | undefined,
    installSource: string | null | undefined,
    options: PluginUninstallOptions,
  ): Promise<PluginMutationOutcome>;
  enable(
    agent: AgentKey,
    name: string,
    marketplace?: string | null,
  ): Promise<PluginMutationOutcome>;
  disable(
    agent: AgentKey,
    name: string,
    marketplace?: string | null,
  ): Promise<PluginMutationOutcome>;
  refreshMarketplace(agent: AgentKey): Promise<PluginMutationOutcome>;
  update(
    agent: AgentKey,
    name: string,
    marketplace: string | null | undefined,
    scope: string | null | undefined,
    options: PluginUpdateOptions,
  ): Promise<PluginMutationOutcome>;
  updatePi(options: PluginUpdateOptions): Promise<PluginMutationOutcome>;
}
