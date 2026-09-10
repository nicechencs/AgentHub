import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  AGENT_TABLE_COLUMN_SPECS,
  AGENT_TABLE_FLEX_COLUMN,
  agentTableColumnLabel,
  agentTableColumnSide,
  AGENT_TABLE_FIXED_COLUMN_SPECS,
  type AgentTableColumnKey,
} from './agent-table';

describe('agent table columns', () => {
  it('keeps a stable column order without a dedicated hide column', () => {
    expect(AGENT_TABLE_COLUMN_SPECS.map((spec) => spec.key)).toEqual([
      'agent',
      'status',
      'version',
      'note',
      'start',
      'upgrade',
      'actions',
    ]);
  });

  it('uses existing Agent-page words for headers', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    const keys: AgentTableColumnKey[] = [
      'agent',
      'status',
      'version',
      'note',
      'start',
      'upgrade',
      'actions',
    ];
    expect(keys.map((key) => agentTableColumnLabel(key, tZh))).toEqual([
      'Agent',
      '状态',
      '版本',
      '说明',
      '启动',
      '更新',
      '操作',
    ]);
    expect(keys.map((key) => agentTableColumnLabel(key, tEn))).toEqual([
      'Agent',
      'Status',
      'Version',
      'Notes',
      'Start',
      'Update',
      'Actions',
    ]);
  });

  it('keeps the start column wide enough for both launch buttons on one row', () => {
    const start = AGENT_TABLE_COLUMN_SPECS.find((spec) => spec.key === 'start');
    expect(start?.minWidth).toBeGreaterThanOrEqual(240);
    expect(start?.defaultWidth).toBeGreaterThanOrEqual(start?.minWidth ?? 0);
  });

  it('keeps the upgrade column wide enough for a labeled Update button', () => {
    const upgrade = AGENT_TABLE_COLUMN_SPECS.find((spec) => spec.key === 'upgrade');
    expect(upgrade?.defaultWidth).toBe(96);
    expect(upgrade?.minWidth).toBe(80);
  });

  it('lets 说明 fill leftover width; left columns start, right columns end', () => {
    expect(AGENT_TABLE_FLEX_COLUMN).toBe('note');
    expect(AGENT_TABLE_FIXED_COLUMN_SPECS.map((spec) => spec.key)).toEqual([
      'agent',
      'status',
      'version',
      'start',
      'upgrade',
      'actions',
    ]);
    expect(agentTableColumnSide('agent')).toBe('left');
    expect(agentTableColumnSide('status')).toBe('left');
    expect(agentTableColumnSide('version')).toBe('left');
    expect(agentTableColumnSide('note')).toBe('flex');
    expect(agentTableColumnSide('start')).toBe('right');
    expect(agentTableColumnSide('upgrade')).toBe('right');
    expect(agentTableColumnSide('actions')).toBe('right');
  });
});
