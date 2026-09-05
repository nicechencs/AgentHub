import type { ColumnWidthSpec } from '@/components/ui/table';
import type { TranslateFn } from '@/lib/i18n';

export type ProjectCardColumnKey = 'name' | 'path';

export const PROJECT_GROUP_CHEVRON_COL = '2rem';
export const PROJECT_GROUP_HIDE_COL = '2.5rem';

export const PROJECT_CARD_COLUMN_SPECS: ColumnWidthSpec<ProjectCardColumnKey>[] = [
  { key: 'name', defaultWidth: 280, minWidth: 140 },
  { key: 'path', defaultWidth: 180, minWidth: 120 },
];

export const PROJECT_CARD_RESIZE_COLUMNS: ProjectCardColumnKey[] = ['name', 'path'];

export function projectCardColumnLabel(key: ProjectCardColumnKey, t: TranslateFn): string {
  switch (key) {
    case 'name':
      return t('projects.tree.colName');
    case 'path':
      return t('projects.tree.colPath');
  }
}

/** Shared list tracks. Time / sessions / size hug their text; leftover sits after size. */
export function projectGroupListTemplate(widths: Record<ProjectCardColumnKey, number>): string {
  return [
    PROJECT_GROUP_CHEVRON_COL,
    `${widths.name}px`,
    `${widths.path}px`,
    'max-content',
    'max-content',
    'max-content',
    'minmax(0,1fr)',
    PROJECT_GROUP_HIDE_COL,
  ].join(' ');
}
