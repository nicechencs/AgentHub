import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { listPluginInventory } from '@/lib/api/plugins';

describe('listPluginInventory (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
  });

  it('returns plugin packs rather than MCP server rows', async () => {
    const inv = await listPluginInventory();
    expect(inv.plugins.map((p) => p.name).sort()).toEqual(['demo', 'gdrive']);
    expect(inv.plugins.every((p) => p.name !== 'filesystem' && p.name !== 'mcpServers')).toBe(
      true,
    );
    const grok = inv.plugins.find((p) => p.agent === 'grok');
    expect(grok?.components.some((c) => c.kind === 'mcp' && c.name === 'gdrive')).toBe(true);
    expect(inv.agents.find((a) => a.agent === 'claude')?.support).toBe('listed');
    expect(inv.agents.find((a) => a.agent === 'codex')?.support).toBe('planned');
  });
});
