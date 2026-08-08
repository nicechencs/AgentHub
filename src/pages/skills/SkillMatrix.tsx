import { useEffect, useState, type ReactNode } from 'react';
import { AlertTriangle, Check, ChevronDown, Circle, Eye, FolderOpen, Link2, Minus } from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { ContextMenu, ContextMenuItem } from '@/components/ui/context-menu';
import {
  ColumnResizeHandle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
  TableShell,
  useColumnWidths,
  type ColumnWidthSpec,
} from '@/components/ui/table';
import { Hint, Tip } from '@/components/ui/tooltip';
import { AGENTS } from '@/config/agents';
import { isMappedState } from '@/lib/api/skill';
import { isCapabilityUsable } from '@/lib/capability';
import { normalizeOpenPath } from '@/lib/path-open';
import { sharedSkillActiveKey } from '@/lib/skills/preview-keys';
import type { AgentColumn } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, Skill, SkillMapStatus, SkillSyncState } from '@/lib/types';
import { cn } from '@/lib/utils';
import { skillsCopy } from './copy';

/** checkbox 固定；技能名 / Agent 列可拖（Agent 列共用宽度，矩阵对齐） */
const CHECK_COL_W = 40;
const MATRIX_WIDTH_SPECS: ColumnWidthSpec<'skill' | 'agent'>[] = [
  { key: 'skill', defaultWidth: 192, minWidth: 120 },
  { key: 'agent', defaultWidth: 96, minWidth: 72 },
];

const LEGEND_STORAGE_KEY = 'agenthub.skills.matrixLegendOpen';

function readLegendOpen(): boolean {
  if (typeof window === 'undefined') return false;
  try {
    return window.localStorage.getItem(LEGEND_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

/** 表头短名:Claude Code → Claude */
function shortName(name: string) {
  return name.split(' ')[0];
}

function resolveMapStatus(
  state: SkillSyncState,
  mapStatus: SkillMapStatus | undefined,
  agentInstalled: boolean,
  agentSkillsCapable: boolean,
): SkillMapStatus {
  // detect 安装状态由后端 agent list 提供，不是前端猜能力
  if (!agentInstalled) return 'agent_not_installed';
  // 投影 mapStatus 优先（Rust 适配器真实判断）
  if (mapStatus) return mapStatus;
  if (!agentSkillsCapable || state === 'unsupported') return 'agent_unsupported';
  if (state === 'foreign' || state === 'conflict') return 'conflict';
  return 'available';
}

function cellTitle(
  agentName: string,
  state: SkillSyncState,
  mapStatus: SkillMapStatus,
  linkKind?: string,
  reason?: string,
): string {
  return skillsCopy.cell.tip(agentName, state, mapStatus, linkKind, reason);
}

interface SkillMatrixProps {
  skills: Skill[];
  selected: Set<string>;
  allSelected: boolean;
  pendingCells: Set<string>;
  onToggleSelect: (skillId: string) => void;
  onToggleSelectAll: () => void;
  onCellClick: (skill: Skill, agentId: AgentId) => void;
  /** 打开技能真源目录（sourceDir） */
  onOpenDir?: (path: string) => void;
  /** 预览本地 SKILL.md（Markdown） */
  onPreview?: (skill: Skill) => void;
  /** 当前预览复合 key（`shared:id`），与 checkbox selected 分离 */
  activeKey?: string | null;
  /**
   * 矩阵列：推荐传入「已安装」Agent（含不支持 skills 的，如 Kimi）。
   * 灰色单元格用后端 mapStatus 解释；未安装列仅在调用方显式传入时出现。
   */
  agents?: AgentColumn[];
  /** 已安装的 agent id 集合；缺省视为 props.agents 全部已安装 */
  installedAgentIds?: ReadonlySet<AgentId> | AgentId[];
}

/** 矩阵图标图例（默认折叠，记住用户选择） */
export function SkillMatrixLegend({ className }: { className?: string }) {
  const [open, setOpen] = useState(readLegendOpen);

  useEffect(() => {
    try {
      window.localStorage.setItem(LEGEND_STORAGE_KEY, open ? '1' : '0');
    } catch {
      // ignore
    }
  }, [open]);

  const L = skillsCopy.legend.items;
  const items: {
    key: string;
    icon: ReactNode;
    label: string;
    hint: string;
  }[] = [
    {
      key: 'linked',
      icon: <Link2 className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />,
      label: L.linked.label,
      hint: L.linked.hint,
    },
    {
      key: 'copied',
      icon: <Check className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />,
      label: L.copied.label,
      hint: L.copied.hint,
    },
    {
      key: 'absent',
      icon: <Circle className="h-3.5 w-3.5 text-muted" strokeWidth={1.8} />,
      label: L.absent.label,
      hint: L.absent.hint,
    },
    {
      key: 'conflict',
      icon: (
        <span className="relative inline-flex h-3.5 w-3.5 items-center justify-center">
          <Circle className="h-3.5 w-3.5 text-muted" strokeWidth={1.8} />
          <AlertTriangle
            className="absolute -right-1 -top-1 h-2.5 w-2.5 text-warning"
            strokeWidth={2.5}
          />
        </span>
      ),
      label: L.conflict.label,
      hint: L.conflict.hint,
    },
    {
      key: 'blocked',
      icon: <Minus className="h-3.5 w-3.5 text-muted/50" />,
      label: L.blocked.label,
      hint: L.blocked.hint,
    },
  ];

  return (
    <div className={cn('text-2xs', className)} role="note" aria-label="技能启用状态图例">
      <button
        type="button"
        className="inline-flex items-center gap-1 text-muted transition-colors hover:text-secondary"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <ChevronDown
          className={cn('h-3.5 w-3.5 transition-transform', !open && '-rotate-90')}
          aria-hidden
        />
        {skillsCopy.legend.toggle}
      </button>
      {open ? (
        <div className="mt-1.5 rounded-btn border border-border/60 bg-subtle/40 px-3 py-2">
          <ul className="flex flex-wrap gap-x-4 gap-y-1.5">
            {items.map((item) => (
              <li key={item.key} className="flex min-w-[10rem] max-w-xs items-start gap-1.5">
                <span
                  className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center"
                  aria-hidden
                >
                  {item.icon}
                </span>
                <span className="min-w-0 leading-snug">
                  <span className="font-medium text-primary">{item.label}</span>
                  <span className="text-muted"> — {item.hint}</span>
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-1.5 text-muted">{skillsCopy.legend.footer}</p>
        </div>
      ) : null}
    </div>
  );
}

/** 技能 × 工具同步矩阵 — lucide 图标 + sticky 表头；视觉协议见 ui/table */
export function SkillMatrix({
  skills,
  selected,
  allSelected,
  pendingCells,
  onToggleSelect,
  onToggleSelectAll,
  onCellClick,
  onOpenDir,
  onPreview,
  activeKey = null,
  agents,
  installedAgentIds,
  showLegend = true,
}: SkillMatrixProps & { showLegend?: boolean }) {
  const columns: AgentColumn[] =
    agents ??
    // Fallback for Storybook/tests only — real pages pass useInstalledAgents().
    AGENTS.map((a) => ({ ...a }));
  const installedSet =
    installedAgentIds instanceof Set
      ? installedAgentIds
      : new Set<AgentId>(installedAgentIds ?? columns.map((a) => a.id));

  const { widths, onResizeStart } = useColumnWidths(MATRIX_WIDTH_SPECS);
  const tableMinWidth =
    CHECK_COL_W + widths.skill + widths.agent * Math.max(columns.length, 1);
  const [rowMenu, setRowMenu] = useState<{
    x: number;
    y: number;
    skill: Skill;
    path: string | null;
  } | null>(null);

  return (
    <div className="space-y-2">
      <TableShell>
        <Table className="table-fixed" style={{ minWidth: tableMinWidth }}>
          <colgroup>
            <col style={{ width: CHECK_COL_W }} />
            <col style={{ width: widths.skill }} />
            {columns.map((agent) => (
              <col key={agent.id} style={{ width: widths.agent }} />
            ))}
          </colgroup>
          <TableHeader>
            <TableHeaderRow sticky>
              <TableHead className="w-10">
                <input
                  type="checkbox"
                  className="h-3.5 w-3.5 accent-accent"
                  checked={allSelected}
                  onChange={onToggleSelectAll}
                  aria-label="全选"
                />
              </TableHead>
              <TableHead className="relative select-none">
                技能名
                <ColumnResizeHandle
                  columnKey="skill"
                  label="技能名"
                  onResizeStart={onResizeStart}
                />
              </TableHead>
              {columns.map((agent) => (
                <TableHead
                  key={agent.id}
                  className="relative select-none text-center"
                >
                  <span className="inline-flex items-center gap-1.5">
                    <AgentDot agentId={agent.id} color={agent.color} />
                    {shortName(agent.name)}
                  </span>
                  {/* 工具列共用宽度：拖任一表头即同步全部工具列 */}
                  <ColumnResizeHandle
                    columnKey="agent"
                    label="工具"
                    onResizeStart={onResizeStart}
                  />
                </TableHead>
              ))}
            </TableHeaderRow>
          </TableHeader>
          <TableBody>
            {skills.map((skill) => {
              // Hide placeholder / mis-parsed YAML block markers (`|`, `>`).
              const desc = skill.description?.trim() ?? '';
              const realDesc =
                !!desc &&
                !/^[|>][+\-]?\d*$/.test(desc) &&
                !desc.endsWith(' 技能') &&
                desc !== `${skill.name} 技能`;
              const openPath = normalizeOpenPath(skill.sourceDir);
              const canOpenDir = Boolean(openPath && onOpenDir);
              const canContext = Boolean(onPreview) || canOpenDir;
              const rowActive = Boolean(activeKey && activeKey === sharedSkillActiveKey(skill.id));
              return (
                <TableRow key={skill.id} active={rowActive}>
                  <TableCell>
                    <input
                      type="checkbox"
                      className="h-3.5 w-3.5 accent-accent"
                      checked={selected.has(skill.id)}
                      onChange={() => onToggleSelect(skill.id)}
                      aria-label={`选择 ${skill.name}`}
                    />
                  </TableCell>
                  <TableCell
                    className="min-w-0"
                    onContextMenu={
                      canContext
                        ? (e) => {
                            e.preventDefault();
                            setRowMenu({
                              x: e.clientX,
                              y: e.clientY,
                              skill,
                              path: canOpenDir ? openPath : null,
                            });
                          }
                        : undefined
                    }
                  >
                    <div className="min-w-0">
                      {onPreview ? (
                        <button
                          type="button"
                          className={cn(
                            'block max-w-full truncate text-left text-sm font-medium text-primary',
                            'rounded-btn focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                          )}
                          onClick={() => onPreview(skill)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              onPreview(skill);
                            }
                          }}
                        >
                          <Tip className="truncate" label={skill.name}>
                            {skill.name}
                          </Tip>
                        </button>
                      ) : (
                        <Tip className="truncate font-medium text-primary" label={skill.name}>
                          {skill.name}
                        </Tip>
                      )}
                      {realDesc ? (
                        <Tip
                          className="mt-0.5 line-clamp-1 truncate text-sm text-secondary"
                          label={skill.description}
                        >
                          {skill.description}
                        </Tip>
                      ) : null}
                    </div>
                  </TableCell>
                  {columns.length === 0 ? (
                    <TableCell className="text-xs text-muted" colSpan={1}>
                      暂无可用工具
                    </TableCell>
                  ) : (
                    columns.map((agent) => {
                      const state = skill.sync[agent.id] ?? 'unsupported';
                      const proj = skill.projections?.find((p) => p.agent === agent.id);
                      const agentInstalled = installedSet.has(agent.id);
                      const skillsCap = agent.capabilities?.skills;
                      const mapStatus = resolveMapStatus(
                        state,
                        proj?.mapStatus,
                        agentInstalled,
                        isCapabilityUsable(skillsCap),
                      );
                      const cellKey = `${skill.id}:${agent.id}`;
                      const hasConflict =
                        mapStatus === 'conflict' ||
                        ((state === 'foreign' || state === 'conflict') &&
                          skill.conflicts.includes(agent.id));
                      // conflict 仍可点；unsupported / not installed / target unavailable 不可点
                      const blocked =
                        mapStatus === 'agent_unsupported' ||
                        mapStatus === 'agent_not_installed' ||
                        mapStatus === 'target_unavailable' ||
                        mapStatus === 'private_source' ||
                        state === 'unsupported';
                      const clickable = !blocked && !pendingCells.has(cellKey);
                      const title = cellTitle(
                        agent.name,
                        state,
                        mapStatus,
                        proj?.linkKind,
                        skillsCap?.reason ?? undefined,
                      );
                      return (
                        <TableCell
                          key={agent.id}
                          className="text-center"
                        >
                          <Hint label={title}>
                            <button
                              type="button"
                              disabled={!clickable}
                              onClick={() => onCellClick(skill, agent.id)}
                              aria-label={title}
                              className={cn(
                                'relative inline-flex h-8 min-w-8 items-center justify-center rounded-btn px-1.5 transition-colors',
                                clickable && 'cursor-pointer hover:bg-hover',
                                blocked && 'cursor-not-allowed text-disabled',
                                pendingCells.has(cellKey) && 'opacity-50',
                              )}
                            >
                              {state === 'linked' && !blocked && (
                                <Link2 className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />
                              )}
                              {state === 'copied' && !blocked && (
                                <Check className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />
                              )}
                              {!blocked &&
                                (state === 'absent' ||
                                  state === 'foreign' ||
                                  state === 'conflict') && (
                                  <>
                                    <Circle className="h-3.5 w-3.5 text-muted" strokeWidth={1.8} />
                                    {hasConflict && (
                                      <AlertTriangle
                                        className="absolute -right-0.5 -top-0.5 h-2.5 w-2.5 text-warning"
                                        strokeWidth={2.5}
                                      />
                                    )}
                                  </>
                                )}
                              {blocked && <Minus className="h-3.5 w-3.5 text-muted/50" />}
                              {isMappedState(state) &&
                                !blocked &&
                                proj?.linkKind &&
                                proj.linkKind !== 'none' && (
                                  <span className="sr-only">{proj.linkKind}</span>
                                )}
                            </button>
                          </Hint>
                        </TableCell>
                      );
                    })
                  )}
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </TableShell>
      {showLegend ? <SkillMatrixLegend /> : null}

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
        {rowMenu?.path && onOpenDir ? (
          <ContextMenuItem
            onSelect={() => {
              if (!rowMenu?.path || !onOpenDir) return;
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
