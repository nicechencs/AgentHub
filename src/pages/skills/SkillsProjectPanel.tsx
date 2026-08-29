import { FolderKanban, PackageSearch, Trash2 } from 'lucide-react';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
} from '@/components/ui/table';
import { TableSkeleton } from '@/components/ui/skeleton';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { InstalledSkillDto } from '@/lib/api/skill';
import { cn } from '@/lib/utils';
import type { ProjectSkillOption } from './skills-project-model';
import { filterProjectSkillRows, projectSkillRowKey } from './skills-project-model';

export type SkillsProjectPanelProps = {
  options: ProjectSkillOption[];
  workspacePath: string | null;
  onWorkspaceChange: (path: string) => void;
  projectsLoading: boolean;
  projectsError: unknown | null;
  onRetryProjects: () => void;
  search: string;
  onSearchChange: (v: string) => void;
  rows: InstalledSkillDto[] | null;
  loading: boolean;
  error: unknown | null;
  onRetry: () => void;
  activeKey: string | null;
  onPreview: (row: InstalledSkillDto) => void;
  onDelete: (row: InstalledSkillDto) => void;
};

export function SkillsProjectPanel(props: SkillsProjectPanelProps) {
  const {
    options,
    workspacePath,
    onWorkspaceChange,
    projectsLoading,
    projectsError,
    onRetryProjects,
    search,
    onSearchChange,
    rows,
    loading,
    error,
    onRetry,
    activeKey,
    onPreview,
    onDelete,
  } = props;
  const { t } = useI18n();
  const selected = workspacePath && options.some((o) => o.workspacePath === workspacePath)
    ? workspacePath
    : '';
  const filtered = filterProjectSkillRows(rows ?? [], search);

  if (projectsError !== null) {
    return <ErrorState error={projectsError} onRetry={onRetryProjects} />;
  }
  if (projectsLoading && options.length === 0) {
    return <TableSkeleton rows={6} cols={3} />;
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <Select
          value={selected || undefined}
          onValueChange={onWorkspaceChange}
          disabled={options.length === 0}
        >
          <SelectTrigger className="w-72 max-w-full" aria-label={t('skills.filters.projectAria')}>
            <SelectValue placeholder={t('skills.filters.projectPlaceholder')} />
          </SelectTrigger>
          <SelectContent className="min-w-[18rem]">
            {options.map((option) => (
              <SelectItem
                key={option.workspacePath}
                value={option.workspacePath}
                textValue={option.label}
              >
                <span className="flex min-w-0 flex-col">
                  <span className="truncate">{option.label}</span>
                  <span className="truncate text-meta text-muted">{option.subtitle}</span>
                </span>
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <SearchField
          className="w-64"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('skills.filters.searchPlaceholder')}
          disabled={!workspacePath}
        />
      </div>

      {options.length === 0 ? (
        <EmptyState
          icon={FolderKanban}
          title={t('skills.empty.noProjectsTitle')}
          description={t('skills.empty.noProjectsDesc')}
        />
      ) : !workspacePath ? (
        <EmptyState
          icon={FolderKanban}
          title={t('skills.empty.pickProjectTitle')}
          description={t('skills.empty.pickProjectDesc')}
        />
      ) : error !== null ? (
        <ErrorState error={error} onRetry={onRetry} />
      ) : loading ? (
        <TableSkeleton rows={6} cols={3} />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title={
            search
              ? t('skills.empty.noMatchTitle')
              : t('skills.empty.emptyProjectTitle')
          }
          description={
            search
              ? t('skills.empty.noMatchFilter')
              : t('skills.empty.emptyProjectDesc')
          }
          action={
            search ? (
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => onSearchChange('')}
              >
                {t('skills.empty.clearFilter')}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <TableShell>
          <Table>
            <TableHeader>
              <TableHeaderRow sticky>
                <TableHead>{t('skills.matrix.skillName')}</TableHead>
                <TableHead>{t('skills.filters.location')}</TableHead>
                <TableHead className="w-24 text-right">{t('skills.market.colActions')}</TableHead>
              </TableHeaderRow>
            </TableHeader>
            <TableBody>
              {filtered.map((row) => {
                const key = projectSkillRowKey(row);
                const active = activeKey === key;
                return (
                  <TableRow key={key} active={active}>
                    <TableCell>
                      <button
                        type="button"
                        className={cn(
                          'block min-w-0 text-left',
                          active ? 'text-primary' : 'text-primary hover:underline',
                        )}
                        onClick={() => onPreview(row)}
                      >
                        <span className="block truncate font-medium">{row.name}</span>
                        {row.description ? (
                          <span className="mt-0.5 block truncate text-meta text-muted">
                            {row.description}
                          </span>
                        ) : null}
                      </button>
                    </TableCell>
                    <TableCell className="font-mono text-meta text-muted">
                      {row.rootLabel}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        size="sm"
                        variant="ghost"
                        className="text-danger"
                        onClick={() => onDelete(row)}
                        aria-label={t('skills.workspace.removeAria')}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        {t('skills.workspace.delete')}
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </TableShell>
      )}
    </div>
  );
}
