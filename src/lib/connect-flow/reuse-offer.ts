/**
 * 是否在 Connections 行展示「接到…」。
 *
 * 目标语义（docs/connection-binding-model.md §5.2）：
 * - 每一张**真票**都有「接到…」
 * - 生成投影与非票行不展示入口
 * - 不可行目标在 ConnectFlow 对话框内置灰 + 原因，不在列表隐藏入口
 *
 * 可行性权威是 `plan.canApply`（现在能否写入），本文件不再镜像商品白名单。
 */
import type { Provider } from '@/lib/types';

export const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
export const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
export const ANTHROPIC_API_ENDPOINT_NEEDLE = 'api.anthropic.com';

export const SOURCE_ALL_INFEASIBLE_MESSAGE =
  '当前没有可写入的目标 Agent。不可行的目标仍会留在列表里并显示原因；当前不支持不等于连接失效。';

export const AGENT_ALL_INFEASIBLE_MESSAGE = '现有凭据都不可用于此连接。可新增凭据后再试。';

export type ReuseOfferEntry = {
  source: 'account' | 'provider';
  id: string;
  agentId: string;
  /** Optional; surface helpers may still classify providers. */
  provider?: Pick<Provider, 'agentId' | 'preset' | 'configText'>;
};

function textHasNeedle(text: string | undefined, needle: string): boolean {
  return typeof text === 'string' && text.toLowerCase().includes(needle.toLowerCase());
}

/** Surface helpers kept for tests / diagnostics; no longer gate the row button. */
export function isKimiMembershipProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  if (provider.agentId !== 'kimi') return false;
  return provider.preset === KIMI_MEMBERSHIP_PRESET
    || textHasNeedle(provider.configText, KIMI_CODING_ENDPOINT_NEEDLE);
}

export function isAnthropicApiProvider(provider: Pick<Provider, 'agentId' | 'preset' | 'configText'>): boolean {
  if (provider.agentId !== 'claude') return false;
  return provider.preset === 'anthropic'
    || textHasNeedle(provider.configText, ANTHROPIC_API_ENDPOINT_NEEDLE);
}

/**
 * 是否视为可展示「接到…」的真票行。
 * account 与非投影 provider 均为真票；生成投影由 shouldShowReuseAction 排除。
 */
export function connectionCanReuseToOtherAgents(entry: ReuseOfferEntry): boolean {
  if (entry.source === 'account') return true;
  if (entry.source === 'provider') return Boolean(entry.id);
  return false;
}

/**
 * 真票常驻「接到…」；仅排除生成投影与未接线页面。
 */
export function shouldShowReuseAction(
  entry: ReuseOfferEntry,
  options: {
    reuseEnabled?: boolean;
    adapterGeneratedProviderIds?: ReadonlySet<string>;
  },
): boolean {
  if (!options.reuseEnabled) return false;
  if (entry.source === 'provider' && options.adapterGeneratedProviderIds?.has(entry.id)) {
    return false;
  }
  return connectionCanReuseToOtherAgents(entry);
}
