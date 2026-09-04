import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { disablePlugin, enablePlugin, listPluginInventory } from '@/lib/api/plugins';

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
});
