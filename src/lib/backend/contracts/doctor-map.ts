/**
 * DoctorReport → 前端 UI 模型（RuntimeDetect / AgentStatus）的集中映射。
 * 纯映射：不注入 browser demo capability；matrix 缺失时返回 undefined。
 */
import { RUNTIME_MAP } from '@/config/runtimes';
import { checkChannelEnv, defaultChannel, findChannel } from '@/lib/env';
import type {
  DoctorCapabilityState,
  DoctorDetectResult,
  DoctorEnvStatus,
  DoctorRemediation,
  DoctorReport,
} from './doctor-types';
import type { AgentCapabilities, AgentCapability, Capability } from '@/lib/capability';
import type {
  AgentStatus,
  EnvRemediation,
  InstallChannel,
  RemediationKind,
  RuntimeDetect,
} from '@/lib/types';

const INSTALL_CHANNELS: InstallChannel[] = ['native', 'npm'];

function asInstallChannel(raw?: string | null): InstallChannel | undefined {
  if (!raw) return undefined;
  const id = raw.toLowerCase();
  return INSTALL_CHANNELS.includes(id as InstallChannel)
    ? (id as InstallChannel)
    : undefined;
}

function mapCapabilityState(s: DoctorCapabilityState): AgentCapability {
  return {
    level: s.level,
    reason: s.reason ?? undefined,
    minVersion: s.minVersion ?? undefined,
  };
}

export function mapDoctorCapabilities(
  raw?: Record<string, Record<string, DoctorCapabilityState>>,
  agentId?: string,
): AgentCapabilities | undefined {
  if (!agentId || !raw) return undefined;
  // Only doctor-provided rows — no silent mock fill in production mapping.
  const row = raw[agentId];
  if (!row) return undefined;
  const out: AgentCapabilities = {};
  for (const [key, val] of Object.entries(row)) {
    if (val && typeof val === 'object' && 'level' in val) {
      out[key as Capability] = mapCapabilityState(val as DoctorCapabilityState);
    }
  }
  return out;
}

/** core Remediation（单对象）→ UI EnvRemediation[] */
export function mapDoctorRemediation(r: DoctorRemediation): EnvRemediation[] {
  const items: EnvRemediation[] = [];
  const kind = (r.kind || 'hint').toLowerCase();

  if (r.command) {
    const cmdKind: RemediationKind =
      kind === 'winget' ? 'winget' : kind === 'brew' ? 'brew' : 'command';
    items.push({
      kind: cmdKind,
      value: r.command,
      label:
        cmdKind === 'winget'
          ? '用 winget 安装'
          : cmdKind === 'brew'
            ? '用 Homebrew 安装'
            : '执行命令',
    });
  }
  if (r.url) {
    items.push({
      kind: 'url',
      value: r.url,
      label: '打开官方页面',
    });
  }
  if (r.text) {
    items.push({
      kind: 'hint',
      value: r.text,
    });
  }
  return items;
}

/** core EnvStatus → RuntimeDetect */
export function mapDoctorEnvStatus(env: DoctorEnvStatus): RuntimeDetect {
  const meta = RUNTIME_MAP[env.id];
  const fromDoctor = env.remediation ? mapDoctorRemediation(env.remediation) : [];
  const remediations =
    fromDoctor.length > 0 ? fromDoctor : (meta?.remediations ?? []);

  return {
    id: env.id,
    status: env.status,
    version: env.version ?? undefined,
    path: env.path ?? undefined,
    minRequired: env.minRequired ?? meta?.minVersion,
    remediations,
    notes: env.notes?.length ? [...env.notes] : undefined,
  };
}

/** core DetectResult + 已映射 runtimes → AgentStatus */
export function mapDoctorDetectResult(
  d: DoctorDetectResult,
  runtimes: RuntimeDetect[],
  capabilities?: Record<string, Record<string, DoctorCapabilityState>>,
): AgentStatus {
  const installed = d.status === 'installed';
  const detectedChannel = d.channel?.trim() || undefined;
  const installChannel = asInstallChannel(detectedChannel);
  const chMeta = installChannel
    ? (findChannel(d.agent, installChannel) ?? defaultChannel(d.agent))
    : defaultChannel(d.agent);
  const check = checkChannelEnv(chMeta, runtimes);

  return {
    agentId: d.agent,
    installed,
    version: d.version ?? undefined,
    channel: installed ? detectedChannel : undefined,
    binPath: d.binaryPath ?? undefined,
    extraCopies: d.extraCopies?.length ? d.extraCopies.map((c) => ({ ...c })) : undefined,
    notes: d.notes?.length ? [...d.notes] : undefined,
    // Doctor 只做安装检测；登录/API 由 listAgents 合并账号池与供应商池后覆盖
    authStatus: 'none',
    authLabel: installed ? '未检测登录态' : '未配置',
    running: false,
    // 未安装：用渠道环境检查；已安装：优先 core 的 envReady，并附带缺失列表
    envReady: installed ? d.envReady : check.ready,
    envMissing: check.missing.length ? check.missing : undefined,
    capabilities: mapDoctorCapabilities(capabilities, d.agent),
  };
}

/** 整份 DoctorReport → 页面可用的 runtimes + agents */
export function mapDoctorReport(report: DoctorReport): {
  runtimes: RuntimeDetect[];
  agents: AgentStatus[];
} {
  const runtimes = report.runtimes.map(mapDoctorEnvStatus);
  const agents = report.agents.map((a) =>
    mapDoctorDetectResult(a, runtimes, report.capabilities),
  );
  return { runtimes, agents };
}
