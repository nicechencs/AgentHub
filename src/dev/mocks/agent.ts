import { AGENTS, AGENT_MAP, agentDisplayName } from '@/config/agents';
import { enrichStatusesWithConnections } from '@/lib/api/agent-connection';
import type { Backend, AgentPort } from '@/lib/backend/contracts';
import { mergeAgentListWithCatalog } from '@/lib/backend/contracts/agent-catalog';
import {
  EnvNotReadyError,
  InstallFailedError,
} from '@/lib/backend/contracts/agent-errors';
import { unavailableError } from '@/lib/backend/contracts/errors';
import { delay, randomLatency } from '@/dev/mocks/delay';
import { checkChannelEnv, defaultChannel, findChannel } from '@/lib/env';
import { resolveAutoInstallPlan } from '@/lib/env-plan';
import { logger } from '@/lib/logger';
import type {
  AgentId,
  AgentStatus,
  AgentUpdateInfo,
  AgentUpdateState,
  InstallChannel,
} from '@/lib/types';
import { MOCK_CAPABILITIES } from './capabilities';

const log = logger.scope('dev:mock:agent');

export { EnvNotReadyError, InstallFailedError, mergeAgentListWithCatalog };

/**
 * Demo seeds so `pnpm dev:mock` can exercise update button colors without network:
 * - claude: npm installed, outdated → green
 * - codex: npm installed, up to date → gray force
 * - others: not installed until user installs
 */
function defaultMockAgentStatuses(): Record<AgentId, AgentStatus> {
  return {
    claude: {
      agentId: 'claude',
      installed: true,
      version: '1.0.0',
      latestVersion: '1.2.0',
      channel: 'npm',
      binPath: '~/AppData/Roaming/npm/claude.cmd',
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.claude,
    },
    codex: {
      agentId: 'codex',
      installed: true,
      version: '0.50.0',
      latestVersion: '0.50.0',
      channel: 'npm',
      binPath: '~/AppData/Roaming/npm/codex.cmd',
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.codex,
    },
    kimi: {
      agentId: 'kimi',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.kimi,
    },
    grok: {
      agentId: 'grok',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.grok,
    },
    pi: {
      agentId: 'pi',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.pi,
    },
    workbuddy: {
      agentId: 'workbuddy',
      installed: true,
      version: '5.3.8',
      channel: 'native',
      binPath: '~/AppData/Local/Programs/WorkBuddy/WorkBuddy.exe',
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.workbuddy,
    },
    cursor: {
      agentId: 'cursor',
      installed: false,
      authStatus: 'none',
      authLabel: '未配置',
      running: false,
      capabilities: MOCK_CAPABILITIES.cursor,
    },
  };
}

const state: Record<AgentId, AgentStatus> = defaultMockAgentStatuses();

/** Restore default install flags so opt-in ConnectFlow seeds do not leak across tests. */
export function resetMockAgentStatuses(): void {
  const defaults = defaultMockAgentStatuses();
  (Object.keys(defaults) as AgentId[]).forEach((id) => {
    const next = defaults[id];
    const current = state[id] ?? (state[id] = next);
    current.installed = next.installed;
    current.version = next.version;
    current.latestVersion = next.latestVersion;
    current.channel = next.channel;
    current.binPath = next.binPath;
    current.authStatus = next.authStatus;
    current.authLabel = next.authLabel;
    current.running = next.running;
    current.currentProvider = next.currentProvider;
    current.capabilities = next.capabilities;
  });
}

/** Test / opt-in fixture helper. Does not run from createBackend(). */
export function markMockAgentInstalled(agentId: AgentId, installed = true): void {
  const current = state[agentId] ?? (state[agentId] = missingAgentStatus(agentId));
  current.installed = installed;
  if (installed) {
    current.version = current.version ?? '1.0.0';
    current.channel = current.channel ?? 'npm';
    current.binPath = current.binPath ?? `~/AppData/Roaming/npm/${agentId}.cmd`;
  }
}

function missingAgentStatus(id: AgentId): AgentStatus {
  return {
    agentId: id,
    installed: false,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    envReady: true,
    capabilities: MOCK_CAPABILITIES[id],
  };
}

async function withConnectionEnrichment(
  backend: Backend,
  agents: AgentStatus[],
): Promise<AgentStatus[]> {
  try {
    const [accounts, providers] = await Promise.all([
      backend.account.listAccounts(),
      backend.provider.listProviders(),
    ]);
    return enrichStatusesWithConnections(agents, accounts, providers);
  } catch (e) {
    log.warn('connection enrichment failed', e);
    return enrichStatusesWithConnections(agents, [], []);
  }
}

async function withEnvFields(backend: Backend, s: AgentStatus): Promise<AgentStatus> {
  const runtimes = await backend.env.listRuntimes();
  const ch = s.channel
    ? findChannel(s.agentId, s.channel) ?? defaultChannel(s.agentId)
    : defaultChannel(s.agentId);
  const check = checkChannelEnv(ch, runtimes);
  return {
    ...s,
    envReady: check.ready,
    envMissing: check.missing.length ? check.missing : undefined,
  };
}

export function createMockAgentPort(backend: Backend): AgentPort {
  return {
    async listAgents() {
      await delay(randomLatency());
      const runtimes = await backend.env.listRuntimes();
      const mockRows = Object.values(state).map((s) => {
        const ch = s.channel
          ? findChannel(s.agentId, s.channel) ?? defaultChannel(s.agentId)
          : defaultChannel(s.agentId);
        const check = checkChannelEnv(ch, runtimes);
        return {
          ...s,
          envReady: check.ready,
          envMissing: check.missing.length ? check.missing : undefined,
        };
      });
      const merged = mergeAgentListWithCatalog(mockRows, AGENTS).map((s) => ({
        ...s,
        capabilities:
          s.capabilities ??
          AGENT_MAP[s.agentId]?.capabilities ??
          MOCK_CAPABILITIES[s.agentId],
      }));
      return withConnectionEnrichment(backend, merged);
    },

    async getAgent(agentId) {
      await delay(randomLatency());
      if (!state[agentId] && !AGENT_MAP[agentId]) {
        throw new Error(`未知 agent: ${agentId}`);
      }
      const base = await withEnvFields(backend, {
        ...(state[agentId] ?? missingAgentStatus(agentId)),
      });
      base.capabilities =
        base.capabilities ??
        AGENT_MAP[agentId]?.capabilities ??
        MOCK_CAPABILITIES[agentId];
      const [enriched] = await withConnectionEnrichment(backend, [base]);
      return enriched!;
    },

    async installAgentDetailed(agentId, channel, opts = {}) {
      await this.installAgent(agentId, channel, opts);
      return {
        ok: true,
        action: 'install_agent',
        logs: mockInstallLogs(agentId, 'install'),
        message: 'mock ok',
      };
    },

    async installAgent(agentId, channel, opts = {}) {
      await delay(randomLatency());
      const ch = findChannel(agentId, channel) ?? defaultChannel(agentId);
      const runtimes = await backend.env.listRuntimes();
      const check = checkChannelEnv(ch, runtimes);

      if (!check.ready) {
        if (!opts.installDeps) {
          throw new EnvNotReadyError(agentId, ch.id, [
            ...check.missing,
            ...check.outdated,
            ...check.broken,
          ]);
        }
        const plan = resolveAutoInstallPlan(runtimes, [
          ...check.missing,
          ...check.outdated,
          ...check.broken,
        ]);
        if (plan.targets.length) {
          await backend.env.installRuntimesBatch(plan.targets);
        }
        const again = checkChannelEnv(ch, await backend.env.listRuntimes());
        if (!again.ready) {
          throw new EnvNotReadyError(agentId, ch.id, [
            ...again.missing,
            ...again.outdated,
            ...again.broken,
          ]);
        }
      }

      const s = state[agentId];
      s.installed = true;
      s.version = s.latestVersion ?? '1.0.0';
      s.channel = ch.id as InstallChannel;
      s.binPath =
        ch.id === 'npm'
          ? `~/AppData/Roaming/npm/${agentId}.cmd`
          : `~/.local/bin/${agentId}.exe`;
      s.authStatus = 'none';
      s.authLabel = '未配置';
      return withEnvFields(backend, { ...s });
    },

    async upgradeAgentDetailed(agentId) {
      await this.upgradeAgent(agentId);
      return {
        ok: true,
        action: 'upgrade_agent',
        logs: mockInstallLogs(agentId, 'upgrade'),
        message: 'mock ok',
      };
    },

    async upgradeAgent(agentId) {
      await delay(randomLatency());
      const s = state[agentId];
      const target = s.latestVersion ?? s.version ?? '1.0.0';
      s.version = target;
      s.latestVersion = target;
      s.installed = true;
      return withEnvFields(backend, { ...s });
    },

    async uninstallAgentDetailed(agentId, deleteConfig) {
      await this.uninstallAgent(agentId, deleteConfig);
      return {
        ok: true,
        action: 'uninstall_agent',
        logs: [`$ agenthub uninstall ${agentId}`],
        message: 'mock ok',
      };
    },

    async uninstallAgent(agentId, _deleteConfig) {
      await delay(randomLatency());
      const s = state[agentId];
      s.installed = false;
      s.version = undefined;
      s.binPath = undefined;
      s.channel = undefined;
      s.authStatus = 'none';
      s.authLabel = '未配置';
      s.currentProvider = undefined;
    },

    async openAgentConfig() {
      // Browser mock cannot open real OS folders — fail closed (no fake success toast).
      throw unavailableError('openAgentConfig', 'mock 模式无法打开系统文件夹；请用 Tauri 桌面端');
    },

    async checkAgentUpdates(agentIds, _force = false) {
      await delay(randomLatency());
      const ids = agentIds?.length
        ? agentIds
        : (Object.keys(state) as AgentId[]);
      return ids.map((id) => mockUpdateInfo(id));
    },
  };
}

function mockUpdateInfo(agentId: AgentId): AgentUpdateInfo {
  const s = state[agentId] ?? missingAgentStatus(agentId);
  if (!s.installed) {
    return {
      agentId,
      state: 'not_installed',
    };
  }
  if (agentId === 'workbuddy') {
    return {
      agentId,
      state: 'unsupported',
      currentVersion: s.version,
      source: 'none',
      note: '该 Agent 仅提供官网 Setup，无法自动检测更新',
      setupUrl: 'https://www.codebuddy.cn/work/',
      checkedAt: new Date().toISOString(),
    };
  }
  // Agents with official non-npm version probes (align with core catalog).
  const officialSource =
    agentId === 'cursor'
      ? 'install-script'
      : agentId === 'grok'
        ? 'cdn:stable'
        : agentId === 'kimi' && s.channel === 'native'
          ? 'cdn'
          : null;
  if (s.channel && s.channel !== 'npm' && !officialSource) {
    return {
      agentId,
      state: 'unknown',
      currentVersion: s.version,
      source: s.channel,
      note: `当前渠道为 ${s.channel}，无远端版本源可查询；可强制升级`,
      checkedAt: new Date().toISOString(),
    };
  }
  const latest = s.latestVersion ?? s.version ?? '1.0.0';
  const current = s.version ?? '0.0.0';
  const stateOut: AgentUpdateState =
    latest !== current ? 'update_available' : 'up_to_date';
  return {
    agentId,
    state: stateOut,
    currentVersion: current,
    latestVersion: latest,
    source: officialSource ?? 'npm',
    checkedAt: new Date().toISOString(),
    note: officialSource
      ? `当前安装渠道为 ${s.channel ?? 'native'}，已对照官方版本源（${officialSource}）；升级仍按本机渠道执行`
      : undefined,
  };
}

/** Mock-only install log lines for InstallOutcome.logs */
function mockInstallLogs(agentId: AgentId, action: 'install' | 'upgrade'): string[] {
  const name = agentDisplayName(agentId);
  const ver = state[agentId].latestVersion ?? '1.0.0';
  return [
    `$ agenthub ${action} ${agentId}`,
    `正在解析 ${name} 的最新版本...`,
    `下载中 ██████████ 100% (12.4 MB)`,
    `校验 SHA256 ... 通过`,
    action === 'install' ? '写入二进制到安装目录...' : `替换旧版本二进制...`,
    `验证安装: ${agentId} --version`,
    `${agentId} ${ver}`,
    `✓ ${action === 'install' ? '安装' : '升级'}完成`,
  ];
}
