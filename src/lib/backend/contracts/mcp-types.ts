import type { AgentKey } from '@/lib/types';

export interface McpServerEntry {
  agent: AgentKey;
  name: string;
  transport: string;
  command?: string | null;
  url?: string | null;
  sourcePath: string;
  sourceFormat: string;
  enabled?: boolean | null;
  /** Local-file fragment for this server only. */
  snippet?: string | null;
}

export interface McpSourceFile {
  agent: AgentKey;
  path: string;
  exists: boolean;
  readable: boolean;
  error?: string | null;
  serverCount: number;
  label: string;
  /** MCP-related section of the source file. */
  snippet?: string | null;
}

export interface McpInventory {
  sources: McpSourceFile[];
  servers: McpServerEntry[];
}

export interface McpCatalogEntry {
  id: string;
  name: string;
  title: string;
  description: string;
  transport: string;
  command?: string | null;
  args: string[];
  url?: string | null;
  agents: string[];
}

export interface McpServerSpec {
  name: string;
  transport?: string;
  command?: string | null;
  args?: string[];
  url?: string | null;
  enabled?: boolean | null;
}

export interface McpProbeResult {
  ok: boolean;
  message: string;
  transport: string;
  detail?: string | null;
}

export interface McpWriteResult {
  agent: AgentKey;
  name: string;
  path: string;
  enabled: boolean;
}

export interface McpPort {
  listInventory(): Promise<McpInventory>;
  listCatalog(): Promise<McpCatalogEntry[]>;
  probeServer(spec: McpServerSpec): Promise<McpProbeResult>;
  upsertServer(agent: AgentKey, spec: McpServerSpec): Promise<McpWriteResult>;
  setServerEnabled(agent: AgentKey, name: string, enabled: boolean): Promise<McpWriteResult>;
}
