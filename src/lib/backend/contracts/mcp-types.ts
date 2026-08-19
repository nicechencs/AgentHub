import type { AgentId } from '@/lib/types';

export interface McpServerEntry {
  agent: AgentId;
  name: string;
  transport: string;
  command?: string | null;
  url?: string | null;
  sourcePath: string;
  sourceFormat: string;
  enabled?: boolean | null;
  /** Redacted raw fragment for this server only. */
  snippet?: string | null;
}

export interface McpSourceFile {
  agent: AgentId;
  path: string;
  exists: boolean;
  readable: boolean;
  error?: string | null;
  serverCount: number;
  label: string;
  /** Redacted MCP-related section of the source file. */
  snippet?: string | null;
}

export interface McpInventory {
  sources: McpSourceFile[];
  servers: McpServerEntry[];
}

export interface McpPort {
  listInventory(): Promise<McpInventory>;
}
