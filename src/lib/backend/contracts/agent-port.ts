import type { AgentId, AgentStatus, AgentUpdateInfo } from '@/lib/types';
import type { InstallOutcome } from './install-types';

export interface AgentPort {
  listAgents(): Promise<AgentStatus[]>;
  getAgent(agentId: AgentId): Promise<AgentStatus>;
  installAgent(
    agentId: AgentId,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<AgentStatus>;
  installAgentDetailed(
    agentId: AgentId,
    channel: string,
    opts?: { installDeps?: boolean },
  ): Promise<InstallOutcome>;
  upgradeAgent(agentId: AgentId): Promise<AgentStatus>;
  upgradeAgentDetailed(agentId: AgentId): Promise<InstallOutcome>;
  uninstallAgent(agentId: AgentId, deleteConfig: boolean): Promise<void>;
  uninstallAgentDetailed(agentId: AgentId, deleteConfig: boolean): Promise<InstallOutcome>;
  openAgentConfig(agentId: AgentId): Promise<string | null>;
  /**
   * Probe remote latest for agents (npm registry, cached).
   * Empty / omitted agentIds → all agents. force bypasses TTL cache.
   */
  checkAgentUpdates(agentIds?: AgentId[], force?: boolean): Promise<AgentUpdateInfo[]>;
  /** Soft-hide / unhide. Does not uninstall or delete credentials. */
  setAgentHidden(agentId: AgentId, hidden: boolean): Promise<void>;
}
