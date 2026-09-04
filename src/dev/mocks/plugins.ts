import type { PluginPort } from '@/lib/backend/contracts';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';
import { delay } from '@/dev/mocks/delay';

const DEMO: PluginInventory = {
  agents: [
    {
      agent: 'claude',
      support: 'listed',
      source: 'cli',
      pluginCount: 1,
    },
    {
      agent: 'grok',
      support: 'listed',
      source: 'live',
      pluginCount: 1,
    },
    { agent: 'codex', support: 'planned', errorCode: 'planned', pluginCount: 0 },
    { agent: 'pi', support: 'planned', errorCode: 'planned', pluginCount: 0 },
    { agent: 'cursor', support: 'unsupported', errorCode: 'unsupported-cursor', pluginCount: 0 },
    { agent: 'kimi', support: 'unsupported', errorCode: 'unsupported-no-cli', pluginCount: 0 },
    { agent: 'workbuddy', support: 'unsupported', errorCode: 'unsupported-no-cli', pluginCount: 0 },
    { agent: 'dsh', support: 'unsupported', errorCode: 'unsupported-dsh', pluginCount: 0 },
    { agent: 'zcode', support: 'unsupported', errorCode: 'unsupported-zcode', pluginCount: 0 },
  ],
  sources: [
    {
      agent: 'claude',
      path: '~/.claude/plugins',
      exists: true,
      readable: true,
      sourceKind: 'plugin-tree',
      itemCount: 1,
      label: 'Claude plugins',
    },
    {
      agent: 'grok',
      path: '~/.grok/plugins',
      exists: true,
      readable: true,
      sourceKind: 'plugin-tree',
      itemCount: 1,
      label: 'Grok plugins',
    },
    {
      agent: 'cursor',
      path: '~/.cursor/skills-cursor',
      exists: false,
      readable: false,
      sourceKind: 'skills',
      itemCount: 0,
      label: 'Cursor skills',
    },
    {
      agent: 'kimi',
      path: '~/.kimi-code/skills',
      exists: false,
      readable: false,
      sourceKind: 'skills',
      itemCount: 0,
      label: 'Kimi skills',
    },
    {
      agent: 'workbuddy',
      path: '~/.workbuddy/.mcp.json',
      exists: false,
      readable: false,
      sourceKind: 'mcp',
      itemCount: 0,
      label: 'WorkBuddy MCP config',
    },
    {
      agent: 'dsh',
      path: '~/.dsh/cordis.patch.yml',
      exists: false,
      readable: false,
      sourceKind: 'cordis',
      itemCount: 0,
      label: 'DSH Cordis patch',
    },
  ],
  plugins: [
    {
      id: 'claude:demo@official',
      agent: 'claude',
      name: 'demo',
      marketplace: 'official',
      version: '1.2.0',
      scope: 'user',
      enabled: true,
      path: '~/.claude/plugins/cache/demo/1.2.0',
      description: 'Example Claude plugin pack',
      source: 'cli',
      components: [
        { kind: 'skills', name: 'ship', description: 'Ship a release' },
        { kind: 'commands', name: 'demo' },
      ],
    },
    {
      id: 'grok:gdrive',
      agent: 'grok',
      name: 'gdrive',
      marketplace: 'xAI Official',
      version: '0.4.0',
      scope: 'user',
      enabled: false,
      trusted: true,
      path: '~/.grok/plugins/gdrive',
      description: 'Google Drive pack',
      source: 'live',
      components: [
        { kind: 'skills', name: 'search' },
        { kind: 'mcp', name: 'gdrive' },
      ],
    },
  ],
};

let inventory: PluginInventory = structuredClone(DEMO);

export function resetMockPlugins(): void {
  inventory = structuredClone(DEMO);
}

function assertListedAgent(agent: AgentKey): void {
  if (agent !== 'claude' && agent !== 'grok') {
    throw new Error('enable/disable is only available for listed Claude and Grok plugin packs');
  }
}

function setEnabled(agent: AgentKey, name: string, marketplace: string | null | undefined, enabled: boolean) {
  assertListedAgent(agent);
  const row = inventory.plugins.find(
    (p) =>
      p.agent === agent &&
      p.name === name &&
      (marketplace == null || marketplace === '' || p.marketplace === marketplace),
  );
  if (!row) {
    throw new Error(`plugin not listed: ${name}`);
  }
  row.enabled = enabled;
}

export function createMockPluginPort(): PluginPort {
  return {
    async listInventory() {
      await delay(150);
      return structuredClone(inventory);
    },
    async enable(agent, name, marketplace) {
      await delay(40);
      setEnabled(agent, name, marketplace, true);
    },
    async disable(agent, name, marketplace) {
      await delay(40);
      setEnabled(agent, name, marketplace, false);
    },
  };
}
