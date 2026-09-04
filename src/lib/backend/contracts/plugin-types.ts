import type { AgentKey } from '@/lib/types';

export interface PluginComponent {
  kind: string;
  name: string;
  description?: string | null;
}

export interface PluginEntry {
  id: string;
  agent: AgentKey;
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
  agent: AgentKey;
  /** listed | planned | unsupported */
  support: string;
  source?: string | null;
  errorCode?: string | null;
  error?: string | null;
  pluginCount: number;
}

export interface PluginSourceFile {
  agent: AgentKey;
  path: string;
  exists: boolean;
  readable: boolean;
  error?: string | null;
  /** plugin-tree | config | skills | mcp | cordis */
  sourceKind: string;
  itemCount: number;
  label: string;
}

export interface PluginInventory {
  agents: PluginAgentStatus[];
  plugins: PluginEntry[];
  sources?: PluginSourceFile[];
}

export interface PluginPort {
  listInventory(): Promise<PluginInventory>;
  enable(agent: AgentKey, name: string, marketplace?: string | null): Promise<void>;
  disable(agent: AgentKey, name: string, marketplace?: string | null): Promise<void>;
}
