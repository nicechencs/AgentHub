import { AGENT_MAP, AGENTS } from '@/config/agents';
import { enrichStatusesWithConnections } from '@/lib/api/agent-connection';
import type { Backend, AgentPort } from '@/lib/backend/contracts';
import { mergeAgentListWithCatalog } from '@/lib/backend/contracts/agent-catalog';
import {
  EnvNotReadyError,
  InstallFailedError,
} from '@/lib/backend/contracts/agent-errors';
import { logger } from '@/lib/logger';
import type { AgentId, AgentStatus, AgentUpdateInfo } from '@/lib/types';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:agent');

export { EnvNotReadyError, InstallFailedError, mergeAgentListWithCatalog };

function missingAgentStatus(id: AgentId): AgentStatus {
  return {
    agentId: id,
    installed: false,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    envReady: true,
  };
}

async function withConnectionEnrichment(
  backend: Backend,
  agents: AgentStatus[],
): Promise<AgentStatus[]> {
  const installed = agents.filter((a) => a.installed).length;
  try {
    const [accounts, providers] = await Promise.all([
      backend.account.listAccounts(),
      backend.provider.listProviders(),
    ]);
    log.debug('connection enrichment load pools', {
      agents: agents.length,
      installed,
      accounts: accounts.length,
      providers: providers.length,
    });
    return enrichStatusesWithConnections(agents, accounts, providers);
  } catch (e) {
    log.warn('connection enrichment failed; showing detect-only status', {
      agents: agents.length,
      installed,
      error: e instanceof Error ? e.message : String(e),
    });
    return enrichStatusesWithConnections(agents, [], []);
  }
}

export function createTauriAgentPort(backend: Backend): AgentPort {
  return {
    async listAgents() {
      const doctor = await backend.doctor.loadDoctorMapped();
      return withConnectionEnrichment(
        backend,
        // AGENTS is catalog-driven (boot / mock seed); empty → detected-only.
        mergeAgentListWithCatalog(doctor.agents, AGENTS),
      );
    },

    async getAgent(agentId) {
      const doctor = await backend.doctor.loadDoctorMapped();
      const found = doctor.agents.find((a) => a.agentId === agentId);
      const inCatalog = Boolean(AGENT_MAP[agentId]);
      const base = found
        ? found
        : inCatalog
          ? missingAgentStatus(agentId)
          : null;
      if (!base) throw new Error(`未知 agent: ${agentId}`);
      if (!base.capabilities && AGENT_MAP[agentId]?.capabilities) {
        base.capabilities = AGENT_MAP[agentId].capabilities;
      }
      const [enriched] = await withConnectionEnrichment(backend, [base]);
      return enriched!;
    },

    async installAgentDetailed(agentId, channel, opts = {}) {
      const outcome = await backend.install.installAgentCmd(
        agentId,
        channel,
        opts.installDeps ?? false,
      );
      await backend.doctor.refreshDoctor();
      return outcome;
    },

    async installAgent(agentId, channel, opts = {}) {
      try {
        const outcome = await backend.install.installAgentCmd(
          agentId,
          channel,
          opts.installDeps ?? false,
        );
        if (!outcome.ok) throw new InstallFailedError(outcome);
        await backend.doctor.refreshDoctor();
        return this.getAgent(agentId);
      } catch (e) {
        log.error('installAgent failed', { agentId, channel }, e);
        throw e;
      }
    },

    async upgradeAgentDetailed(agentId) {
      const outcome = await backend.install.upgradeAgentCmd(agentId);
      await backend.doctor.refreshDoctor();
      return outcome;
    },

    async upgradeAgent(agentId) {
      try {
        const outcome = await backend.install.upgradeAgentCmd(agentId);
        if (!outcome.ok) throw new InstallFailedError(outcome);
        await backend.doctor.refreshDoctor();
        return this.getAgent(agentId);
      } catch (e) {
        log.error('upgradeAgent failed', { agentId }, e);
        throw e;
      }
    },

    async uninstallAgentDetailed(agentId, deleteConfig) {
      const outcome = await backend.install.uninstallAgentCmd(agentId, deleteConfig);
      await backend.doctor.refreshDoctor();
      return outcome;
    },

    async uninstallAgent(agentId, deleteConfig) {
      try {
        const outcome = await backend.install.uninstallAgentCmd(agentId, deleteConfig);
        if (!outcome.ok) throw new InstallFailedError(outcome);
        await backend.doctor.refreshDoctor();
      } catch (e) {
        log.error('uninstallAgent failed', { agentId, deleteConfig }, e);
        throw e;
      }
    },

    async openAgentConfig(agentId) {
      try {
        return await backend.install.openAgentConfigDir(agentId);
      } catch (e) {
        log.error('openAgentConfig failed', { agentId }, e);
        throw e;
      }
    },

    async checkAgentUpdates(agentIds, force = false) {
      try {
        return await invoke<AgentUpdateInfo[]>('check_agent_updates', {
          agentIds: agentIds?.length ? agentIds : null,
          force,
        });
      } catch (e) {
        log.error('checkAgentUpdates failed', { agentIds, force }, e);
        throw e;
      }
    },
  };
}
