import type { PluginPort } from '@/lib/backend/contracts';
import type { PluginEntry, PluginInventory } from '@/lib/backend/contracts/plugin-types';
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
    {
      agent: 'codex',
      support: 'listed',
      source: 'cli',
      pluginCount: 1,
    },
    {
      agent: 'pi',
      support: 'listed',
      source: 'live',
      pluginCount: 4,
    },
    { agent: 'cursor', support: 'unsupported', errorCode: 'unsupported-cursor', pluginCount: 0 },
    { agent: 'kimi', support: 'unsupported', errorCode: 'unsupported-no-cli', pluginCount: 0 },
    { agent: 'workbuddy', support: 'unsupported', errorCode: 'unsupported-no-cli', pluginCount: 0 },
    { agent: 'dsh', support: 'unsupported', errorCode: 'unsupported-dsh', pluginCount: 0 },
    { agent: 'zcode', support: 'unsupported', errorCode: 'unsupported-zcode', pluginCount: 0 },
    { agent: 'kiro', support: 'unsupported', errorCode: 'unsupported-kiro', pluginCount: 0 },
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
      agent: 'codex',
      path: '~/.codex/plugins/cache',
      exists: true,
      readable: true,
      sourceKind: 'plugin-tree',
      itemCount: 1,
      label: 'Codex plugins',
    },
    {
      agent: 'pi',
      path: '~/.pi/agent/settings.json',
      exists: true,
      readable: true,
      sourceKind: 'config',
      itemCount: 1,
      label: 'Pi installed packages',
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
      installSource: 'demo@official',
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
      installSource: 'gdrive',
      source: 'live',
      components: [
        { kind: 'skills', name: 'search' },
        { kind: 'mcp', name: 'gdrive' },
      ],
    },
    {
      id: 'codex:workflows@openai-curated',
      agent: 'codex',
      name: 'workflows',
      marketplace: 'openai-curated',
      version: '2.1.0',
      scope: 'user',
      enabled: true,
      path: '~/.codex/plugins/cache/openai-curated/workflows/2.1.0',
      description: 'Curated Codex workflow pack',
      installSource: 'workflows@openai-curated',
      source: 'cli',
      components: [{ kind: 'skills', name: 'review' }],
    },
    {
      id: 'pi:pi-subagents@npm',
      agent: 'pi',
      name: 'pi-subagents',
      marketplace: 'npm',
      version: '0.64.0',
      scope: 'user',
      path: '~/.pi/agent/npm/node_modules/pi-subagents',
      description: 'Pi extension for single-agent delegation',
      installSource: 'npm:pi-subagents',
      source: 'live',
      components: [
        { kind: 'skills', name: 'search' },
        { kind: 'agents', name: 'delegate' },
      ],
    },
    {
      id: 'pi:old-notes@npm',
      agent: 'pi',
      name: 'old-notes',
      marketplace: 'npm',
      version: '1.0.0',
      requestedVersion: '1.4',
      scope: 'user',
      path: '~/.pi/agent/npm/node_modules/old-notes',
      description: 'Notes helper that is behind its specified version',
      installSource: 'npm:old-notes@1.4',
      source: 'live',
      components: [{ kind: 'skills', name: 'note' }],
    },
    {
      id: 'pi:missing-pack@npm',
      agent: 'pi',
      name: 'missing-pack',
      marketplace: 'npm',
      version: '2.0.0',
      requestedVersion: '2.0.0',
      scope: 'user',
      description: 'Listed in Pi settings but not on disk',
      installSource: 'npm:missing-pack@2.0.0',
      source: 'live',
      components: [],
    },
    {
      id: 'pi:git-tools@git',
      agent: 'pi',
      name: 'git-tools',
      marketplace: 'git',
      version: '0.5.0',
      requestedVersion: 'main',
      scope: 'user',
      path: '~/.pi/agent/git/github.com/example/git-tools',
      description: 'Pi extension following a git ref',
      installSource: 'git:github.com/example/git-tools@main',
      source: 'live',
      components: [{ kind: 'commands', name: 'git-tools' }],
    },
  ],
};

const AVAILABLE: PluginEntry[] = [
  {
    id: 'grok:superpowers',
    agent: 'grok',
    name: 'superpowers',
    marketplace: 'xAI Official',
    description: 'Core skills library for software development',
    installSource: 'superpowers',
    source: 'available',
    components: [
      { kind: 'skills', name: 'tdd', description: 'Test-driven development' },
      { kind: 'skills', name: 'debug' },
    ],
  },
  {
    id: 'claude:demo-available@official',
    agent: 'claude',
    name: 'demo-available',
    marketplace: 'official',
    description: 'Example Claude marketplace pack',
    installSource: 'demo-available@official',
    source: 'available',
    components: [{ kind: 'commands', name: 'hello' }],
  },
  {
    id: 'codex:release-tools@openai-curated',
    agent: 'codex',
    name: 'release-tools',
    marketplace: 'openai-curated',
    description: 'Release workflows for Codex',
    installSource: 'release-tools@openai-curated',
    source: 'available',
    components: [{ kind: 'skills', name: 'release' }],
  },
  {
    id: 'codex:team-tools@team',
    agent: 'codex',
    name: 'team-tools',
    marketplace: 'team',
    description: 'Example team marketplace pack',
    installSource: 'team-tools@team',
    source: 'available',
    components: [{ kind: 'commands', name: 'team-check' }],
  },
];

let inventory: PluginInventory = structuredClone(DEMO);
let available: PluginEntry[] = structuredClone(AVAILABLE);

export function resetMockPlugins(): void {
  inventory = structuredClone(DEMO);
  available = structuredClone(AVAILABLE);
}

function assertListedAgent(agent: AgentKey): void {
  if (agent !== 'claude' && agent !== 'codex' && agent !== 'grok') {
    throw new Error(
      'enable/disable is only available for listed Claude, Codex, and Grok plugin packs',
    );
  }
}

function assertInstallAgent(agent: AgentKey): void {
  if (agent !== 'claude' && agent !== 'codex' && agent !== 'grok' && agent !== 'pi') {
    throw new Error('install is only available for listed Claude, Codex, Grok, and Pi plugin packs');
  }
}

function assertMarketplaceAgent(agent: AgentKey): void {
  if (agent !== 'claude' && agent !== 'codex' && agent !== 'grok') {
    throw new Error('marketplace refresh is only available for Claude, Codex, and Grok');
  }
}

function assertUpdateAgent(agent: AgentKey): void {
  if (agent !== 'claude' && agent !== 'grok') {
    throw new Error('individual update is only available for Claude and Grok plugin packs');
  }
}

function isSafePluginIdentifier(value: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(value) && value !== 'mcpServers';
}

function isExactNpmSemver(value?: string | null): boolean {
  const match = (value?.trim() ?? '').match(
    /^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/,
  );
  if (!match) return false;
  const prerelease = match[4];
  return (
    !prerelease ||
    prerelease
      .split('.')
      .every((part) => !/^\d+$/.test(part) || part === '0' || !part.startsWith('0'))
  );
}

function previewName(agent: AgentKey, source: string): string {
  if (agent !== 'pi') return source.split('@')[0] || source;
  if (source.startsWith('npm:')) {
    const packageSpec = source.slice(4);
    const versionAt = packageSpec.lastIndexOf('@');
    return versionAt > 0 ? packageSpec.slice(0, versionAt) : packageSpec;
  }
  const withoutRef = source.split('#')[0]?.replace(/@[^/]+$/, '') ?? source;
  return withoutRef.split(/[\\/]/).filter(Boolean).at(-1) ?? source;
}

function isPiInstallSource(source: string): boolean {
  return (
    /^npm:\S+$/.test(source) ||
    /^(?:git:|https?:\/\/|ssh:\/\/|git@)\S+$/.test(source) ||
    source.startsWith('/') ||
    source.startsWith('~/') ||
    source === '~' ||
    source.startsWith('~\\') ||
    source.startsWith('\\\\') ||
    /^[A-Za-z]:[\\/]/.test(source)
  );
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
    async listAvailable(agent) {
      await delay(40);
      assertInstallAgent(agent);
      return structuredClone(available.filter((row) => row.agent === agent));
    },
    async previewInstall(agent, source) {
      await delay(40);
      assertInstallAgent(agent);
      const trimmed = source.trim();
      const fromCatalog = available.find(
        (row) =>
          row.agent === agent &&
          (row.installSource === trimmed ||
            (agent !== 'codex' &&
              (row.name === trimmed ||
                (row.marketplace ? `${row.name}@${row.marketplace}` : row.name) === trimmed))),
      );
      if (fromCatalog) return structuredClone(fromCatalog);
      if (agent === 'codex') {
        throw new Error('plugin is not available from a configured Codex marketplace');
      }
      if (agent === 'pi' && !isPiInstallSource(trimmed)) {
        throw new Error('Pi install source must be npm:, git, or a local path');
      }
      return {
        id: `${agent}:${trimmed}`,
        agent,
        name: previewName(agent, trimmed),
        installSource: trimmed,
        source: 'available',
        components: [],
      };
    },
    async install(agent, source, options) {
      await delay(40);
      assertInstallAgent(agent);
      if (!options.confirmed) {
        throw new Error('installation needs confirmation');
      }
      const preview = await this.previewInstall(agent, source);
      if (inventory.plugins.some((p) => p.agent === agent && p.name === preview.name)) {
        throw new Error(`plugin already listed: ${preview.name}`);
      }
      inventory.plugins.push({
        ...preview,
        id: `${agent}:${preview.name}${preview.marketplace ? `@${preview.marketplace}` : ''}`,
        enabled: true,
        source: 'cli',
        path:
          agent === 'grok'
            ? `~/.grok/plugins/${preview.name}`
            : agent === 'codex'
              ? `~/.codex/plugins/cache/${preview.marketplace ?? 'unknown'}/${preview.name}/1.0.0`
              : agent === 'pi'
                ? `~/.pi/agent/packages/${preview.name}`
                : `~/.claude/plugins/cache/${preview.name}/1.0.0`,
        version: preview.version ?? '1.0.0',
        scope: 'user',
      });
      const status = inventory.agents.find((row) => row.agent === agent);
      if (status) status.pluginCount = inventory.plugins.filter((p) => p.agent === agent).length;
    },
    async uninstall(agent, name, marketplace, installSource, _options) {
      await delay(40);
      assertInstallAgent(agent);
      if (agent === 'pi' && !installSource) {
        throw new Error('Pi uninstall requires the exact install source');
      }
      const index = inventory.plugins.findIndex(
        (p) =>
          p.agent === agent &&
          p.name === name &&
          (agent !== 'pi' || p.installSource === installSource) &&
          (marketplace == null || marketplace === '' || p.marketplace === marketplace),
      );
      if (index < 0) {
        throw new Error(`plugin not listed: ${name}`);
      }
      inventory.plugins.splice(index, 1);
      const status = inventory.agents.find((row) => row.agent === agent);
      if (status) status.pluginCount = inventory.plugins.filter((p) => p.agent === agent).length;
    },
    async enable(agent, name, marketplace) {
      await delay(40);
      setEnabled(agent, name, marketplace, true);
    },
    async disable(agent, name, marketplace) {
      await delay(40);
      setEnabled(agent, name, marketplace, false);
    },
    async refreshMarketplace(agent) {
      await delay(40);
      assertMarketplaceAgent(agent);
    },
    async update(agent, name, marketplace, scope, options) {
      await delay(40);
      assertUpdateAgent(agent);
      if (!options.confirmed) throw new Error('update needs confirmation');
      if (scope !== 'user') throw new Error('only user-scope plugin packs can be updated here');
      if (!isSafePluginIdentifier(name)) throw new Error('invalid plugin name');
      const row = inventory.plugins.find(
        (plugin) =>
          plugin.agent === agent &&
          plugin.name === name &&
          (marketplace == null || marketplace === '' || plugin.marketplace === marketplace),
      );
      if (!row) throw new Error('plugin not listed');
      row.version = row.version ? `${row.version}-updated` : 'updated';
    },
    async updatePi(options) {
      await delay(40);
      if (!options.confirmed) throw new Error('update needs confirmation');
      for (const row of inventory.plugins) {
        if (
          row.agent !== 'pi' ||
          (row.marketplace === 'npm' && isExactNpmSemver(row.requestedVersion))
        ) {
          continue;
        }
        row.version = row.version ? `${row.version}-updated` : 'updated';
      }
    },
  };
}
