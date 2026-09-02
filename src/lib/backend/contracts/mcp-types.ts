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

export interface McpPort {
  listInventory(): Promise<McpInventory>;
}
