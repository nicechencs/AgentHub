import { describe, expect, it } from 'vitest';
import type { McpSourceFile } from '@/lib/backend/contracts/mcp-types';
import { isVisibleMcpSource, visibleMcpSources } from './mcp-sources';

function source(partial: Partial<McpSourceFile> & Pick<McpSourceFile, 'path'>): McpSourceFile {
  return {
    agent: 'claude',
    exists: false,
    readable: false,
    serverCount: 0,
    label: '探测 mcp.json',
    ...partial,
  };
}

describe('visibleMcpSources', () => {
  it('hides probe paths that do not exist and have no read error', () => {
    expect(isVisibleMcpSource(source({ path: '/tmp/mcp.json' }))).toBe(false);
    expect(visibleMcpSources([source({ path: '/tmp/mcp.json' })])).toEqual([]);
  });

  it('keeps existing empty configs and unreadable files', () => {
    const empty = source({
      path: '/home/.claude.json',
      exists: true,
      readable: true,
      label: 'Claude 全局',
    });
    const broken = source({
      path: '/home/config.toml',
      exists: true,
      error: 'invalid json',
      label: 'Codex config.toml',
    });
    expect(visibleMcpSources([empty, broken, source({ path: '/tmp/missing.json' })])).toEqual([
      empty,
      broken,
    ]);
  });
});
