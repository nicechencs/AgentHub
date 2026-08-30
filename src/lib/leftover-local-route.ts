/**
 * Detect leftover generated 本机路由 provider rows — never labeled 官方登录.
 * Shared by Connections, Routes, and Chat.
 */
import { isInternalGeneratedName } from '@/lib/backend/contracts/agent-connection';
import type { Provider } from '@/lib/types';

const AGENTHUB_BRIDGE_SLUG = /agenthub_[^\s"'\\]*_bridge/i;

export function isLeftoverLocalRouteProvider(
  provider: Pick<Provider, 'id' | 'name' | 'preset' | 'configText' | 'configFormat'>,
): boolean {
  if (isInternalGeneratedName(provider.name) || isInternalGeneratedName(provider.id)) return true;
  const haystack = `${provider.id}\n${provider.name}\n${provider.preset ?? ''}\n${provider.configText ?? ''}`;
  return haystack.includes('本机路由') || haystack.includes('Local route') || AGENTHUB_BRIDGE_SLUG.test(haystack);
}

export function leftoverProviderIsCurrent(providers: readonly Provider[]): boolean {
  return providers.some((provider) => provider.isCurrent && isLeftoverLocalRouteProvider(provider));
}
