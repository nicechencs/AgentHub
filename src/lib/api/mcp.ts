import { getBackend } from '@/app/runtime';
import type {
  McpCatalogEntry,
  McpInventory,
  McpProbeResult,
  McpServerSpec,
  McpWriteResult,
} from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';

/** Scan known agent MCP config files. */
export async function listMcpInventory(): Promise<McpInventory> {
  return getBackend().mcp.listInventory();
}

/** Built-in local MCP templates (no remote marketplace). */
export async function listMcpCatalog(): Promise<McpCatalogEntry[]> {
  return getBackend().mcp.listCatalog();
}

/** Probe stdio PATH or HTTP reachability — no OAuth. */
export async function probeMcpServer(spec: McpServerSpec): Promise<McpProbeResult> {
  return getBackend().mcp.probeServer(spec);
}

/** Write MCP into a supported Agent config and refresh callers afterwards. */
export async function upsertMcpServer(
  agent: AgentKey,
  spec: McpServerSpec,
): Promise<McpWriteResult> {
  return getBackend().mcp.upsertServer(agent, spec);
}

/** Enable / disable (Codex disable removes the entry). */
export async function setMcpServerEnabled(
  agent: AgentKey,
  name: string,
  enabled: boolean,
): Promise<McpWriteResult> {
  return getBackend().mcp.setServerEnabled(agent, name, enabled);
}
