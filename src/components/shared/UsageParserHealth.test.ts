import { describe, expect, it } from 'vitest';

import { visibleInstalledIds } from '@/lib/agent-visibility';
import type { AgentStatus } from '@/lib/types';

import {
  DASHBOARD_PARSE_EMPTY,
  filterHealthRowsByVisibleIds,
} from './UsageParserHealth';

function status(
  agentId: AgentStatus['agentId'],
  extra: Partial<AgentStatus> = {},
): AgentStatus {
  return {
    agentId,
    installed: true,
    authStatus: 'none',
    authLabel: '未配置',
    running: false,
    ...extra,
  };
}

describe('UsageParserHealth dashboard filter', () => {
  it('omits uninstalled and hidden agents from footer rows via visibleInstalledIds', () => {
    const visible = visibleInstalledIds([
      status('claude', { installed: false }),
      status('codex', { hidden: true }),
      status('kimi', { installed: false, hidden: true }),
      status('grok', { installed: true }),
    ]);
    const rows = [
      { agentId: 'claude', records: 0 },
      { agentId: 'codex', records: 0 },
      { agentId: 'kimi', records: 0 },
      { agentId: 'grok', records: 19 },
      { agentId: 'pi', records: 0 },
    ];
    expect(filterHealthRowsByVisibleIds(rows, visible).map((row) => row.agentId)).toEqual([
      'grok',
    ]);
  });

  it('shows the empty copy when no agent is installed and visible', () => {
    const visible = visibleInstalledIds([
      status('claude', { installed: false }),
      status('grok', { hidden: true }),
    ]);
    expect(filterHealthRowsByVisibleIds([{ agentId: 'claude' }, { agentId: 'grok' }], visible)).toEqual(
      [],
    );
    expect(DASHBOARD_PARSE_EMPTY).toBe('暂无已安装的 Agent');
  });

  it('does not filter when visible ids are omitted', () => {
    const rows = [{ agentId: 'claude' }, { agentId: 'grok' }];
    expect(filterHealthRowsByVisibleIds(rows, undefined)).toEqual(rows);
  });
});
