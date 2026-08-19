/**
 * MCP inventory façade + mock port contract.
 */
import { beforeEach, describe, expect, it } from 'vitest';
import { resetBackend } from '@/app/runtime';
import { listMcpInventory } from '@/lib/api/mcp';

describe('listMcpInventory (browser mock)', () => {
  beforeEach(() => {
    resetBackend();
  });

  it('returns extracted MCP/server snippets without unrelated config keys', async () => {
    const inv = await listMcpInventory();
    const claude = inv.sources.find((s) => s.agent === 'claude');
    expect(claude?.snippet).toContain('mcpServers');
    expect(claude?.snippet).toContain('filesystem');
    expect(claude?.snippet).not.toMatch(/theme|numStartups|oauthAccount/i);

    const fs = inv.servers.find((s) => s.name === 'filesystem');
    expect(fs?.snippet).toContain('filesystem');
    expect(fs?.snippet).toContain('npx');
    expect(fs?.snippet).not.toContain('docs');

    const docs = inv.servers.find((s) => s.name === 'docs');
    expect(docs?.snippet).toContain('docs');
    expect(docs?.snippet).not.toContain('filesystem');

    const demo = inv.servers.find((s) => s.name === 'demo');
    expect(demo?.snippet).toContain('mcp_servers.demo');
    expect(demo?.snippet).not.toMatch(/^model\s*=/m);
  });
});
