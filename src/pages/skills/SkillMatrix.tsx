import { useEffect, useState, type ReactNode } from 'react';
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Circle,
  Copy,
  Eye,
  FolderOpen,
  Link2,
  Minus,
  Share2,
  Trash2,
} from 'lucide-react';
import { AgentDot } from '@/components/shared/AgentDot';
import { useI18n } from '@/components/shared/LanguageProvider';
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
import { AGENTS, agentDisplayName } from '@/config/agents';
import {
  isMappedState,
  mapCoreSkill,
  type InstalledSkillDto,
  type SkillCopyLocation,
} from '@/lib/api/skill';
import { isCapabilityUsable } from '@/lib/capability';
import type { AgentColumn } from '@/lib/hooks/useInstalledAgents';
import type { AgentKey, Skill, SkillMapStatus, SkillSyncState } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';
import { skillCellTip, sharedRootPresence } from './copy';
import type { SkillPreviewCopy, SkillPreviewTarget } from './SkillMarkdownPreviewPanel';

/** checkbox 固定；技能名 / 共享根 / Agent 列可拖（Agent 列共用宽度，矩阵对齐） */
const CHECK_COL_W = 40;
const MATRIX_WIDTH_SPECS: ColumnWidthSpec<'skill' | 'shared' | 'agent'>[] = [
  { key: 'skill', defaultWidth: 192, minWidth: 120 },
  { key: 'shared', defaultWidth: 152, minWidth: 128 },
  { key: 'agent', defaultWidth: 96, minWidth: 72 },
];

const COLUMN_WIDTHS_STORAGE_KEY = StorageKey.skillsMatrixColumnWidths;

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
  t: TranslateFn,
  agentName: string,
  state: SkillSyncState,
  mapStatus: SkillMapStatus,
  linkKind?: string,
  reason?: string,
): string {
  return skillCellTip(t, agentName, state, mapStatus, linkKind, reason);
}

export function catalogRowKey(
  row: Pick<InstalledSkillDto, 'origin' | 'id' | 'contentHash'>,
): string {
  if (row.origin === 'shared') return `shared:${row.id}`;
  const hash = row.contentHash?.trim();
  if (hash) return `private:${row.id}:${hash}`;
  return `private:${row.origin}:${row.id}`;
}

export function isSharedCatalogRow(row: InstalledSkillDto): boolean {
  return row.origin === 'shared';
}

/** 表头用共享真源短路径；优先 catalog 行上的 rootLabel。 */
export function sharedRootColumnLabel(
  rows: Array<Pick<InstalledSkillDto, 'origin' | 'rootLabel'>>,
  t?: TranslateFn,
): string {
  const labeled = rows.find((row) => row.origin === 'shared' && row.rootLabel.trim());
  return labeled?.rootLabel.trim() || (t ? t('skills.matrix.sharedRoot') : '~/.agents/skills');
}

/** 私有真源行：只在本工具，尚未加入共享库。投影副本不进表。 */
export function isPrivateSourceRow(row: InstalledSkillDto): boolean {
  return row.origin !== 'shared' && row.mapStatus === 'private_source';
}

/** 私有行归属的工具列；必须画在该列，不得塞进第一列（Claude）占位。 */
export function privateRowOriginId(row: InstalledSkillDto): AgentKey | null {
  return privateRowCopies(row)[0]?.agentId ?? null;
}

export function privateRowCopies(row: InstalledSkillDto): SkillCopyLocation[] {
  if (!isPrivateSourceRow(row)) return [];
  if (row.copies && row.copies.length > 0) return row.copies;
  return [
    {
      agentId: row.origin as AgentKey,
      sourceDir: row.sourceDir,
      rootDir: row.rootDir,
      rootLabel: row.rootLabel,
    },
  ];
}

function privateGroupKey(row: InstalledSkillDto): string {
  const hash = row.contentHash?.trim();
  if (hash) return `${row.id}\0${hash}`;
  return `${row.id}\0origin:${row.origin}`;
}

function mappedProjectionCopies(row: InstalledSkillDto): SkillPreviewCopy[] {
  return (row.projections ?? [])
    .filter((proj) => proj.state === 'linked' || proj.state === 'copied')
    .map((proj) => ({
      agentId: proj.agent,
      sourceDir: proj.targetDir ?? proj.resolvedTarget ?? '',
    }));
}

export function previewTargetFromCatalogRow(
  row: InstalledSkillDto,
  agentId?: AgentKey,
): SkillPreviewTarget {
  if (isSharedCatalogRow(row)) {
    const copies = mappedProjectionCopies(row);
    const selected =
      agentId && copies.some((copy) => copy.agentId === agentId) ? agentId : null;
    const loc = copies.find((copy) => copy.agentId === selected);
    return {
      skillId: row.id,
      name: row.name,
      sourceDir: loc?.sourceDir || row.sourceDir,
      libraryDir: row.sourceDir,
      privateAgent: selected,
      copies,
      includeShared: true,
      rowKey: catalogRowKey(row),
    };
  }
  const copies = privateRowCopies(row).map((copy) => ({
    agentId: copy.agentId,
    sourceDir: copy.sourceDir,
  }));
  const selected =
    (agentId && copies.some((copy) => copy.agentId === agentId) ? agentId : undefined) ??
    copies[0]?.agentId ??
    (row.origin as AgentKey);
  const loc = copies.find((copy) => copy.agentId === selected);
  return {
    skillId: row.id,
    name: row.name,
    sourceDir: loc?.sourceDir ?? row.sourceDir,
    privateAgent: selected,
    copies,
    includeShared: false,
    rowKey: catalogRowKey(row),
  };
}

function mergePrivateGroup(members: InstalledSkillDto[]): InstalledSkillDto {
  const primary = members[0]!;
  const copies: SkillCopyLocation[] = members.map((member) => ({
    agentId: member.origin as AgentKey,
    sourceDir: member.sourceDir,
    rootDir: member.rootDir,
    rootLabel: member.rootLabel,
  }));
  return { ...primary, copies };
}

/** 所有状态格同一盒子，避免勾/圈/横杠对不齐。 */
const STATUS_GLYPH_CLASS =
  'relative inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-btn';

/**
 * 本地表可见行：共享矩阵 + 私有占位（不含已在共享库 / 冲突副本行）。
 * 传入 `visibleAgentIds` 时，丢掉当前工具列里没有的私有真源行。
 * 同 id 且 contentHash 相同的私有副本合成一行（`copies` 列出各物理文件）。
 */
export function visibleCatalogRows(
  rows: InstalledSkillDto[],
  visibleAgentIds?: Iterable<string>,
): InstalledSkillDto[] {
  const visible =
    visibleAgentIds === undefined
      ? null
      : visibleAgentIds instanceof Set
        ? visibleAgentIds
        : new Set(visibleAgentIds);

  const eligible = rows.filter((row) => {
    if (isSharedCatalogRow(row)) return true;
    if (!isPrivateSourceRow(row)) return false;
    return visible === null || visible.has(row.origin);
  });

  const emitted = new Set<string>();
  const out: InstalledSkillDto[] = [];
  for (const row of eligible) {
    if (isSharedCatalogRow(row)) {
      out.push(row.copies ? { ...row, copies: [] } : row);
      continue;
    }
    const key = privateGroupKey(row);
    if (emitted.has(key)) continue;
    emitted.add(key);
    const members = eligible.filter(
      (item) => isPrivateSourceRow(item) && privateGroupKey(item) === key,
    );
    out.push(mergePrivateGroup(members));
  }
  return out;
}

export function catalogRowHasMapped(row: InstalledSkillDto): boolean {
  return (row.projections ?? []).some((p) => isMappedState(p.state));
}

export function catalogRowHasConflict(row: InstalledSkillDto): boolean {
  return (row.projections ?? []).some((p) => p.mapStatus === 'conflict');
}

function asMatrixSkill(row: InstalledSkillDto): Skill {
  return mapCoreSkill({
    id: row.id,
    name: row.name,
    description: row.description,
    sourceDir: row.sourceDir,
    projections: row.projections ?? [],
  });
}

function isRealDescription(name: string, description?: string): boolean {
  const desc = description?.trim() ?? '';
  return !!desc && !/^[|>][+\-]?\d*$/.test(desc) && !desc.endsWith(' 技能') && desc !== `${name} 技能`;
}

interface SkillMatrixProps {
  /** catalog 行（共享 + 合成后的私有占位）；React key 为 catalogRowKey */
  rows: InstalledSkillDto[];
  selected: Set<string>;
  allSelected: boolean;
  pendingCells: Set<string>;
  importingIds: Set<string>;
  onToggleSelect: (skillId: string) => void;
  onToggleSelectAll: () => void;
  onCellClick: (skill: Skill, agentId: AgentKey) => void;
  /** Right-click a projection cell: persist link/copy, or disable. */
  onCellProject?: (
    skill: Skill,
    agentId: AgentKey,
    mode: 'link' | 'copy' | 'disable',
  ) => void;
  /** 预览本地 SKILL.md；私有行可带上要点亮的那一份 Agent */
  onPreview?: (row: InstalledSkillDto, agentId?: AgentKey) => void;
  /** 当前预览行 key（catalogRowKey），与 checkbox selected 分离 */
  activeKey?: string | null;
  onAdopt: (skillId: string, agentId: AgentKey, name: string) => void;
  onOpenDir?: (path: string) => void;
  onDeleteShared?: (row: InstalledSkillDto) => void;
  onDeleteFromTool?: (skillId: string, agentId: AgentKey, name: string) => void;
  /**
   * 矩阵列：推荐传入「已安装」Agent（含不支持 skills 的，如 Kimi）。
   * 灰色单元格用后端 mapStatus 解释；未安装列仅在调用方显式传入时出现。
   */
  agents?: AgentColumn[];
  /** 已安装的 agent id 集合；缺省视为 props.agents 全部已安装 */
  installedAgentIds?: ReadonlySet<AgentKey> | AgentKey[];
}

function StatusGlyph({
  hint,
  children,
  disabled,
  busy,
  onClick,
  onContextMenu,
}: {
  hint: string;
  children: ReactNode;
  disabled?: boolean;
  busy?: boolean;
  onClick?: () => void;
  onContextMenu?: (e: { preventDefault: () => void; clientX: number; clientY: number }) => void;
}) {
  if (onClick || onContextMenu) {
    return (
      <Hint label={hint}>
        <button
          type="button"
          disabled={disabled || busy}
          onClick={onClick}
          onContextMenu={
            onContextMenu
              ? (e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  onContextMenu(e);
                }
              : undefined
          }
          aria-label={hint}
          className={cn(
            STATUS_GLYPH_CLASS,
            'transition-colors',
            disabled && !onContextMenu
              ? 'cursor-not-allowed text-disabled'
              : 'cursor-pointer hover:bg-hover',
            busy && 'opacity-50',
          )}
        >
          {children}
        </button>
      </Hint>
    );
  }
  return (
    <Hint label={hint}>
      <span tabIndex={0} className={STATUS_GLYPH_CLASS} aria-label={hint}>
        {children}
      </span>
    </Hint>
  );
}

function SharedRootCell({
  inLibrary,
  label,
  privateRow,
  importing,
  onAdopt,
  onContextMenu,
}: {
  inLibrary: boolean;
  label: string;
  privateRow: boolean;
  importing: boolean;
  onAdopt?: () => void;
  onContextMenu?: (e: { preventDefault: () => void; clientX: number; clientY: number }) => void;
}) {
  const { t } = useI18n();
  if (privateRow && onAdopt) {
    const hint = importing ? t('skills.workspace.adoptBusy') : t('skills.workspace.adoptHint');
    return (
      <StatusGlyph hint={hint} busy={importing} onClick={onAdopt} onContextMenu={onContextMenu}>
        <Share2 className={cn('h-3.5 w-3.5', importing && 'animate-pulse')} />
      </StatusGlyph>
    );
  }
  const hint = sharedRootPresence(t, inLibrary, label);
  return (
    <StatusGlyph hint={hint} onContextMenu={onContextMenu}>
      {inLibrary ? (
        <Check className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />
      ) : (
        <Minus className="h-3.5 w-3.5 text-muted/50" />
      )}
    </StatusGlyph>
  );
}

function PrivateOriginCell({
  agentId,
  onClick,
  onContextMenu,
}: {
  agentId: AgentKey;
  onClick?: () => void;
  onContextMenu?: (e: { preventDefault: () => void; clientX: number; clientY: number }) => void;
}) {
  const { t } = useI18n();
  const agentName = agentDisplayName(agentId);
  const hint = skillCellTip(t, agentName, 'absent', 'private_source');
  return (
    <StatusGlyph hint={hint} onClick={onClick} onContextMenu={onContextMenu}>
      <AgentDot agentId={agentId} title={null} className="h-3.5 w-3.5" />
    </StatusGlyph>
  );
}

/** 矩阵图标图例（默认折叠，记住用户选择） */
export function SkillMatrixLegend({ className }: { className?: string }) {
  const { t } = useI18n();
  const [open, setOpen] = useState(() => loadBool(StorageKey.skillsMatrixLegendOpen));

  useEffect(() => {
    saveBool(StorageKey.skillsMatrixLegendOpen, open);
  }, [open]);

  const items: {
    key: string;
    icon: ReactNode;
    label: string;
    hint: string;
  }[] = [
    {
      key: 'linked',
      icon: <Link2 className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />,
      label: t('skills.legend.linkedLabel'),
      hint: t('skills.legend.linkedHint'),
    },
    {
      key: 'copied',
      icon: <Check className="h-3.5 w-3.5 text-success" strokeWidth={2.5} />,
      label: t('skills.legend.copiedLabel'),
      hint: t('skills.legend.copiedHint'),
    },
    {
      key: 'absent',
      icon: <Circle className="h-3.5 w-3.5 text-muted" strokeWidth={1.8} />,
      label: t('skills.legend.absentLabel'),
      hint: t('skills.legend.absentHint'),
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
      label: t('skills.legend.conflictLabel'),
      hint: t('skills.legend.conflictHint'),
    },
    {
      key: 'blocked',
      icon: <Minus className="h-3.5 w-3.5 text-muted/50" />,
      label: t('skills.legend.blockedLabel'),
      hint: t('skills.legend.blockedHint'),
    },
  ];

  return (
    <div className={cn('text-meta', className)} role="note" aria-label={t('skills.matrix.legendAria')}>
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
        {t('skills.legend.toggle')}
      </button>
      {open ? (
        <div className="mt-1.5 rounded-card border border-border/60 bg-subtle/40 px-3 py-2">
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
          <p className="mt-1.5 text-muted">{t('skills.legend.footer')}</p>
        </div>
      ) : null}
    </div>
  );
}

/** 技能 × 工具同步矩阵 — lucide 图标 + sticky 表头；视觉协议见 ui/table */
export function SkillMatrix({
  rows,
  selected,
  allSelected,
  pendingCells,
  importingIds,
  onToggleSelect,
  onToggleSelectAll,
  onCellClick,
  onCellProject,
  onPreview,
  activeKey = null,
  onAdopt,
  onOpenDir,
  onDeleteShared,
  onDeleteFromTool,
  agents,
  installedAgentIds,
  showLegend = true,
}: SkillMatrixProps & { showLegend?: boolean }) {
  const { t } = useI18n();
  const columns: AgentColumn[] =
    agents ??
    // Fallback for Storybook/tests only — real pages pass useInstalledAgents().
    AGENTS.map((a) => ({ ...a }));
  const installedSet =
    installedAgentIds instanceof Set
      ? installedAgentIds
      : new Set<AgentKey>(installedAgentIds ?? columns.map((a) => a.id));

  const { widths, onResizeStart, onResizeKeyDown } = useColumnWidths(
    MATRIX_WIDTH_SPECS,
    COLUMN_WIDTHS_STORAGE_KEY,
  );
  const sharedRootLabel = sharedRootColumnLabel(rows, t);
  const tableMinWidth =
    CHECK_COL_W +
    widths.skill +
    widths.shared +
    widths.agent * Math.max(columns.length, 1);
  const [rowMenu, setRowMenu] = useState<{
    x: number;
    y: number;
    row: InstalledSkillDto;
  } | null>(null);
  const [cellMenu, setCellMenu] = useState<{
    x: number;
    y: number;
    skill: Skill;
    agentId: AgentKey;
    state: SkillSyncState;
    folderPath: string | null;
  } | null>(null);
  const [folderMenu, setFolderMenu] = useState<{
    x: number;
    y: number;
    path: string;
    sharedRow?: InstalledSkillDto;
    fromTool?: { skillId: string; agentId: AgentKey; name: string };
  } | null>(null);

  const openFolder = (path: string | null | undefined) => {
    const next = path?.trim();
    if (!next || !onOpenDir) return;
    onOpenDir(next);
  };

  return (
    <div className="space-y-2" data-help="skills-matrix">
      <TableShell>
        <Table className="table-fixed" style={{ minWidth: tableMinWidth }}>
          <colgroup>
            <col style={{ width: CHECK_COL_W }} />
            <col style={{ width: widths.skill }} />
            <col style={{ width: widths.shared }} />
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
                  aria-label={t('skills.matrix.selectAll')}
                />
              </TableHead>
              <TableHead className="relative select-none">
                {t('skills.matrix.skillName')}
                <ColumnResizeHandle
                  columnKey="skill"
                  label={t('skills.matrix.skillName')}
                  onResizeStart={onResizeStart}
                    onResizeKeyDown={onResizeKeyDown}
                />
              </TableHead>
              <TableHead className="relative select-none text-center">
                <span className="font-mono text-meta whitespace-nowrap">{sharedRootLabel}</span>
                <ColumnResizeHandle
                  columnKey="shared"
                  label={sharedRootLabel}
                  onResizeStart={onResizeStart}
                    onResizeKeyDown={onResizeKeyDown}
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
                    label={t('skills.matrix.tool')}
                    onResizeStart={onResizeStart}
                    onResizeKeyDown={onResizeKeyDown}
                  />
                </TableHead>
              ))}
            </TableHeaderRow>
          </TableHeader>
          <TableBody>
            {rows.map((row) => {
              const realDesc = isRealDescription(row.name, row.description);
              const privateRow = isPrivateSourceRow(row);
              const copies = privateRowCopies(row);
              const originId = copies[0]?.agentId ?? null;
              const copyIds = new Set(copies.map((copy) => copy.agentId));
              const canContext = Boolean(onPreview);
              const skill = privateRow ? null : asMatrixSkill(row);
              const rowActive = Boolean(activeKey && activeKey === catalogRowKey(row));
              const openRowMenu = canContext
                ? (e: { preventDefault: () => void; clientX: number; clientY: number }) => {
                    e.preventDefault();
                    setCellMenu(null);
                    setFolderMenu(null);
                    setRowMenu({
                      x: e.clientX,
                      y: e.clientY,
                      row,
                    });
                  }
                : undefined;
              return (
                <TableRow key={catalogRowKey(row)} active={rowActive}>
                  <TableCell>
                    {privateRow ? null : (
                      <input
                        type="checkbox"
                        className="h-3.5 w-3.5 accent-accent"
                        checked={selected.has(row.id)}
                        onChange={() => onToggleSelect(row.id)}
                        aria-label={t('skills.matrix.selectSkill', { name: row.name })}
                      />
                    )}
                  </TableCell>
                  <TableCell className="min-w-0" onContextMenu={openRowMenu}>
                    <div className="min-w-0">
                      {onPreview ? (
                        <button
                          type="button"
                          className={cn(
                            'block max-w-full truncate text-left text-sm font-medium text-primary',
                            'rounded-btn focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/60',
                          )}
                          onClick={() => onPreview(row)}
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') {
                              e.preventDefault();
                              onPreview(row);
                            }
                          }}
                        >
                          <Tip className="truncate" label={row.name}>
                            {row.name}
                          </Tip>
                        </button>
                      ) : (
                        <Tip className="truncate font-medium text-primary" label={row.name}>
                          {row.name}
                        </Tip>
                      )}
                      {realDesc ? (
                        <Tip
                          className="mt-0.5 line-clamp-1 truncate text-sm text-secondary"
                          label={row.description}
                        >
                          {row.description}
                        </Tip>
                      ) : null}
                    </div>
                  </TableCell>
                  <TableCell className="text-center">
                    <SharedRootCell
                      inLibrary={isSharedCatalogRow(row)}
                      label={sharedRootLabel}
                      privateRow={privateRow}
                      importing={copies.some((copy) =>
                        importingIds.has(`${copy.agentId}:${row.id}`),
                      ) || importingIds.has(`${row.origin}:${row.id}`)}
                      onAdopt={
                        privateRow && originId
                          ? () => onAdopt(row.id, originId, row.name)
                          : undefined
                      }
                      onContextMenu={
                        isSharedCatalogRow(row) && row.sourceDir && (onOpenDir || onDeleteShared)
                          ? (e) => {
                              setRowMenu(null);
                              setCellMenu(null);
                              setFolderMenu({
                                x: e.clientX,
                                y: e.clientY,
                                path: row.sourceDir,
                                sharedRow: row,
                              });
                            }
                          : undefined
                      }
                    />
                  </TableCell>
                  {columns.length === 0 ? (
                    <TableCell className="text-xs text-muted" colSpan={1}>
                      {t('skills.matrix.noTools')}
                    </TableCell>
                  ) : (
                    columns.map((agent) => {
                      if (privateRow) {
                        const isOrigin = copyIds.has(agent.id);
                        return (
                          <TableCell key={agent.id} className="text-center">
                            {isOrigin ? (
                              <PrivateOriginCell
                                agentId={agent.id}
                                onClick={
                                  onPreview ? () => onPreview(row, agent.id) : undefined
                                }
                                onContextMenu={
                                  onOpenDir || onDeleteFromTool
                                    ? (e) => {
                                        const loc = copies.find((copy) => copy.agentId === agent.id);
                                        const path = loc?.sourceDir ?? row.sourceDir;
                                        if (!path) return;
                                        setRowMenu(null);
                                        setCellMenu(null);
                                        setFolderMenu({
                                          x: e.clientX,
                                          y: e.clientY,
                                          path,
                                          fromTool: {
                                            skillId: row.id,
                                            agentId: agent.id,
                                            name: row.name,
                                          },
                                        });
                                      }
                                    : undefined
                                }
                              />
                            ) : (
                              <StatusGlyph
                                hint={sharedRootPresence(
                                  t,
                                  false,
                                  sharedRootLabel,
                                )}
                              >
                                <Minus className="h-3.5 w-3.5 text-muted/50" />
                              </StatusGlyph>
                            )}
                          </TableCell>
                        );
                      }
                      const matrixSkill = skill!;
                      const state = matrixSkill.projectionByAgent[agent.id] ?? 'unsupported';
                      const proj = matrixSkill.projections?.find((p) => p.agent === agent.id);
                      const agentInstalled = installedSet.has(agent.id);
                      const skillsCap = agent.capabilities?.skills;
                      const mapStatus = resolveMapStatus(
                        state,
                        proj?.mapStatus,
                        agentInstalled,
                        isCapabilityUsable(skillsCap),
                      );
                      const cellKey = `${row.id}:${agent.id}`;
                      const hasConflict =
                        mapStatus === 'conflict' ||
                        ((state === 'foreign' || state === 'conflict') &&
                          matrixSkill.conflicts.includes(agent.id));
                      // conflict 仍可点；unsupported / not installed / target unavailable 不可点
                      const blocked =
                        mapStatus === 'agent_unsupported' ||
                        mapStatus === 'agent_not_installed' ||
                        mapStatus === 'target_unavailable' ||
                        mapStatus === 'private_source' ||
                        state === 'unsupported';
                      const pending = pendingCells.has(cellKey);
                      const title = cellTitle(
                        t,
                        agent.name,
                        state,
                        mapStatus,
                        proj?.linkKind,
                        skillsCap?.reason ?? undefined,
                      );
                      return (
                        <TableCell key={agent.id} className="text-center">
                          <StatusGlyph
                            hint={title}
                            disabled={blocked || pending}
                            busy={pending}
                            onClick={
                              blocked
                                ? undefined
                                : () => onCellClick(matrixSkill, agent.id)
                            }
                            onContextMenu={
                              (!blocked && !pending && (onCellProject || onOpenDir))
                                ? (e) => {
                                    setRowMenu(null);
                                    setFolderMenu(null);
                                    setCellMenu({
                                      x: e.clientX,
                                      y: e.clientY,
                                      skill: matrixSkill,
                                      agentId: agent.id,
                                      state,
                                      folderPath:
                                        proj?.targetDir ?? proj?.resolvedTarget ?? null,
                                    });
                                  }
                                : undefined
                            }
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
                          </StatusGlyph>
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
              onPreview(rowMenu.row);
              setRowMenu(null);
            }}
          >
            <Eye className="h-3.5 w-3.5" />
            {t('skills.menu.preview')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem
          disabled={!rowMenu?.row.sourceDir || !onOpenDir}
          onSelect={() => {
            if (!rowMenu) return;
            openFolder(rowMenu.row.sourceDir);
            setRowMenu(null);
          }}
        >
          <FolderOpen className="h-3.5 w-3.5" />
          {t('skills.menu.openFolder')}
        </ContextMenuItem>
        {onDeleteShared && rowMenu && isSharedCatalogRow(rowMenu.row) ? (
          <ContextMenuItem
            onSelect={() => {
              if (!rowMenu) return;
              onDeleteShared(rowMenu.row);
              setRowMenu(null);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('skills.menu.deleteShared')}
          </ContextMenuItem>
        ) : null}
      </ContextMenu>
      <ContextMenu
        open={cellMenu !== null}
        point={cellMenu}
        onClose={() => setCellMenu(null)}
      >
        {onCellProject ? (
          <>
            <ContextMenuItem
              disabled={cellMenu?.state === 'linked'}
              onSelect={() => {
                if (!cellMenu || !onCellProject) return;
                onCellProject(cellMenu.skill, cellMenu.agentId, 'link');
                setCellMenu(null);
              }}
            >
              <Link2 className="h-3.5 w-3.5" />
              {t('skills.menu.enableLink')}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={cellMenu?.state === 'copied'}
              onSelect={() => {
                if (!cellMenu || !onCellProject) return;
                onCellProject(cellMenu.skill, cellMenu.agentId, 'copy');
                setCellMenu(null);
              }}
            >
              <Copy className="h-3.5 w-3.5" />
              {t('skills.menu.enableCopy')}
            </ContextMenuItem>
            <ContextMenuItem
              disabled={!cellMenu || !isMappedState(cellMenu.state)}
              onSelect={() => {
                if (!cellMenu || !onCellProject) return;
                onCellProject(cellMenu.skill, cellMenu.agentId, 'disable');
                setCellMenu(null);
              }}
            >
              <Minus className="h-3.5 w-3.5" />
              {t('skills.menu.disable')}
            </ContextMenuItem>
          </>
        ) : null}
        {onDeleteFromTool &&
        (cellMenu?.state === 'foreign' || cellMenu?.state === 'conflict') ? (
          <ContextMenuItem
            onSelect={() => {
              if (!cellMenu || !onDeleteFromTool) return;
              onDeleteFromTool(cellMenu.skill.id, cellMenu.agentId, cellMenu.skill.name);
              setCellMenu(null);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('skills.menu.deleteFromTool')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem
          disabled={!cellMenu?.folderPath || !onOpenDir}
          onSelect={() => {
            if (!cellMenu) return;
            openFolder(cellMenu.folderPath);
            setCellMenu(null);
          }}
        >
          <FolderOpen className="h-3.5 w-3.5" />
          {t('skills.menu.openFolder')}
        </ContextMenuItem>
      </ContextMenu>
      <ContextMenu
        open={folderMenu !== null}
        point={folderMenu}
        onClose={() => setFolderMenu(null)}
      >
        {onDeleteFromTool && folderMenu?.fromTool ? (
          <ContextMenuItem
            onSelect={() => {
              if (!folderMenu?.fromTool || !onDeleteFromTool) return;
              onDeleteFromTool(
                folderMenu.fromTool.skillId,
                folderMenu.fromTool.agentId,
                folderMenu.fromTool.name,
              );
              setFolderMenu(null);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('skills.menu.deleteFromTool')}
          </ContextMenuItem>
        ) : null}
        <ContextMenuItem
          onSelect={() => {
            if (!folderMenu) return;
            openFolder(folderMenu.path);
            setFolderMenu(null);
          }}
        >
          <FolderOpen className="h-3.5 w-3.5" />
          {t('skills.menu.openFolder')}
        </ContextMenuItem>
        {onDeleteShared && folderMenu?.sharedRow ? (
          <ContextMenuItem
            onSelect={() => {
              if (!folderMenu?.sharedRow || !onDeleteShared) return;
              onDeleteShared(folderMenu.sharedRow);
              setFolderMenu(null);
            }}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('skills.menu.deleteShared')}
          </ContextMenuItem>
        ) : null}
      </ContextMenu>
    </div>
  );
}
