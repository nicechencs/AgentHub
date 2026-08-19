import { PackageSearch } from 'lucide-react';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SearchField } from '@/components/shared/SearchField';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Button } from '@/components/ui/button';
import { actionCountClass, segmentedCountClass } from '@/components/ui/segmented-styles';
import { TableSkeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import type { InstalledSkillDto } from '@/lib/api/skill';
import type { AgentColumn } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, Skill } from '@/lib/types';
import { skillsCopy } from './copy';
import { SkillMatrix } from './SkillMatrix';
import { FILTERS } from './skills-catalog-model';
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
  onOpenDir: (path: string) => void;
  onPreview: (row: InstalledSkillDto) => void;
  activeKey: string | null;
  onAdopt: (skillId: string, agentId: AgentId, name: string) => void;
  onUninstall: (skillId: string, agentId: AgentId, name: string, inLibrary: boolean) => void;
  agents: AgentColumn[];
  installedAgentIds: Set<AgentId> | AgentId[];
};

export function SkillsLibraryPanel(props: SkillsLibraryPanelProps) {
  const {
    error, loading, onRetry, search, onSearchChange, filter, onFilterChange,
    filterCounts, selected, onClearSelected, batchSyncing, onBatchEnable,
    filtered, allSelected, pendingCells, importingIds, onToggleSelect, onToggleSelectAll,
    onCellClick, onOpenDir, onPreview, activeKey, onAdopt, onUninstall, agents, installedAgentIds,
  } = props;

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
          placeholder={skillsCopy.filters.searchPlaceholder}
        />
        <SegmentedControl
          value={filter}
          onChange={onFilterChange}
          options={FILTERS.map((f) =>
            f.id === 'private'
              ? {
                  value: f.id,
                  label: (
                    <span className="inline-flex items-center gap-1.5">
                      {f.label}
                      {filterCounts.private > 0 ? (
                        <Tip
                          className={actionCountClass}
                          label={skillsCopy.tabs.privateBadge(filterCounts.private)}
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
          aria-label="启用状态过滤"
        />
        {selected.size > 0 ? (
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <span className="text-xs text-muted">
              {skillsCopy.filters.selectedCount(selected.size)}
            </span>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void onBatchEnable()}
              disabled={batchSyncing}
              title={skillsCopy.filters.batchEnableHint}
            >
              {batchSyncing
                ? skillsCopy.filters.batchEnableBusy
                : skillsCopy.filters.batchEnable}
            </Button>
            <Button size="sm" variant="ghost" onClick={onClearSelected}>
              {skillsCopy.filters.clearSelection}
            </Button>
          </div>
        ) : null}
      </div>

      {filtered.length === 0 ? (
        <EmptyState
          icon={PackageSearch}
          title={search || filter !== 'all' ? skillsCopy.empty.noMatchTitle : skillsCopy.empty.emptyLibraryTitle}
          description={
            search || filter !== 'all'
              ? skillsCopy.empty.noMatchFilter
              : skillsCopy.empty.noMatchLibrary
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
                {skillsCopy.empty.clearFilter}
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
          onOpenDir={onOpenDir}
          onPreview={onPreview}
          activeKey={activeKey}
          onAdopt={onAdopt}
          onUninstall={onUninstall}
          agents={agents}
          installedAgentIds={installedAgentIds}
        />
      )}
    </>
  );
}
