import type { AgentId } from '@/lib/types';

export interface PluginComponent {
  kind: string;
  name: string;
  description?: string | null;
}

export interface PluginEntry {
  id: string;
  agent: AgentId;
  name: string;
  marketplace?: string | null;
  version?: string | null;
  scope?: string | null;
  enabled?: boolean | null;
  trusted?: boolean | null;
  path?: string | null;
  description?: string | null;
  /** cli | live */
  source: string;
  components: PluginComponent[];
}

export interface PluginAgentStatus {
  agent: AgentId;
  /** listed | planned | unsupported */
  support: string;
  source?: string | null;
  errorCode?: string | null;
  error?: string | null;
  pluginCount: number;
}

export interface PluginInventory {
  agents: PluginAgentStatus[];
  plugins: PluginEntry[];
}

export interface PluginPort {
  listInventory(): Promise<PluginInventory>;
  enable(agent: AgentId, name: string, marketplace?: string | null): Promise<void>;
  disable(agent: AgentId, name: string, marketplace?: string | null): Promise<void>;
}
