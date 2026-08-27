/**
 * Agent API façade — delegates to app runtime backend.
 */
import {
  applyAgentHidden,
  getAgentStatusSnapshot,
  getBackend,
  loadAgentStatuses,
  refreshRuntimeReadModels,
  revertAgentHidden,
} from '@/app/runtime';
import type { InstallOutcome } from '@/lib/backend/contracts/install-types';
import type { AgentId, AgentStatus, AgentUpdateInfo } from '@/lib/types';

export {
  EnvNotReadyError,
  InstallFailedError,
} from '@/lib/backend/contracts/agent-errors';

export { mergeAgentListWithCatalog } from '@/lib/backend/contracts/agent-catalog';

export async function listAgents(opts: { force?: boolean } = {}): Promise<AgentStatus[]> {
  const snapshot = await loadAgentStatuses(getBackend(), opts);
  return snapshot.statuses;
}

async function refreshAgentStatusStore(): Promise<void> {
  try {
    await refreshRuntimeReadModels(getBackend(), { models: ['agentStatus'] });
  } catch {
    // The mutation result remains authoritative; refresh errors stay on the status snapshot.
  }
}

export async function getAgent(agentId: AgentId): Promise<AgentStatus> {
  return getBackend().agent.getAgent(agentId);
}

export async function installAgentDetailed(
  agentId: AgentId,
  channel: string,
  opts: { installDeps?: boolean } = {},
): Promise<InstallOutcome> {
  const outcome = await getBackend().agent.installAgentDetailed(agentId, channel, opts);
  await refreshAgentStatusStore();
  return outcome;
}

export async function installAgent(
  agentId: AgentId,
  channel: string,
  opts: { installDeps?: boolean } = {},
): Promise<AgentStatus> {
  const outcome = await getBackend().agent.installAgent(agentId, channel, opts);
  await refreshAgentStatusStore();
  return outcome;
}

export async function upgradeAgentDetailed(agentId: AgentId): Promise<InstallOutcome> {
  const outcome = await getBackend().agent.upgradeAgentDetailed(agentId);
  await refreshAgentStatusStore();
  return outcome;
}

export async function upgradeAgent(agentId: AgentId): Promise<AgentStatus> {
  const outcome = await getBackend().agent.upgradeAgent(agentId);
  await refreshAgentStatusStore();
  return outcome;
}

export async function uninstallAgentDetailed(
  agentId: AgentId,
  deleteConfig: boolean,
): Promise<InstallOutcome> {
  const outcome = await getBackend().agent.uninstallAgentDetailed(agentId, deleteConfig);
  await refreshAgentStatusStore();
  return outcome;
}

export async function uninstallAgent(agentId: AgentId, deleteConfig: boolean): Promise<void> {
  await getBackend().agent.uninstallAgent(agentId, deleteConfig);
  await refreshAgentStatusStore();
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

export async function setAgentHidden(agentId: AgentId, hidden: boolean): Promise<void> {
  const previous = Boolean(
    getAgentStatusSnapshot().statuses.find((row) => row.agentId === agentId)?.hidden,
  );
  applyAgentHidden(agentId, hidden);
  try {
    await getBackend().agent.setAgentHidden(agentId, hidden);
  } catch (error) {
    revertAgentHidden(agentId, previous);
    throw error;
  }
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
