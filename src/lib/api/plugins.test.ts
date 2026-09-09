import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import {
  disablePlugin,
  enablePlugin,
  installPlugin,
  listAvailablePlugins,
  listPluginInventory,
  previewPluginInstall,
  uninstallPlugin,
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
      'missing-pack',
      'old-notes',
      'pi-subagents',
    ]);
    expect(inv.plugins.every((p) => p.name !== 'filesystem' && p.name !== 'mcpServers')).toBe(
      true,
    );
    const grok = inv.plugins.find((p) => p.agent === 'grok');
    expect(grok?.components.some((c) => c.kind === 'mcp' && c.name === 'gdrive')).toBe(true);
    expect(inv.agents.find((a) => a.agent === 'claude')?.support).toBe('listed');
    expect(inv.agents.find((a) => a.agent === 'pi')?.support).toBe('listed');
    expect(inv.plugins.find((p) => p.name === 'old-notes')?.requestedVersion).toBe('1.4.0');
    expect(inv.plugins.find((p) => p.name === 'missing-pack')?.path).toBeFalsy();
    expect(inv.agents.find((a) => a.agent === 'codex')?.support).toBe('planned');
    expect(inv.sources?.some((s) => s.agent === 'cursor' && s.sourceKind === 'skills')).toBe(true);
    expect(inv.sources?.some((s) => s.agent === 'dsh' && s.sourceKind === 'cordis')).toBe(true);
  });

  it('round-trips enable then disable for listed Claude and Grok packs', async () => {
    await disablePlugin('claude', 'demo', 'official');
    await enablePlugin('grok', 'gdrive', 'xAI Official');
    let inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'claude')?.enabled).toBe(false);
    expect(inv.plugins.find((p) => p.agent === 'grok')?.enabled).toBe(true);

    await enablePlugin('claude', 'demo', 'official');
    await disablePlugin('grok', 'gdrive', 'xAI Official');
    inv = await listPluginInventory();
    expect(inv.plugins.find((p) => p.agent === 'claude')?.enabled).toBe(true);
    expect(inv.plugins.find((p) => p.agent === 'grok')?.enabled).toBe(false);
  });

  it('rejects enable/disable for planned and unsupported agents', async () => {
    await expect(enablePlugin('codex', 'anything')).rejects.toThrow(/Claude and Grok/);
    await expect(enablePlugin('pi', 'pi-subagents')).rejects.toThrow(/Claude and Grok/);
    await expect(disablePlugin('cursor', 'anything')).rejects.toThrow(/Claude and Grok/);
  });

  it('lists marketplace packs separately from installed inventory', async () => {
    const available = await listAvailablePlugins('grok');
    expect(available.map((p) => p.name)).toEqual(['superpowers']);
    expect(available[0]?.source).toBe('available');
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

    await uninstallPlugin('grok', 'superpowers', 'xAI Official', { keepData: true });
    inv = await listPluginInventory();
    expect(inv.plugins.some((p) => p.name === 'superpowers')).toBe(false);
    expect(inv.plugins.some((p) => p.name === 'gdrive')).toBe(true);
  });

  it('rejects install for Pi and Codex', async () => {
    await expect(installPlugin('pi', 'anything', { confirmed: true })).rejects.toThrow(
      /Claude and Grok/,
    );
    await expect(installPlugin('codex', 'anything', { confirmed: true })).rejects.toThrow(
      /Claude and Grok/,
    );
  });
});
