import type { PluginPort } from '@/lib/backend/contracts';
import type { PluginInventory } from '@/lib/backend/contracts/plugin-types';
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

export function createMockPluginPort(): PluginPort {
  return {
    async listInventory() {
      await delay(150);
      return structuredClone(DEMO);
    },
  };
}
