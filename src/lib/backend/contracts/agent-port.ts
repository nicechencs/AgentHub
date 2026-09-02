import type { AgentKey, AgentStatus, AgentUpdateInfo } from '@/lib/types';
import type { InstallOutcome } from './install-types';

export interface AgentPort {
  listAgents(): Promise<AgentStatus[]>;
  getAgent(agentId: AgentKey): Promise<AgentStatus>;
  installAgent(
    agentId: AgentKey,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<AgentStatus>;
  installAgentDetailed(
    agentId: AgentKey,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<InstallOutcome>;
  upgradeAgent(agentId: AgentKey): Promise<AgentStatus>;
  upgradeAgentDetailed(agentId: AgentKey): Promise<InstallOutcome>;
  uninstallAgent(agentId: AgentKey, deleteConfig: boolean): Promise<void>;
  uninstallAgentDetailed(agentId: AgentKey, deleteConfig: boolean): Promise<InstallOutcome>;
  openAgentConfig(agentId: AgentKey): Promise<string | null>;
  /**
   * Probe remote latest for agents (npm registry, cached).
   * Empty / omitted agentIds → all agents. force bypasses TTL cache.
   */
  checkAgentUpdates(agentIds?: AgentKey[], force?: boolean): Promise<AgentUpdateInfo[]>;
  /** Soft-hide / unhide. Does not uninstall or delete credentials. */
  setAgentHidden(agentId: AgentKey, hidden: boolean): Promise<void>;
}
