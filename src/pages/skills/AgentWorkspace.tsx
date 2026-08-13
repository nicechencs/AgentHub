import { useMemo, useRef, useEffect, useState } from 'react';
import { Eye, FolderOpen, Share2, Trash2 } from 'lucide-react';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { SearchField } from '@/components/shared/SearchField';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Button } from '@/components/ui/button';
import { ContextMenu, ContextMenuItem } from '@/components/ui/context-menu';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableFooterBar,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { actionCountClass } from '@/components/ui/segmented-styles';
import { Hint, Tip } from '@/components/ui/tooltip';
import { AGENT_MAP, type AgentMeta, agentDisplayName } from '@/config/agents';
import {
  canAdoptWorkspaceSkill,
  isPrivateInstalledOrigin,
  resolveWorkspacePresence,
  type InstalledSkillDto,
} from '@/lib/api/skill';
import { normalizeOpenPath } from '@/lib/path-open';
import { privateSkillActiveKey } from '@/lib/skills/preview-keys';
import type { AgentColumn } from '@/lib/hooks/useInstalledAgents';
import type { AgentId } from '@/lib/types';
import { cn } from '@/lib/utils';
import { skillsCopy } from './copy';

export type WorkspaceAgentFilter = 'all' | AgentId;

type PresenceFilter = 'all' | 'private_only' | 'in_library' | 'conflict';

type ColumnKey = 'check' | 'name' | 'agent' | 'status' | 'actions';

const WIDTH_SPECS: ColumnWidthSpec<ColumnKey>[] = [
  { key: 'check', defaultWidth: 44, minWidth: 36 },
  { key: 'name', defaultWidth: 240, minWidth: 120 },
  { key: 'agent', defaultWidth: 120, minWidth: 88 },
  { key: 'status', defaultWidth: 140, minWidth: 96 },
  { key: 'actions', defaultWidth: 120, minWidth: 96 },
];

const COLUMN_LABELS: Record<ColumnKey, string> = {
  check: '选择',
  name: '技能名',
  agent: 'Agent',
  status: '状态',
  actions: '操作',
};

const PRESENCE_FILTERS: { id: PresenceFilter; label: string }[] = [
  { id: 'all', label: '全部' },
  { id: 'private_only', label: '只在本工具' },
  { id: 'in_library', label: '已在共享库' },
  { id: 'conflict', label: '内容不同' },
];

function originDisplayName(origin: string): string {
  if (origin === 'shared') return '共享库';
  return agentDisplayName(origin as AgentId);
}

function presenceLabel(presence: ReturnType<typeof resolveWorkspacePresence>): string {
  if (presence === 'private_only') return '只在本工具';
  if (presence === 'in_library') return '已在共享库';
  if (presence === 'conflict') return '内容不同';
  return '—';
}

export interface AgentWorkspaceProps {
  installed: InstalledSkillDto[];
  installedAgents: AgentColumn[];
  loading?: boolean;
  importingIds: Set<string>;
  onOpenDir: (path: string) => void;
  /** 预览本地 SKILL.md */
  onPreview?: (skill: InstalledSkillDto) => void;
  /** 当前预览复合 key（`agent:id:skill`），与 checkbox selected 分离 */
  activeKey?: string | null;
  onAdopt: (skillId: string, agentId: AgentId, name: string) => void;
  onUninstall: (
    skillId: string,
    agentId: AgentId,
    name: string,
    inLibrary: boolean,
  ) => void;
  onBatchAdopt: (items: { skillId: string; agentId: AgentId; name: string }[]) => void;
  batchAdopting?: boolean;
}

export function AgentWorkspace({
  installed,
  installedAgents,
  importingIds,
  onOpenDir,
  onPreview,
  activeKey = null,
  onAdopt,
  onUninstall,
  onBatchAdopt,
  batchAdopting = false,
}: AgentWorkspaceProps) {
  const [agentFilter, setAgentFilter] = useState<WorkspaceAgentFilter>('all');
  const [presenceFilter, setPresenceFilter] = useState<PresenceFilter>('private_only');
  const [search, setSearch] = useState('');
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [rowMenu, setRowMenu] = useState<{
    x: number;
    y: number;
    skill: InstalledSkillDto;
    path: string | null;
  } | null>(null);
  const { widths, onResizeStart, totalWidth } = useColumnWidths(WIDTH_SPECS);

  /** Agent 工作区行（非 shared origin） */
  const workspaceSkills = useMemo(
    () => installed.filter((s) => isPrivateInstalledOrigin(s.origin)),
    [installed],
  );

  const counts = useMemo(() => {
    let privateOnly = 0;
    let inLibrary = 0;
    let conflict = 0;
    const privateByAgent = new Map<string, number>();
    const diskByAgent = new Map<string, number>();
    for (const s of workspaceSkills) {
      const p = resolveWorkspacePresence(s.origin, s.mapStatus);
      diskByAgent.set(s.origin, (diskByAgent.get(s.origin) ?? 0) + 1);
      if (p === 'private_only') {
        privateOnly++;
        privateByAgent.set(s.origin, (privateByAgent.get(s.origin) ?? 0) + 1);
      } else if (p === 'in_library') {
        inLibrary++;
      } else if (p === 'conflict') {
        conflict++;
      }
    }
    return { privateOnly, inLibrary, conflict, privateByAgent, diskByAgent };
  }, [workspaceSkills]);

  const filtered = useMemo(() => {
    const keyword = search.trim().toLowerCase();
    return workspaceSkills.filter((s) => {
      if (agentFilter !== 'all' && s.origin !== agentFilter) return false;
      const presence = resolveWorkspacePresence(s.origin, s.mapStatus);
      if (presenceFilter === 'private_only' && presence !== 'private_only') return false;
      if (presenceFilter === 'in_library' && presence !== 'in_library') return false;
      if (presenceFilter === 'conflict' && presence !== 'conflict') return false;
      if (keyword) {
        const hay = `${s.name} ${s.id} ${s.description} ${s.rootLabel}`.toLowerCase();
        if (!hay.includes(keyword)) return false;
      }
      return true;
    });
  }, [workspaceSkills, agentFilter, presenceFilter, search]);

  const rowKey = (s: InstalledSkillDto) => `${s.origin}:${s.id}`;

  const selectableKeys = filtered
    .filter((s) => canAdoptWorkspaceSkill(s.origin, s.mapStatus))
    .map(rowKey);

  const allSelectableSelected =
    selectableKeys.length > 0 && selectableKeys.every((k) => selected.has(k));
  const someSelectableSelected =
    selectableKeys.length > 0 && selectableKeys.some((k) => selected.has(k));

  const selectAllRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    const el = selectAllRef.current;
    if (!el) return;
    el.indeterminate = someSelectableSelected && !allSelectableSelected;
  }, [someSelectableSelected, allSelectableSelected]);

  const toggleSelect = (key: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  /** 全选当前列表中「可加入共享库」的行（已在共享库的行不可选） */
  const toggleSelectAll = () => {
    if (selectableKeys.length === 0) return;
    if (allSelectableSelected) {
      setSelected((prev) => {
        const next = new Set(prev);
        for (const k of selectableKeys) next.delete(k);
        return next;
      });
    } else {
      setSelected((prev) => {
        const next = new Set(prev);
        for (const k of selectableKeys) next.add(k);
        return next;
      });
    }
  };

  const selectedItems = filtered
    .filter((s) => selected.has(rowKey(s)) && canAdoptWorkspaceSkill(s.origin, s.mapStatus))
    .map((s) => ({
      skillId: s.id,
      agentId: s.origin as AgentId,
      name: s.name,
    }));

  /** 已安装 ∪ 磁盘上仍有技能目录的 agent（保持产品序） */
  const tabAgents = useMemo(() => {
    const byId = new Map<AgentId, AgentMeta>();
    for (const a of installedAgents) byId.set(a.id, a);
    for (const origin of counts.diskByAgent.keys()) {
      if (origin === 'shared' || byId.has(origin as AgentId)) continue;
      const meta = AGENT_MAP[origin as AgentId];
      if (meta) byId.set(meta.id, meta);
    }
    return Array.from(byId.values());
  }, [installedAgents, counts.diskByAgent]);

  const W = skillsCopy.workspace;

  /** 仅琥珀：可加入共享库的数量；0 不占位，避免双数字噪音 */
  const privateBadge = (privateCount: number) =>
    privateCount > 0 ? (
      <Tip className={actionCountClass} label={W.privateTabTip}>
        {privateCount}
      </Tip>
    ) : null;

  return (
    <div className="space-y-3">
      {/* 工具总览：仅琥珀角标 = 可加入共享库；汇总见筛选分段数字，不占常驻说明行 */}
      <AgentTabStrip
        showAll
        value={agentFilter}
        onChange={setAgentFilter}
        agents={tabAgents}
        aria-label="按工具筛选"
        emptyLabel="尚未发现任何 Agent 技能目录"
        renderEnd={(id) => {
          // 琥珀 pill：行动提示（可加入共享库），非普通计数排版
          if (id === 'all') return privateBadge(counts.privateOnly);
          return privateBadge(counts.privateByAgent.get(id) ?? 0);
        }}
      />

      <div className="flex flex-wrap items-center gap-3">
        <SearchField
          className="w-64"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="搜索技能名…"
        />
        <SegmentedControl
          value={presenceFilter}
          onChange={setPresenceFilter}
          aria-label="存在状态过滤"
          options={PRESENCE_FILTERS.map((f) => {
            const count =
              f.id === 'all'
                ? workspaceSkills.length
                : f.id === 'private_only'
                  ? counts.privateOnly
                  : f.id === 'in_library'
                    ? counts.inLibrary
                    : f.id === 'conflict'
                      ? counts.conflict
                      : 0;
            return {
              value: f.id,
              label: f.label,
              count,
            };
          })}
        />
        {selectedItems.length > 0 ? (
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <span className="text-xs text-muted">{W.selectedBar(selectedItems.length)}</span>
            <Button
              size="sm"
              variant="secondary"
              disabled={batchAdopting}
              onClick={() => onBatchAdopt(selectedItems)}
            >
              <Share2 className={cn('h-3.5 w-3.5', batchAdopting && 'animate-pulse')} />
              {batchAdopting ? W.batchAdoptBusy : W.batchAdopt}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
              {skillsCopy.filters.clearSelection}
            </Button>
          </div>
        ) : null}
      </div>

      {filtered.length === 0 ? (
        <EmptyState
          icon={Share2}
          title={
            presenceFilter === 'private_only' && counts.privateOnly === 0
              ? W.emptyPrivateTitle
              : W.emptyFilterTitle
          }
          description={
            presenceFilter === 'private_only' && counts.privateOnly === 0
              ? W.emptyPrivateDesc(counts.inLibrary)
              : W.emptyFilterDesc
          }
          actionLabel={
            search || presenceFilter !== 'private_only' || agentFilter !== 'all'
              ? W.resetFilter
              : counts.inLibrary > 0
                ? W.viewInLibrary
                : undefined
          }
          onAction={
            search || presenceFilter !== 'private_only' || agentFilter !== 'all'
              ? () => {
                  setSearch('');
                  setPresenceFilter('private_only');
                  setAgentFilter('all');
                }
              : counts.inLibrary > 0
                ? () => setPresenceFilter('in_library')
                : undefined
          }
        />
      ) : (
        <TableShell
          footer={
            <TableFooterBar>
              <span className="text-muted">
                {selectableKeys.length > 0
                  ? W.footerSelectable(selectableKeys.length, selectedItems.length)
                  : W.footerNone}
              </span>
              <span>
                显示 {filtered.length}
                {presenceFilter === 'private_only'
                  ? ` / 只在本工具 ${counts.privateOnly}`
                  : ` / 目录合计 ${workspaceSkills.length}`}
              </span>
            </TableFooterBar>
          }
        >
          <Table className="table-fixed" style={{ minWidth: totalWidth }}>
            <colgroup>
              {WIDTH_SPECS.map((c) => (
                <col key={c.key} style={{ width: widths[c.key] }} />
              ))}
            </colgroup>
            <TableHeader>
              <TableHeaderRow>
                {(Object.keys(COLUMN_LABELS) as ColumnKey[]).map((key) => (
                  <TableHead key={key} className="relative select-none">
                    {key === 'check' ? (
                      <Hint
                        label={
                          selectableKeys.length === 0
                            ? W.selectAllEmpty
                            : W.selectAllHint
                        }
                      >
                        <input
                          ref={selectAllRef}
                          type="checkbox"
                          className="h-3.5 w-3.5 accent-accent"
                          checked={allSelectableSelected}
                          onChange={toggleSelectAll}
                          disabled={selectableKeys.length === 0}
                          aria-label={W.selectAllHint}
                        />
                      </Hint>
                    ) : (
                      COLUMN_LABELS[key]
                    )}
                    <ColumnResizeHandle
                      columnKey={key}
                      label={COLUMN_LABELS[key]}
                      onResizeStart={onResizeStart}
                    />
                  </TableHead>
                ))}
              </TableHeaderRow>
            </TableHeader>
            <TableBody>
              {filtered.map((s) => {
                const key = rowKey(s);
                const presence = resolveWorkspacePresence(s.origin, s.mapStatus);
                const importing = importingIds.has(key);
                const canAdopt = canAdoptWorkspaceSkill(s.origin, s.mapStatus);
                const agentId = s.origin as AgentId;
                const agentMeta = AGENT_MAP[agentId];
                const openPath =
                  normalizeOpenPath(s.sourceDir) ?? normalizeOpenPath(s.rootDir);
                const canContext = Boolean(onPreview) || Boolean(openPath);
                const desc = s.description?.trim() ?? '';
                const realDesc =
                  !!desc &&
                  !/^[|>][+\-]?\d*$/.test(desc) &&
                  !desc.endsWith(' 技能') &&
                  desc !== `${s.name} 技能`;
                const rowActive =
                  Boolean(activeKey) &&
                  activeKey === privateSkillActiveKey(agentId, s.id);
                return (
                  <TableRow
                    key={key}
                    active={rowActive}
                    onContextMenu={
                      canContext
                        ? (e) => {
                            e.preventDefault();
                            setRowMenu({
                              x: e.clientX,
                              y: e.clientY,
                              skill: s,
                              path: openPath,
                            });
                          }
                        : undefined
                    }
                  >
                    <TableCell
                      onClick={(e) => e.stopPropagation()}
                      onContextMenu={(e) => e.stopPropagation()}
                    >
                      <Hint label={canAdopt ? undefined : W.alreadyInLibrary}>
                        <input
                          type="checkbox"
                          className="h-3.5 w-3.5 accent-accent"
                          checked={selected.has(key)}
                          disabled={!canAdopt}
                          onChange={() => toggleSelect(key)}
                          aria-label={
                            canAdopt
                              ? `选择 ${s.name}`
                              : `${s.name} · ${W.alreadyInLibrary}`
                          }
                        />
                      </Hint>
                    </TableCell>
                    <TableCell className="min-w-0">
                      <div className="min-w-0">
                        {onPreview ? (
                          <button
                            type="button"
                            className={cn(
                              'block max-w-full truncate text-left text-sm font-medium text-primary',
                              'rounded-btn focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                            )}
                            onClick={() => onPreview(s)}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') {
                                e.preventDefault();
                                onPreview(s);
                              }
                            }}
                          >
                            <Tip className="truncate" label={s.name}>
                              {s.name}
                            </Tip>
                          </button>
                        ) : (
                          <Tip className="truncate font-medium text-primary" label={s.name}>
                            {s.name}
                          </Tip>
                        )}
                        {realDesc ? (
                          <Tip
                            className="mt-0.5 line-clamp-1 truncate text-sm text-secondary"
                            label={s.description}
                          >
                            {s.description}
                          </Tip>
                        ) : null}
                      </div>
                    </TableCell>
                    <TableCell className="min-w-0">
                      <span className="inline-flex max-w-full items-center gap-1 truncate text-xs text-secondary">
                        {agentMeta ? <AgentDot agentId={agentId} size="sm" /> : null}
                        <span className="truncate">{originDisplayName(s.origin)}</span>
                      </span>
                    </TableCell>
                    <TableCell className="min-w-0">
                      <span className="truncate text-xs text-muted">
                        {presenceLabel(presence)}
                        {s.rootLabel ? ` · ${s.rootLabel}` : ''}
                      </span>
                    </TableCell>
                    <TableCell
                      onClick={(e) => e.stopPropagation()}
                      onContextMenu={(e) => e.stopPropagation()}
                    >
                      <div className="flex flex-wrap items-center gap-1">
                        {canAdopt ? (
                          <Button
                            size="icon"
                            variant="ghost"
                            disabled={importing}
                            aria-label={
                              presence === 'conflict'
                                ? W.adoptConflict
                                : importing
                                  ? W.adoptBusy
                                  : W.adopt
                            }
                            title={
                              presence === 'conflict'
                                ? W.adoptConflictHint
                                : W.adoptHint
                            }
                            onClick={() => onAdopt(s.id, agentId, s.name)}
                          >
                            <Share2 className={cn('h-3.5 w-3.5', importing && 'animate-pulse')} />
                          </Button>
                        ) : (
                          <Button
                            size="icon"
                            variant="ghost"
                            className="text-disabled"
                            disabled
                            aria-label={W.inLibrary}
                            title={W.inLibrary}
                          >
                            <Share2 className="h-3.5 w-3.5" />
                          </Button>
                        )}
                        <Button
                          size="icon"
                          variant="ghost"
                          className="text-danger"
                          aria-label={
                            presence === 'in_library'
                              ? W.removeProjectionAria
                              : W.removeAria
                          }
                          title={
                            presence === 'in_library'
                              ? W.removeProjection
                              : W.remove
                          }
                          onClick={() =>
                            onUninstall(s.id, agentId, s.name, presence === 'in_library')
                          }
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        </TableShell>
      )}

      <ContextMenu
        open={rowMenu !== null}
        point={rowMenu}
        onClose={() => setRowMenu(null)}
      >
        {onPreview ? (
          <ContextMenuItem
            onSelect={() => {
              if (!rowMenu) return;
              onPreview(rowMenu.skill);
              setRowMenu(null);
            }}
          >
            <Eye className="h-3.5 w-3.5" />
            {skillsCopy.menu.preview}
          </ContextMenuItem>
        ) : null}
        {rowMenu?.path ? (
          <ContextMenuItem
            onSelect={() => {
              if (!rowMenu?.path) return;
              onOpenDir(rowMenu.path);
              setRowMenu(null);
            }}
          >
            <FolderOpen className="h-3.5 w-3.5" />
            {skillsCopy.menu.openFolder}
          </ContextMenuItem>
        ) : null}
      </ContextMenu>
    </div>
  );
}
