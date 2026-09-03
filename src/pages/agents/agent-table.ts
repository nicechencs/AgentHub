/**
 * Agents manage-page table columns.
 */
import type { ColumnWidthSpec } from '@/components/ui/table';
import type { TranslateFn } from '@/lib/i18n';

export type AgentTableColumnKey =
  | 'agent'
  | 'status'
  | 'version'
  | 'note'
  | 'start'
  | 'upgrade'
  | 'hide'
  | 'actions';

export const AGENT_TABLE_FLEX_COLUMN: AgentTableColumnKey = 'note';

export const AGENT_TABLE_COLUMN_SPECS: ColumnWidthSpec<AgentTableColumnKey>[] = [
  { key: 'agent', defaultWidth: 200, minWidth: 140 },
  { key: 'status', defaultWidth: 96, minWidth: 80 },
  { key: 'version', defaultWidth: 120, minWidth: 80 },
  { key: 'note', defaultWidth: 200, minWidth: 120 },
  { key: 'start', defaultWidth: 176, minWidth: 128 },
  { key: 'upgrade', defaultWidth: 64, minWidth: 52 },
  { key: 'hide', defaultWidth: 64, minWidth: 52 },
  { key: 'actions', defaultWidth: 88, minWidth: 64 },
];

export function agentTableColumnLabel(
  key: AgentTableColumnKey,
  t: TranslateFn,
): string {
  switch (key) {
    case 'agent':
      return t('agents.table.agent');
    case 'status':
      return t('agents.table.status');
    case 'version':
      return t('agents.table.version');
    case 'note':
      return t('agents.table.note');
    case 'start':
      return t('agents.table.start');
    case 'upgrade':
      return t('agents.table.upgrade');
    case 'hide':
      return t('agents.table.hide');
    case 'actions':
      return t('agents.table.actions');
  }
}

export const AGENT_TABLE_FIXED_COLUMN_SPECS = AGENT_TABLE_COLUMN_SPECS.filter(
  (spec) => spec.key !== AGENT_TABLE_FLEX_COLUMN,
);

/** 说明左侧靠左，说明右侧靠右；说明本身撑满剩余宽度。 */
export function agentTableColumnSide(
  key: AgentTableColumnKey,
): 'left' | 'flex' | 'right' {
  if (key === AGENT_TABLE_FLEX_COLUMN) return 'flex';
  const noteAt = AGENT_TABLE_COLUMN_SPECS.findIndex((spec) => spec.key === AGENT_TABLE_FLEX_COLUMN);
  const at = AGENT_TABLE_COLUMN_SPECS.findIndex((spec) => spec.key === key);
  return at < noteAt ? 'left' : 'right';
}
