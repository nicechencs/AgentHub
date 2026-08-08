/**
 * Agent API façade — delegates to app runtime backend.
 */
import { getBackend } from '@/app/runtime';
import type { InstallOutcome } from '@/lib/backend/contracts/install-types';
import type { AgentId, AgentStatus, AgentUpdateInfo } from '@/lib/types';

export {
  EnvNotReadyError,
  InstallFailedError,
} from '@/lib/backend/contracts/agent-errors';

export { mergeAgentListWithCatalog } from '@/lib/backend/contracts/agent-catalog';

export async function listAgents(): Promise<AgentStatus[]> {
  return getBackend().agent.listAgents();
}

export async function getAgent(agentId: AgentId): Promise<AgentStatus> {
  return getBackend().agent.getAgent(agentId);
}

export async function installAgentDetailed(
  agentId: AgentId,
  channel: string,
  opts: { installDeps?: boolean } = {},
): Promise<InstallOutcome> {
  return getBackend().agent.installAgentDetailed(agentId, channel, opts);
}

export async function installAgent(
  agentId: AgentId,
  channel: string,
  opts: { installDeps?: boolean } = {},
): Promise<AgentStatus> {
  return getBackend().agent.installAgent(agentId, channel, opts);
}

export async function upgradeAgentDetailed(agentId: AgentId): Promise<InstallOutcome> {
  return getBackend().agent.upgradeAgentDetailed(agentId);
}

export async function upgradeAgent(agentId: AgentId): Promise<AgentStatus> {
  return getBackend().agent.upgradeAgent(agentId);
}

export async function uninstallAgentDetailed(
  agentId: AgentId,
  deleteConfig: boolean,
): Promise<InstallOutcome> {
  return getBackend().agent.uninstallAgentDetailed(agentId, deleteConfig);
}

export async function uninstallAgent(agentId: AgentId, deleteConfig: boolean): Promise<void> {
  return getBackend().agent.uninstallAgent(agentId, deleteConfig);
}

export async function openAgentConfig(agentId: AgentId): Promise<string | null> {
  return getBackend().agent.openAgentConfig(agentId);
}

export async function checkAgentUpdates(
  agentIds?: AgentId[],
  force = false,
): Promise<AgentUpdateInfo[]> {
  return getBackend().agent.checkAgentUpdates(agentIds, force);
}

/** Merge update probe rows onto agent status list (by agentId). */
export function applyAgentUpdates(
  agents: AgentStatus[],
  updates: AgentUpdateInfo[],
): AgentStatus[] {
  const map = new Map(updates.map((u) => [u.agentId, u]));
  return agents.map((a) => {
    const u = map.get(a.agentId);
    if (!u) return a;
    return {
      ...a,
      latestVersion: u.latestVersion ?? a.latestVersion,
      update: u,
    };
  });
}
