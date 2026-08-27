import { getBackend } from '@/app/runtime';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';
import type { AgentId } from '@/lib/types';

/** Scan of vendor plugin / extension packs (not MCP). */
export async function listPluginInventory(): Promise<PluginInventory> {
  return getBackend().plugins.listInventory();
}

/** Turn on a listed Claude or Grok pack via the official command. */
export async function enablePlugin(
  agent: AgentId,
  name: string,
  marketplace?: string | null,
): Promise<void> {
  return getBackend().plugins.enable(agent, name, marketplace);
}

/** Turn off a listed Claude or Grok pack via the official command. */
export async function disablePlugin(
  agent: AgentId,
  name: string,
  marketplace?: string | null,
): Promise<void> {
  return getBackend().plugins.disable(agent, name, marketplace);
}
