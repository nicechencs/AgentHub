import type { AgentKey } from '@/lib/types';

/** Agents that accept MCP catalog → probe → write / enable in this slice. */
export const MCP_WRITE_AGENTS: readonly AgentKey[] = [
  'claude',
  'codex',
  'grok',
  'cursor',
  'workbuddy',
];

export function agentSupportsMcpWrite(agent: AgentKey): boolean {
  return (MCP_WRITE_AGENTS as readonly string[]).includes(agent);
}
