import type { McpServerEntry } from '@/lib/backend/contracts/mcp-types';
import type { AgentKey } from '@/lib/types';

export type McpFileGroup = {
  sourcePath: string;
  sourceFormat: string;
  servers: McpServerEntry[];
};

export type McpAgentGroup = {
  agent: AgentKey;
  servers: McpServerEntry[];
  files: McpFileGroup[];
};

/** Agent → source file → servers, for the MCP table. */
export function groupMcpServersByAgentAndFile(servers: McpServerEntry[]): McpAgentGroup[] {
  const byAgent = new Map<AgentKey, McpServerEntry[]>();
  for (const s of servers) {
    const list = byAgent.get(s.agent);
    if (list) list.push(s);
    else byAgent.set(s.agent, [s]);
  }
  return [...byAgent.keys()]
    .sort((a, b) => a.localeCompare(b))
    .map((agent) => {
      const items = byAgent.get(agent) ?? [];
      const byFile = new Map<string, McpServerEntry[]>();
      for (const s of items) {
        const list = byFile.get(s.sourcePath);
        if (list) list.push(s);
        else byFile.set(s.sourcePath, [s]);
      }
      const files = [...byFile.keys()]
        .sort((a, b) => a.localeCompare(b))
        .map((sourcePath) => {
          const fileServers = [...(byFile.get(sourcePath) ?? [])].sort((a, b) =>
            a.name.localeCompare(b.name),
          );
          return {
            sourcePath,
            sourceFormat: fileServers[0]?.sourceFormat ?? '',
            servers: fileServers,
          };
        });
      return { agent, servers: items, files };
    });
}
