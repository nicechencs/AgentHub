import type { McpPort } from '@/lib/backend/contracts';
import type { McpInventory } from '@/lib/backend/contracts/mcp-types';
import { delay } from '@/dev/mocks/delay';

const DEMO: McpInventory = {
  sources: [
    {
      agent: 'claude',
      path: 'C:\\Users\\demo\\.claude.json',
      exists: true,
      readable: true,
      serverCount: 2,
      label: 'Claude 全局 (~/.claude.json)',
      snippet: `{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\\\work"]
    },
    "docs": {
      "type": "sse",
      "url": "https://mcp.example.com/sse"
    }
  }
}`,
    },
    {
      agent: 'claude',
      path: 'C:\\Users\\demo\\.claude\\settings.json',
      exists: true,
      readable: true,
      serverCount: 0,
      label: 'Claude settings.json',
    },
    {
      agent: 'codex',
      path: 'C:\\Users\\demo\\.codex\\config.toml',
      exists: true,
      readable: true,
      serverCount: 1,
      label: 'Codex config.toml',
      snippet: `[mcp_servers.demo]
command = "uvx"
args = ["mcp-server-demo"]
`,
    },
    {
      agent: 'workbuddy',
      path: 'C:\\Users\\demo\\.workbuddy\\.mcp.json',
      exists: false,
      readable: false,
      serverCount: 0,
      label: 'WorkBuddy .mcp.json',
    },
  ],
  servers: [
    {
      agent: 'claude',
      name: 'filesystem',
      transport: 'stdio',
      command: 'npx -y @modelcontextprotocol/server-filesystem C:\\work',
      sourcePath: 'C:\\Users\\demo\\.claude.json',
      sourceFormat: 'json',
      snippet: `{
  "filesystem": {
    "command": "npx",
    "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\\\work"]
  }
}`,
    },
    {
      agent: 'claude',
      name: 'docs',
      transport: 'sse',
      url: 'https://mcp.example.com/sse',
      sourcePath: 'C:\\Users\\demo\\.claude.json',
      sourceFormat: 'json',
      enabled: true,
      snippet: `{
  "docs": {
    "type": "sse",
    "url": "https://mcp.example.com/sse"
  }
}`,
    },
    {
      agent: 'codex',
      name: 'demo',
      transport: 'stdio',
      command: 'uvx mcp-server-demo',
      sourcePath: 'C:\\Users\\demo\\.codex\\config.toml',
      sourceFormat: 'toml',
      snippet: `[mcp_servers.demo]
command = "uvx"
args = ["mcp-server-demo"]
`,
    },
  ],
};

export function createMockMcpPort(): McpPort {
  return {
    async listInventory() {
      await delay(150);
      return structuredClone(DEMO);
    },
    async listCatalog() {
      await delay(80);
      return [
        {
          id: 'memory',
          name: 'memory',
          title: 'Memory',
          description: 'In-process memory (stdio)',
          transport: 'stdio',
          command: 'npx',
          args: ['-y', '@modelcontextprotocol/server-memory'],
          url: null,
          agents: ['claude', 'codex', 'cursor', 'workbuddy'],
        },
      ];
    },
    async probeServer(spec) {
      await delay(80);
      const transport = spec.transport || 'stdio';
      if (transport === 'stdio') {
        const cmd = (spec.command || '').trim();
        return {
          ok: Boolean(cmd),
          message: cmd ? `stdio command present: ${cmd}` : 'stdio command missing',
          transport,
          detail: null,
        };
      }
      const url = (spec.url || '').trim();
      return {
        ok: Boolean(url),
        message: url ? `url accepted: ${url}` : 'url missing',
        transport,
        detail: null,
      };
    },
    async upsertServer(agent, spec) {
      await delay(100);
      const name = spec.name.trim();
      const existing = DEMO.servers.find((s) => s.agent === agent && s.name === name);
      if (existing) {
        existing.enabled = spec.enabled !== false;
        existing.transport = spec.transport || existing.transport;
        existing.command = spec.command ?? existing.command;
        existing.url = spec.url ?? existing.url;
      } else {
        DEMO.servers.push({
          agent,
          name,
          transport: spec.transport || 'stdio',
          command: spec.command ?? null,
          url: spec.url ?? null,
          sourcePath: `mock://${agent}/mcp`,
          sourceFormat: agent === 'codex' || agent === 'grok' ? 'toml' : 'json',
          enabled: spec.enabled !== false,
          snippet: name,
        });
      }
      return {
        agent,
        name,
        path: `mock://${agent}/mcp`,
        enabled: spec.enabled !== false,
      };
    },
    async setServerEnabled(agent, name, enabled) {
      await delay(80);
      const existing = DEMO.servers.find((s) => s.agent === agent && s.name === name);
      if (!existing) {
        throw new Error(`MCP server not found: ${agent}/${name}`);
      }
      if (agent === 'codex' && !enabled) {
        DEMO.servers = DEMO.servers.filter((s) => !(s.agent === agent && s.name === name));
        return { agent, name, path: existing.sourcePath, enabled: false };
      }
      existing.enabled = enabled;
      return { agent, name, path: existing.sourcePath, enabled };
    },
  };
}
