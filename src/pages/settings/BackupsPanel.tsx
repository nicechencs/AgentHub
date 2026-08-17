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
import { useI18n } from '@/components/shared/LanguageProvider';
import { AGENTS, AGENT_MAP, type AgentMeta, agentDisplayName } from '@/config/agents';
import { createBackup, deleteBackup, listBackups, restoreBackup } from '@/lib/api/backup';
import type { TranslateFn } from '@/lib/i18n';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import type { AgentId, BackupKind, BackupMeta } from '@/lib/types';
import { cn, fmtBytes } from '@/lib/utils';
import { fmtAbsoluteI18n, fmtRelativeI18n } from './settings-format';

const KIND_VARIANT: Record<BackupKind, 'accent' | 'default' | 'warning'> = {
  'auto-switch': 'accent',
  manual: 'default',
  'pre-uninstall': 'warning',
  'pre-restore': 'accent',
  'pre-skill-uninstall': 'warning',
};

function backupKindLabel(kind: BackupKind, t: TranslateFn): string {
  switch (kind) {
    case 'auto-switch':
      return t('settings.backups.kindAutoSwitch');
    case 'manual':
      return t('settings.backups.kindManual');
    case 'pre-uninstall':
      return t('settings.backups.kindPreUninstall');
    case 'pre-restore':
      return t('settings.backups.kindPreRestore');
    case 'pre-skill-uninstall':
      return t('settings.backups.kindPreSkillUninstall');
  }
}

/** 卡片内文件路径最多展示行数 */
const FILE_PREVIEW = 3;

function shortPath(f: string): string {
  const norm = f.replace(/\\/g, '/');
  if (norm.length <= 48) return f;
  const base = norm.split('/').pop() ?? norm;
  return `…/${base}`;
}

export function BackupsPanel() {
  const { toast } = useToast();
  const { t, lang } = useI18n();
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
        title: t('settings.backups.cannotBackup'),
        description: t('settings.backups.notInstalledDesc', { name: agentMeta.name }),
        variant: 'danger',
      });
      return;
    }
    setCreating(true);
    try {
      await createBackup(agentId);
      toast({ title: t('settings.backups.backupCreated'), description: agentMeta.name, variant: 'success' });
      await refresh();
      void reloadAgents();
    } catch (e) {
      toast({ title: t('settings.backups.backupFailed'), description: e instanceof Error ? e.message : String(e), variant: 'danger' });
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
        title: t('settings.backups.restored'),
        description: `${agentDisplayName(target.agentId)} · ${fmtRelativeI18n(target.createdAt, t)}`,
        variant: 'success',
      });
      setRestoreTarget(null);
      await refresh();
    } catch (e) {
      toast({ title: t('settings.backups.restoreFailed'), description: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusyId(null);
    }
  };

  const handleDelete = async (bk: BackupMeta) => {
    setBusyId(bk.id);
    try {
      await deleteBackup(bk.id);
      toast({ title: t('settings.backups.deleted') });
      await refresh();
    } catch (e) {
      toast({ title: t('settings.backups.deleteFailed'), description: e instanceof Error ? e.message : String(e), variant: 'danger' });
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
            <p className="text-sm">{t('settings.backups.enabledTitle')}</p>
            <p className="mt-0.5 text-xs text-muted">
              {t('settings.backups.enabledDesc')}
            </p>
          </div>
        </div>
        {agentMeta && (
          <Button
            disabled={creating || !isInstalled}
            title={!isInstalled ? t('settings.backups.createTitleNotInstalled') : undefined}
            onClick={() => void handleCreate()}
          >
            <Plus className="h-4 w-4" />
            {creating
              ? t('settings.backups.creating')
              : isInstalled
                ? t('settings.backups.backupAgent', { name: agentMeta.name.replace(' Code', '') })
                : t('settings.backups.notInstalled')}
          </Button>
        )}
      </div>

      <p className="mb-4 text-xs text-muted">
        {t('settings.backups.pageHint')}
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
            emptyLabel={t('settings.backups.emptyAgents')}
          />
        )}
        {!pageLoading && agentMeta && agentId && (
          <span className="text-xs text-muted">
            {t('settings.backups.recordCount', { name: agentMeta.name, count: counts[agentId] ?? 0 })}
            {!isInstalled && t('settings.backups.uninstalledHint')}
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
          title={t('settings.backups.noManageable')}
          description={t('settings.backups.installFirst')}
        />
      ) : !agentMeta || !agentId ? (
        <BackupsSkeleton />
      ) : items.length === 0 ? (
        <EmptyState
          icon={Database}
          title={t('settings.backups.noBackups', { name: agentMeta.name })}
          description={
            isInstalled
              ? t('settings.backups.noBackupsInstalled')
              : t('settings.backups.noBackupsUninstalled')
          }
          actionLabel={
            isInstalled
              ? t('settings.backups.backupAgent', { name: agentMeta.name.replace(' Code', '') })
              : undefined
          }
          onAction={isInstalled ? () => void handleCreate() : undefined}
        />
      ) : (
        <div className="flex flex-col gap-2.5">
          {items.map((bk) => {
            const kind = { label: backupKindLabel(bk.kind, t), variant: KIND_VARIANT[bk.kind] };
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
                        {fmtRelativeI18n(bk.createdAt, t)}
                      </span>
                      <span className="text-xs text-muted tabular-nums">
                        {fmtAbsoluteI18n(bk.createdAt, lang)}
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
                          <li className="text-xs text-muted">{t('settings.backups.moreFiles', { count: moreFiles })}</li>
                        )}
                      </ul>
                    ) : (
                      <p className="mt-1.5 text-xs text-muted">{t('settings.backups.noFileList')}</p>
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
                      {t('common.restore')}
                    </Button>
                    <Button
                      variant="ghost"
                      size="default"
                      className="hover:text-danger"
                      disabled={busyId !== null}
                      onClick={() => void handleDelete(bk)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      {t('common.delete')}
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
            <DialogTitle>{t('settings.backups.restoreTitle')}</DialogTitle>
            <DialogDescription>
              {restoreTarget &&
                t('settings.backups.restoreDesc', {
                  name: agentDisplayName(restoreTarget.agentId),
                  when: fmtAbsoluteI18n(restoreTarget.createdAt, lang),
                  kind: backupKindLabel(restoreTarget.kind, t),
                })}
            </DialogDescription>
          </DialogHeader>
          <p className="text-sm text-secondary">
            {t('settings.backups.restoreConfirm')}
          </p>
          <DialogFooter>
            <Button variant="secondary" disabled={busyId !== null} onClick={() => setRestoreTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button disabled={busyId !== null} onClick={() => void handleRestore()}>
              {busyId !== null ? t('settings.backups.restoring') : t('settings.backups.confirmRestore')}
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
