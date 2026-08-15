/**
 * 是否在 Connections 行展示「用于其他 Agent」。
 *
 * 行按钮只做可行动作入口；不可行来源的原因诊断由 Dashboard「连接/切换」承担。
 *
 * 本文件镜像后端门禁，真源：
 * - `crates/agenthub-core/src/services/adapter_route_service.rs` 的 `classify`
 * - 同文件 `implemented_apply_whitelist`（`source_kind != Provider` 一律 canApply=false）
 *
 * 能力矩阵或 apply 白名单变更时，必须同步本文件与 `reuse-offer.test.ts`。
 */
import type { Provider } from '@/lib/types';

export const KIMI_MEMBERSHIP_PRESET = 'kimi-code-membership';
export const KIMI_CODING_ENDPOINT_NEEDLE = 'api.kimi.com/coding';
export const ANTHROPIC_API_ENDPOINT_NEEDLE = 'api.anthropic.com';

export const SOURCE_ALL_INFEASIBLE_MESSAGE =
  '这条凭据目前不能接到其他 Agent。跨服务复用只支持 Kimi Code 会员 Provider（→ Claude / Codex / Pi）和 Claude 的 Anthropic Provider（→ Pi）。当前不支持不等于连接失效。';

export const AGENT_ALL_INFEASIBLE_MESSAGE = '现有凭据都不可用于此连接。可新增凭据后再试。';

export type ReuseOfferEntry = {
  source: 'account' | 'provider';
  id: string;
  agentId: string;
  provider?: Pick<Provider, 'agentId' | 'preset' | 'configText'>;
};

function textHasNeedle(text: string | undefined, needle: string): boolean {
  return typeof text === 'string' && text.toLowerCase().includes(needle.toLowerCase());
}

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
 * 来源是否存在「接到另一个 Agent」的可应用白名单路径。
 * account 来源一律关闭（`implemented_apply_whitelist` 对非 Provider 返回 false）。
 * Codex/Claude OAuth、Kimi 开放平台、生成 Provider 都不算。
 */
export function connectionCanReuseToOtherAgents(entry: ReuseOfferEntry): boolean {
  if (entry.source === 'account') {
    // adapter_route_service.rs implemented_apply_whitelist: source_kind != Provider → false
    return false;
  }
  if (entry.source === 'provider' && entry.provider) {
    if (isKimiMembershipProvider(entry.provider)) return true;
    // isAnthropicApiProvider already requires provider.agentId === 'claude'
    if (isAnthropicApiProvider(entry.provider)) return true;
    return false;
  }
  return false;
}

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
