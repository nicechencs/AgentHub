import { PackageSearch } from 'lucide-react';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Button } from '@/components/ui/button';
import { actionCountClass, segmentedCountClass } from '@/components/ui/segmented-styles';
import { TableSkeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { InstalledSkillDto } from '@/lib/api/skill';
import type { AgentColumn } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, Skill } from '@/lib/types';
import { catalogFilters } from './copy';
import { SkillMatrix } from './SkillMatrix';
import type { LocalFilter as LF } from './skills-preview-model';

export type SkillsLibraryPanelProps = {
  error: unknown | null;
  loading: boolean;
  onRetry: () => void;
  search: string;
  onSearchChange: (v: string) => void;
  filter: LF;
  onFilterChange: (v: LF) => void;
  filterCounts: Record<LF, number>;
  selected: Set<string>;
  onClearSelected: () => void;
  batchSyncing: boolean;
  onBatchEnable: () => void;
  filtered: InstalledSkillDto[];
  allSelected: boolean;
  pendingCells: Set<string>;
  importingIds: Set<string>;
  onToggleSelect: (id: string) => void;
  onToggleSelectAll: () => void;
  onCellClick: (skill: Skill, agentId: AgentId) => void;
  onCellProject?: (
    skill: Skill,
    agentId: AgentId,
    mode: 'link' | 'copy' | 'disable',
  ) => void;
  onPreview: (row: InstalledSkillDto, agentId?: AgentId) => void;
  activeKey: string | null;
  onAdopt: (skillId: string, agentId: AgentId, name: string) => void;
  onOpenDir?: (path: string) => void;
  onDeleteShared?: (row: InstalledSkillDto) => void;
  onDeleteFromTool?: (skillId: string, agentId: AgentId, name: string) => void;
  agents: AgentColumn[];
  installedAgentIds: Set<AgentId> | AgentId[];
};

export function SkillsLibraryPanel(props: SkillsLibraryPanelProps) {
  const {
    error, loading, onRetry, search, onSearchChange, filter, onFilterChange,
    filterCounts, selected, onClearSelected, batchSyncing, onBatchEnable,
    filtered, allSelected, pendingCells, importingIds, onToggleSelect, onToggleSelectAll,
    onCellClick, onCellProject, onPreview, activeKey, onAdopt, onOpenDir, onDeleteShared, onDeleteFromTool, agents, installedAgentIds,
  } = props;
  const { t } = useI18n();

  if (error !== null) {
    return <ErrorState error={error} onRetry={onRetry} />;
  }
  if (loading) {
    return <TableSkeleton rows={8} cols={6} />;
  }
  return (
    <>
      <div className="flex flex-wrap items-center gap-3">
        <SearchField
          className="w-64"
          value={search}
          onChange={(e) => onSearchChange(e.target.value)}
          placeholder={t('skills.filters.searchPlaceholder')}
        />
        <SegmentedControl
          value={filter}
          onChange={onFilterChange}
          options={catalogFilters(t).map((f) =>
            f.id === 'private'
              ? {
                  value: f.id,
                  label: (
                    <span className="inline-flex items-center gap-1.5">
                      {f.label}
                      {filterCounts.private > 0 ? (
                        <Tip
                          className={actionCountClass}
                          label={t('skills.tabs.privateBadge', { n: filterCounts.private })}
                        >
                          {filterCounts.private}
                        </Tip>
                      ) : (
                        <span className={segmentedCountClass}>0</span>
                      )}
                    </span>
                  ),
                }
              : {
                  value: f.id,
                  label: f.label,
                  count: filterCounts[f.id],
                },
          )}
          aria-label={t('skills.filters.enableStatusAria')}
        />
        {selected.size > 0 ? (
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <span className="text-xs text-muted">
              {t('skills.filters.selectedCount', { n: selected.size })}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void onBatchEnable()}
              disabled={batchSyncing}
              title={t('skills.filters.batchEnableHint')}
            >
              {batchSyncing
                ? t('skills.filters.batchEnableBusy')
                : t('skills.filters.batchEnable')}
            </Button>
            <Button size="sm" variant="ghost" onClick={onClearSelected}>
              {t('skills.filters.clearSelection')}
            </Button>
          </div>
        ) : null}
      </div>

      {filtered.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title={search || filter !== 'all' ? t('skills.empty.noMatchTitle') : t('skills.empty.emptyLibraryTitle')}
          description={
            search || filter !== 'all'
              ? t('skills.empty.noMatchFilter')
              : t('skills.empty.noMatchLibrary')
          }
          action={
            search || filter !== 'all' ? (
              <Button
                size="sm"
                variant="outline"
                className="mt-2"
                onClick={() => {
                  onSearchChange('');
                  onFilterChange('all');
                }}
              >
                {t('skills.empty.clearFilter')}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <SkillMatrix
          rows={filtered}
          selected={selected}
          allSelected={allSelected}
          pendingCells={pendingCells}
          importingIds={importingIds}
          onToggleSelect={onToggleSelect}
          onToggleSelectAll={onToggleSelectAll}
          onCellClick={onCellClick}
          onCellProject={onCellProject}
          onPreview={onPreview}
          activeKey={activeKey}
          onAdopt={onAdopt}
          onOpenDir={onOpenDir}
          onDeleteShared={onDeleteShared}
          onDeleteFromTool={onDeleteFromTool}
          agents={agents}
          installedAgentIds={installedAgentIds}
        />
      )}
    </>
  );
}
