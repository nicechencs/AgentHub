import { getBackend } from '@/app/runtime';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';

/** Read-only scan of vendor plugin / extension packs (not MCP). */
export async function listPluginInventory(): Promise<PluginInventory> {
  return getBackend().plugins.listInventory();
}
