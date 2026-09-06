import type {
  RuntimeExtensionItem,
  RuntimeModelOption,
  RuntimeOptions,
} from '@/lib/api/chat';

export type RuntimeCatalogMemory = {
  conversationId: string | null;
  models: RuntimeModelOption[];
  extensions: RuntimeExtensionItem[];
};

/**
 * Keep the last non-empty catalog for the same conversation when a frozen
 * options read returns empty lists (never fetched / mid-turn no-spawn).
 */
export function retainRuntimeCatalog(
  prior: RuntimeCatalogMemory,
  next: RuntimeOptions,
  conversationId: string,
): Pick<RuntimeCatalogMemory, 'models' | 'extensions'> {
  const sameConversation = prior.conversationId === conversationId;
  const nextEmpty = next.models.length === 0 && next.extensions.length === 0;
  const priorUseful = prior.models.length > 0 || prior.extensions.length > 0;
  if (next.settingsFrozen && nextEmpty && sameConversation && priorUseful) {
    return { models: prior.models, extensions: prior.extensions };
  }
  return { models: next.models, extensions: next.extensions };
}
