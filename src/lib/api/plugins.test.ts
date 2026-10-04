import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import {
  disablePlugin,
  enablePlugin,
  installPlugin,
  listAvailablePlugins,
  listPluginInventory,
  previewPluginInstall,
  refreshPluginMarketplace,
  uninstallPlugin,
  updatePiPlugins,
  updatePlugin,
} from '@/lib/api/plugins';

describe('plugin inventory and enable/disable (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
  });

  it('returns plugin packs rather than MCP server rows', async () => {
    const inv = await listPluginInventory();
    expect(inv.plugins.map((p) => p.name).sort()).toEqual([
      'demo',
      'gdrive',
      'git-tools',
      'missing-pack',
      'old-notes',
      'pi-subagents',
      'workflows',
    ]);
    expect(inv.plugins.every((p) => p.name !== 'filesystem' && p.name !== 'mcpServers')).toBe(
      true,
    );
    const grok = inv.plugins.find((p) => p.agent === 'grok');
    expect(grok?.components.some((c) => c.kind === 'mcp' && c.name === 'gdrive')).toBe(true);
    expect(inv.agents.find((a) => a.agent === 'claude')?.support).toBe('listed');
    expect(inv.agents.find((a) => a.agent === 'pi')?.support).toBe('listed');
    expect(inv.plugins.find((p) => p.name === 'old-notes')?.requestedVersion).toBe('1.4');
    expect(inv.plugins.find((p) => p.name === 'missing-pack')?.path).toBeFalsy();
    expect(inv.agents.find((a) => a.agent === 'codex')?.support).toBe('listed');
    expect(inv.plugins.find((p) => p.name === 'pi-subagents')?.installSource).toBe(
      'npm:pi-subagents',
    );
    expect(inv.sources?.some((s) => s.agent === 'cursor' && s.sourceKind === 'skills')).toBe(true);
    expect(inv.sources?.some((s) => s.agent === 'dsh' && s.sourceKind === 'cordis')).toBe(true);
  });

  it('round-trips enable then disable for listed Claude, Codex, and Grok packs', async () => {
    await disablePlugin('claude', 'demo', 'official');
    await disablePlugin('codex', 'workflows', 'openai-curated');
    await enablePlugin('grok', 'gdrive', 'xAI Official');
    let inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'claude')?.enabled).toBe(false);
    expect(inv.plugins.find((p) => p.agent === 'grok')?.enabled).toBe(true);
    expect(inv.plugins.find((p) => p.agent === 'codex')?.enabled).toBe(false);

    await enablePlugin('claude', 'demo', 'official');
    await disablePlugin('grok', 'gdrive', 'xAI Official');
    inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'claude')?.enabled).toBe(true);
    expect(inv.plugins.find((p) => p.agent === 'grok')?.enabled).toBe(false);
  });

  it('rejects enable/disable for planned and unsupported agents', async () => {
    await expect(enablePlugin('pi', 'pi-subagents')).rejects.toThrow(/Claude, Codex, and Grok/);
    await expect(disablePlugin('cursor', 'anything')).rejects.toThrow(
      /Claude, Codex, and Grok/,
    );
  });

  it('lists marketplace packs separately from installed inventory', async () => {
    const available = await listAvailablePlugins('grok');
    expect(available.map((p) => p.name)).toEqual(['superpowers']);
    expect(available[0]?.source).toBe('available');
    expect(available[0]?.installSource).toBe('superpowers');
    const codexAvailable = await listAvailablePlugins('codex');
    expect(codexAvailable.map((p) => p.installSource)).toEqual([
      'release-tools@openai-curated',
      'team-tools@team',
    ]);
    await expect(listAvailablePlugins('pi')).resolves.toEqual([]);
    const inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.name === 'superpowers')).toBe(false);
  });

  it('refuses install without confirmation', async () => {
    await expect(
      installPlugin('grok', 'superpowers', { confirmed: false }),
    ).rejects.toThrow(/confirmation/);
    const inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.name === 'superpowers')).toBe(false);
  });

  it('installs then uninstalls a Grok marketplace pack', async () => {
    const preview = await previewPluginInstall('grok', 'superpowers');
    expect(preview.components.some((c) => c.kind === 'skills')).toBe(true);
    await installPlugin('grok', 'superpowers', { confirmed: true });
    let inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.agent === 'grok' && p.name === 'superpowers')).toBe(true);

    await uninstallPlugin('grok', 'superpowers', 'xAI Official', 'superpowers', {
      keepData: true,
    });
    inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.name === 'superpowers')).toBe(false);
    expect(inv.plugins.some((p) => p.name === 'gdrive')).toBe(true);
  });

  it('installs Codex and removes Pi with the stable install source', async () => {
    await expect(previewPluginInstall('codex', 'release-tools')).rejects.toThrow(
      /configured Codex marketplace/,
    );
    await expect(previewPluginInstall('pi', 'new-pack')).rejects.toThrow(
      /npm:, git, or a local path/,
    );
    await expect(previewPluginInstall('pi', './relative-pack')).rejects.toThrow(
      /npm:, git, or a local path/,
    );
    await expect(previewPluginInstall('pi', '~/src/local-pack')).resolves.toMatchObject({
      installSource: '~/src/local-pack',
    });
    await installPlugin('codex', 'release-tools@openai-curated', { confirmed: true });
    await installPlugin('pi', 'npm:new-pack@1.2', { confirmed: true });
    let inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'codex' && p.name === 'release-tools')).toBeTruthy();
    expect(inv.plugins.find((p) => p.name === 'new-pack')?.installSource).toBe(
      'npm:new-pack@1.2',
    );

    await expect(
      uninstallPlugin('pi', 'new-pack', null, null, { keepData: true }),
    ).rejects.toThrow(/exact install source/);
    await uninstallPlugin('pi', 'new-pack', null, 'npm:new-pack@1.2', { keepData: true });
    inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.name === 'new-pack')).toBe(false);
  });

  it('keeps marketplace refresh separate from installed-pack updates', async () => {
    await expect(refreshPluginMarketplace('claude')).resolves.toBeUndefined();
    await expect(refreshPluginMarketplace('codex')).resolves.toBeUndefined();
    await expect(refreshPluginMarketplace('grok')).resolves.toBeUndefined();
    await expect(refreshPluginMarketplace('pi')).rejects.toThrow(/Claude, Codex, and Grok/);

    await expect(
      updatePlugin('claude', 'demo', 'official', 'user', { confirmed: false }),
    ).rejects.toThrow(/confirmation/);
    await expect(
      updatePlugin('claude', 'demo', 'official', 'project', { confirmed: true }),
    ).rejects.toThrow(/user-scope/);
    await expect(
      updatePlugin('grok', '--debug', null, 'user', { confirmed: true }),
    ).rejects.toThrow(/invalid plugin name/);
    await expect(
      updatePlugin('grok', 'bad\nname', null, 'user', { confirmed: true }),
    ).rejects.toThrow(/invalid plugin name/);
    await updatePlugin('claude', 'demo', 'official', 'user', { confirmed: true });
    const inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'claude')?.version).toBe('1.2.0-updated');
  });

  it('updates only unpinned Pi extensions', async () => {
    await expect(updatePiPlugins({ confirmed: false })).rejects.toThrow(/confirmation/);
    await updatePiPlugins({ confirmed: true });
    const inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.name === 'pi-subagents')?.version).toBe(
      '0.64.0-updated',
    );
    expect(inv.plugins.find((p) => p.name === 'old-notes')?.version).toBe('1.0.0-updated');
    expect(inv.plugins.find((p) => p.name === 'missing-pack')?.version).toBe('2.0.0');
    expect(inv.plugins.find((p) => p.name === 'git-tools')?.version).toBe('0.5.0-updated');
  });
});
