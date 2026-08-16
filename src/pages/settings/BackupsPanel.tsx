// Settings「备份」分区：本机配置快照
// Agent Tab = 已安装 ∪ 有备份记录；列表平铺不折叠
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Database, Plus, RotateCcw, Trash2 } from 'lucide-react';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Tip } from '@/components/ui/tooltip';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Skeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { AGENTS, AGENT_MAP, type AgentMeta, agentDisplayName } from '@/config/agents';
import { createBackup, deleteBackup, listBackups, restoreBackup } from '@/lib/api/backup';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, BackupKind, BackupMeta } from '@/lib/types';
import { cn, fmtBytes, fmtRelative } from '@/lib/utils';

const KIND_META: Record<BackupKind, { label: string; variant: 'accent' | 'default' | 'warning' }> = {
  'auto-switch': { label: '切换前自动', variant: 'accent' },
  manual: { label: '手动', variant: 'default' },
  'pre-uninstall': { label: '卸载前', variant: 'warning' },
  'pre-restore': { label: '恢复前自动', variant: 'accent' },
  'pre-skill-uninstall': { label: '技能卸载前', variant: 'warning' },
};

/** 卡片内文件路径最多展示行数 */
const FILE_PREVIEW = 3;

function fmtAbsolute(iso: string): string {
  return new Date(iso).toLocaleString('zh-CN', { hour12: false });
}

function shortPath(f: string): string {
  const norm = f.replace(/\\/g, '/');
  if (norm.length <= 48) return f;
  const base = norm.split('/').pop() ?? norm;
  return `…/${base}`;
}

export function BackupsPanel() {
  const { toast } = useToast();
  const {
    loading: agentsLoading,
    error: agentsError,
    installedIds,
    hiddenIds,
    reload: reloadAgents,
  } = useInstalledAgents();

  const [agentId, setAgentId] = useState<AgentId | null>(null);
  const [backups, setBackups] = useState<BackupMeta[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [restoreTarget, setRestoreTarget] = useState<BackupMeta | null>(null);

  const refresh = useCallback(async () => {
    try {
      setError(null);
      const list = await listBackups();
      setBackups(list);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const counts = useMemo(() => {
    const map = Object.fromEntries(AGENTS.map((a) => [a.id, 0])) as Record<AgentId, number>;
    if (!backups) return map;
    for (const b of backups) {
      map[b.agentId] = (map[b.agentId] ?? 0) + 1;
    }
    return map;
  }, [backups]);

  /** 已安装 ∪ 有备份记录（保持 AGENTS 产品序） */
  const visibleAgents: AgentMeta[] = useMemo(() => {
    const withBackups = new Set(
      (backups ?? []).map((b) => b.agentId).filter(Boolean) as AgentId[],
    );
    const installed = new Set(installedIds);
    return AGENTS.filter(
      (a) =>
        !hiddenIds.includes(a.id) && (installed.has(a.id) || withBackups.has(a.id)),
    );
  }, [backups, installedIds, hiddenIds]);

  // 当前选中不在可见列表时，落到第一个可见 Agent
  useEffect(() => {
    if (agentsLoading || loading) return;
    if (visibleAgents.length === 0) {
      setAgentId(null);
      return;
    }
    if (!agentId || !visibleAgents.some((a) => a.id === agentId)) {
      setAgentId(visibleAgents[0].id);
    }
  }, [agentsLoading, loading, visibleAgents, agentId]);

  const items = useMemo(() => {
    if (!backups || !agentId) return [];
    return backups
      .filter((b) => b.agentId === agentId)
      .sort((a, b) => +new Date(b.createdAt) - +new Date(a.createdAt));
  }, [backups, agentId]);

  const agentMeta = agentId ? AGENT_MAP[agentId] : null;
  const isInstalled = agentId ? installedIds.includes(agentId) : false;
  const pageLoading = loading || agentsLoading;
  const pageError = error ?? agentsError;

  const handleCreate = async () => {
    if (!agentId || !agentMeta) return;
    if (!isInstalled) {
      toast({
        title: '无法备份',
        description: `${agentMeta.name} 未安装，只能查看或恢复已有备份`,
        variant: 'danger',
      });
      return;
    }
    setCreating(true);
    try {
      await createBackup(agentId);
      toast({ title: '备份已创建', description: agentMeta.name, variant: 'success' });
      await refresh();
      void reloadAgents();
    } catch (e) {
      toast({ title: '备份失败', description: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setCreating(false);
    }
  };

  const handleRestore = async () => {
    if (!restoreTarget) return;
    const target = restoreTarget;
    setBusyId(target.id);
    try {
      await restoreBackup(target.id);
      toast({
        title: '已恢复到该备份',
        description: `${agentDisplayName(target.agentId)} · ${fmtRelative(target.createdAt)}`,
        variant: 'success',
      });
      setRestoreTarget(null);
      await refresh();
    } catch (e) {
      toast({ title: '恢复失败', description: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (bk: BackupMeta) => {
    setBusyId(bk.id);
    try {
      await deleteBackup(bk.id);
      toast({ title: '备份已删除' });
      await refresh();
    } catch (e) {
      toast({ title: '删除失败', description: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div>
      {/* 工具条 */}
      <div className="mb-4 flex flex-wrap items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3">
          <div className="min-w-0">
            <p className="text-sm">安全备份已启用</p>
            <p className="mt-0.5 text-xs text-muted">
              切换、导入或更新连接后自动保留当前配置快照
            </p>
          </div>
        </div>
        {agentMeta && (
          <Button
            disabled={creating || !isInstalled}
            title={!isInstalled ? '未安装，无法创建新备份' : undefined}
            onClick={() => void handleCreate()}
          >
            <Plus className="h-4 w-4" />
            {creating
              ? '备份中…'
              : isInstalled
                ? `备份 ${agentMeta.name.replace(' Code', '')}`
                : '未安装'}
          </Button>
        )}
      </div>

      <p className="mb-4 text-xs text-muted">
        备份各 Agent 当前配置，切换账号/供应商出错时可一键恢复。仅列出已安装或仍有备份的
        Agent。
      </p>

      {/* Agent 切换：已安装 ∪ 有备份 */}
      <div className="mb-5 flex flex-wrap items-center gap-3">
        {pageLoading ? (
          <Skeleton className="h-9 w-64 rounded-card" />
        ) : (
          <AgentTabStrip
            value={agentId ?? visibleAgents[0]?.id ?? AGENTS[0].id}
            onChange={setAgentId}
            agents={visibleAgents}
            emptyLabel="暂无已安装或仍有备份的 Agent"
          />
        )}
        {!pageLoading && agentMeta && agentId && (
          <span className="text-xs text-muted">
            {agentMeta.name} · {counts[agentId] ?? 0} 条记录
            {!isInstalled && ' · 已卸载（可恢复备份）'}
          </span>
        )}
      </div>

      {pageLoading ? (
        <BackupsSkeleton />
      ) : pageError ? (
        <ErrorState
          error={pageError}
          onRetry={() => {
            setLoading(true);
            void refresh();
            void reloadAgents();
          }}
        />
      ) : visibleAgents.length === 0 ? (
        <EmptyState
          icon={Database}
          title="无可管理 Agent"
          description="先到 Agents 页安装"
        />
      ) : !agentMeta || !agentId ? (
        <BackupsSkeleton />
      ) : items.length === 0 ? (
        <EmptyState
          icon={Database}
          title={`${agentMeta.name} 暂无备份`}
          description={
            isInstalled
              ? '切换、导入或更新后会自动保留快照，也可点右上角立即备份'
              : '该 Agent 已卸载，且没有可恢复的备份'
          }
          actionLabel={isInstalled ? `备份 ${agentMeta.name.replace(' Code', '')}` : undefined}
          onAction={isInstalled ? () => void handleCreate() : undefined}
        />
      ) : (
        <div className="flex flex-col gap-2.5">
          {items.map((bk) => {
            const kind = KIND_META[bk.kind];
            const busy = busyId === bk.id;
            const files = bk.files ?? [];
            const shownFiles = files.slice(0, FILE_PREVIEW);
            const moreFiles = files.length - shownFiles.length;

            return (
              <Card
                key={bk.id}
                className={cn(
                  'px-4 py-3.5 transition-colors hover:border-border-strong',
                  busy && 'pointer-events-none opacity-60',
                )}
              >
                <div className="flex items-stretch gap-4">
                  <AgentDot
                    agentId={agentId}
                    color={agentMeta.color}
                    size="lg"
                    className="mt-1.5 ring-4 ring-canvas"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                      <Badge variant={kind.variant}>{kind.label}</Badge>
                      <span className="text-sm font-medium tabular-nums">
                        {fmtRelative(bk.createdAt)}
                      </span>
                      <span className="text-xs text-muted tabular-nums">
                        {fmtAbsolute(bk.createdAt)}
                      </span>
                      <span className="text-xs text-muted">·</span>
                      <span className="text-xs text-muted tabular-nums">
                        {fmtBytes(bk.sizeBytes)}
                      </span>
                    </div>

                    {bk.note && <p className="mt-1.5 text-sm text-secondary">{bk.note}</p>}

                    {files.length > 0 ? (
                      <ul className="mt-1.5 space-y-0.5">
                        {shownFiles.map((f) => (
                          <li key={f}>
                            <Tip className="block truncate font-mono text-xs text-muted" label={f}>
                              {shortPath(f)}
                            </Tip>
                          </li>
                        ))}
                        {moreFiles > 0 && (
                          <li className="text-xs text-muted">另有 {moreFiles} 个文件</li>
                        )}
                      </ul>
                    ) : (
                      <p className="mt-1.5 text-xs text-muted">无文件列表</p>
                    )}
                  </div>

                  <div className="flex shrink-0 flex-col justify-center gap-1.5 sm:flex-row sm:items-center">
                    <Button
                      variant="outline"
                      size="default"
                      disabled={busyId !== null}
                      onClick={() => setRestoreTarget(bk)}
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      恢复
                    </Button>
                    <Button
                      variant="ghost"
                      size="default"
                      className="hover:text-danger"
                      disabled={busyId !== null}
                      onClick={() => void handleDelete(bk)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      删除
                    </Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      <Dialog
        open={restoreTarget !== null}
        onOpenChange={(open) => !open && busyId === null && setRestoreTarget(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>恢复备份</DialogTitle>
            <DialogDescription>
              {restoreTarget && (
                <>
                  将恢复 {agentDisplayName(restoreTarget.agentId)} 在{' '}
                  {fmtAbsolute(restoreTarget.createdAt)} 的备份（
                  {KIND_META[restoreTarget.kind].label}）。
                </>
              )}
            </DialogDescription>
          </DialogHeader>
          <p className="text-sm text-secondary">
            恢复前会先备份当前配置。确定恢复到该备份？
          </p>
          <DialogFooter>
            <Button variant="secondary" disabled={busyId !== null} onClick={() => setRestoreTarget(null)}>
              取消
            </Button>
            <Button disabled={busyId !== null} onClick={() => void handleRestore()}>
              {busyId !== null ? '恢复中…' : '确定恢复'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

function BackupsSkeleton() {
  return (
    <div className="flex flex-col gap-2.5">
      {[0, 1, 2].map((i) => (
        <Skeleton key={i} className="h-[5.5rem] w-full rounded-card" />
      ))}
    </div>
  );
}
