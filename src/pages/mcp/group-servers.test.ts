import { describe, expect, it } from 'vitest';
import type { McpServerEntry } from '@/lib/backend/contracts/mcp-types';
import { groupMcpServersByAgentAndFile } from './group-servers';

function server(
  partial: Pick<McpServerEntry, 'agent' | 'name' | 'sourcePath'> & Partial<McpServerEntry>,
): McpServerEntry {
  return {
    transport: 'stdio',
    sourceFormat: 'json',
    ...partial,
  };
}

describe('groupMcpServersByAgentAndFile', () => {
  it('keeps one agent group and splits servers by file', () => {
    const groups = groupMcpServersByAgentAndFile([
      server({ agent: 'claude', name: 'docs', sourcePath: 'C:\\a\\.claude.json' }),
      server({ agent: 'codex', name: 'demo', sourcePath: 'C:\\a\\.codex\\config.toml', sourceFormat: 'toml' }),
      server({ agent: 'claude', name: 'fs', sourcePath: 'C:\\a\\.claude.json' }),
      server({ agent: 'claude', name: 'extra', sourcePath: 'C:\\a\\.claude\\settings.json' }),
    ]);
    expect(groups.map((g) => g.agent)).toEqual(['claude', 'codex']);
    expect(groups[0].files.map((f) => f.sourcePath)).toEqual([
      'C:\\a\\.claude.json',
      'C:\\a\\.claude\\settings.json',
    ]);
    expect(groups[0].files[0].servers.map((s) => s.name)).toEqual(['docs', 'fs']);
    expect(groups[0].servers).toHaveLength(3);
    expect(groups[1].files).toHaveLength(1);
    expect(groups[1].files[0].sourceFormat).toBe('toml');
  });

  it('returns empty for no servers', () => {
    expect(groupMcpServersByAgentAndFile([])).toEqual([]);
  });
});
