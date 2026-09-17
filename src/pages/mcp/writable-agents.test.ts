import { describe, expect, it } from 'vitest';
import { agentSupportsMcpWrite, MCP_WRITE_AGENTS } from './writable-agents';

describe('MCP write agents', () => {
  it('includes grok alongside the existing write targets', () => {
    expect(MCP_WRITE_AGENTS).toEqual([
      'claude',
      'codex',
      'grok',
      'cursor',
      'workbuddy',
    ]);
    expect(agentSupportsMcpWrite('grok')).toBe(true);
    expect(agentSupportsMcpWrite('kimi')).toBe(false);
  });
});
