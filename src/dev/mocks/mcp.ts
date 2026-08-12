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
    },
    {
      agent: 'codex',
      path: 'C:\\Users\\demo\\.codex\\config.toml',
      exists: true,
      readable: true,
      serverCount: 1,
      label: 'Codex config.toml',
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
    },
    {
      agent: 'claude',
      name: 'docs',
      transport: 'sse',
      url: 'https://mcp.example.com/sse',
      sourcePath: 'C:\\Users\\demo\\.claude.json',
      sourceFormat: 'json',
      enabled: true,
    },
    {
      agent: 'codex',
      name: 'demo',
      transport: 'stdio',
      command: 'uvx mcp-server-demo',
      sourcePath: 'C:\\Users\\demo\\.codex\\config.toml',
      sourceFormat: 'toml',
    },
  ],
};

export function createMockMcpPort(): McpPort {
  return {
    async listInventory() {
      await delay(150);
      return structuredClone(DEMO);
    },
  };
}
