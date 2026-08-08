import { AGENT_MAP, type InstallChannelMeta } from '@/config/agents';
import { RUNTIME_MAP } from '@/config/runtimes';
import type { AgentId, ChannelEnvCheck, RuntimeDetect, RuntimeId } from '@/lib/types';

/** 根据 Runtime 检测结果判断某渠道是否可安装 */
export function checkChannelEnv(
  channel: InstallChannelMeta,
  runtimes: RuntimeDetect[],
): ChannelEnvCheck {
  const byId = new Map(runtimes.map((r) => [r.id, r]));
  const missing: RuntimeId[] = [];
  const outdated: RuntimeId[] = [];
  const broken: RuntimeId[] = [];

  for (const id of channel.requires) {
    const r = byId.get(id);
    if (!r || r.status === 'missing') missing.push(id);
    else if (r.status === 'outdated') outdated.push(id);
    else if (r.status === 'broken_path') broken.push(id);
  }

  return {
    channelId: channel.id,
    ready: missing.length === 0 && outdated.length === 0 && broken.length === 0,
    missing,
    outdated,
    broken,
  };
}

export function defaultChannel(agentId: AgentId): InstallChannelMeta {
  const channels = AGENT_MAP[agentId]?.installChannels;
  if (channels?.[0]) return channels[0];
  // Fail-soft before catalog hydrate / unknown agent id.
  return {
    id: 'native',
    label: 'native',
    command: '',
    requires: [],
  };
}

export function findChannel(agentId: AgentId, channelId: string): InstallChannelMeta | undefined {
  return AGENT_MAP[agentId]?.installChannels.find((c) => c.id === channelId);
}

/** 汇总所有 Runtime 是否整体健康 */
export function allRuntimesOk(runtimes: RuntimeDetect[]): boolean {
  return runtimes.every((r) => r.status === 'ok');
}

export function runtimeLabel(id: RuntimeId): string {
  return RUNTIME_MAP[id]?.name ?? id;
}

export function formatMissingList(ids: RuntimeId[]): string {
  return ids.map(runtimeLabel).join('、');
}

/** 是否有任一 Runtime 处于非 ok */
export function hasEnvIssues(runtimes: RuntimeDetect[]): boolean {
  return runtimes.some((r) => r.status !== 'ok');
}
