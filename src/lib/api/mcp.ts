import { getBackend } from '@/app/runtime';
import type { McpInventory } from '@/lib/backend/contracts/mcp-types';

/** Read-only scan of known agent MCP config files. */
export async function listMcpInventory(): Promise<McpInventory> {
  return getBackend().mcp.listInventory();
}
