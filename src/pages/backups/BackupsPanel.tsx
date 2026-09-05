import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import { useNavigate } from 'react-router-dom';
import { Database, Plus, RotateCcw, Trash2 } from 'lucide-react';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { WorkbenchSplitPage } from '@/components/layout/SideSplit';
import { useSideSplit } from '@/components/layout/use-side-split';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { LIST_ROW_PAD, ListRow, ListRowBody } from '@/components/shared/ListRow';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
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
import { getSettings, updateSettings } from '@/lib/api/settings';
import type { TranslateFn } from '@/lib/i18n';
import { Switch } from '@/components/ui/switch';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import type { AgentKey, BackupKind, BackupMeta } from '@/lib/types';
import { BackupDetailPanel } from './backup-detail-panel';
import {
  backupCardIdentity,
  backupFileLabels,
  fmtAbsoluteI18n,
  fmtRelativeI18n,
} from './backup-format';
import { StorageKey } from '@/lib/ui-preferences';

const KIND_VARIANT: Record<BackupKind, 'accent' | 'default' | 'warning'> = {
  'auto-switch': 'accent',
  manual: 'default',
  'pre-uninstall': 'warning',
  'pre-restore': 'accent',
  'pre-skill-uninstall': 'warning',
};

const BACKUPS_INSPECT_WIDTH_KEY = StorageKey.settingsBackupsInspectWidth;

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

export function BackupsPanel({ toolbar }: { toolbar?: ReactNode }) {
  const navigate = useNavigate();
  const { toast } = useToast();
  const { t, lang } = useI18n();
  const {
    loading: agentsLoading,
    error: agentsError,
    installedIds,
    hiddenIds,
    reload: reloadAgents,
  } = useInstalledAgents();

  const [agentId, setAgentId] = useState<AgentKey | null>(null);
  const [backups, setBackups] = useState<BackupMeta[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [keepCopies, setKeepCopies] = useState(true);
  const [creating, setCreating] = useState(false);
  const [restoreTarget, setRestoreTarget] = useState<BackupMeta | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<BackupMeta | null>(null);
  const inspect = useSideSplit<string>({ storageKey: BACKUPS_INSPECT_WIDTH_KEY });

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

  useEffect(() => {
    void getSettings()
      .then((s) => setKeepCopies(s.keepLiveFileCopies !== false))
      .catch(() => undefined);
  }, []);

  const counts = useMemo(() => {
    const map = Object.fromEntries(AGENTS.map((a) => [a.id, 0])) as Record<AgentKey, number>;
    if (!backups) return map;
    for (const b of backups) {
      map[b.agentId] = (map[b.agentId] ?? 0) + 1;
    }
    return map;
  }, [backups]);

  const visibleAgents: AgentMeta[] = useMemo(() => {
    const withBackups = new Set(
      (backups ?? []).map((b) => b.agentId).filter(Boolean) as AgentKey[],
    );
    const installed = new Set(installedIds);
    return AGENTS.filter(
      (a) =>
        !hiddenIds.includes(a.id) && (installed.has(a.id) || withBackups.has(a.id)),
    );
  }, [backups, installedIds, hiddenIds]);

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
  const detail = items.find((b) => b.id === inspect.target) ?? null;

  useEffect(() => {
    if (inspect.target && !items.some((b) => b.id === inspect.target)) {
      inspect.close();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- close when the selected backup leaves the list
  }, [inspect.target, items]);

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

  const handleDelete = async () => {
    if (!deleteTarget) return;
    const target = deleteTarget;
    setBusyId(target.id);
    try {
      await deleteBackup(target.id);
      toast({ title: t('settings.backups.deleted') });
      setDeleteTarget(null);
      await refresh();
    } catch (e) {
      toast({ title: t('settings.backups.deleteFailed'), description: e instanceof Error ? e.message : String(e), variant: 'danger' });
    } finally {
      setBusyId(null);
    }
  };

  const inspectPanel = detail && agentId ? (
    <BackupDetailPanel
      backup={detail}
      kindLabel={backupKindLabel(detail.kind, t)}
      busy={busyId !== null}
      width={inspect.paneWidth}
      onClose={() => inspect.close()}
      onRestore={() => {
        setDeleteTarget(null);
        setRestoreTarget(detail);
      }}
      onDelete={() => {
        setRestoreTarget(null);
        setDeleteTarget(detail);
      }}
    />
  ) : null;

  return (
    <div className="h-full min-h-0">
      <WorkbenchSplitPage
        split={inspect}
        resizeAria={t('common.resizeSidePanel')}
        panel={inspectPanel}
      >
      <div className={pageRhythm.chromeRow} data-help="page-chrome">
        {toolbar}
        <div className={pageRhythm.chromeActions}>
          <span className="inline-flex items-center gap-2" data-help="backups-keep">
          <Tip className="max-w-[12rem] truncate text-meta text-secondary" label={t('settings.backups.keepCopiesTip')}>
            {t('settings.backups.keepCopiesLabel')}
          </Tip>
          <Switch
            checked={keepCopies}
            onCheckedChange={(v) => {
              setKeepCopies(v);
              void updateSettings({ keepLiveFileCopies: v }).catch(() => {
                setKeepCopies(!v);
              });
            }}
          />
          </span>
          {agentMeta && (
            <Button
              size="sm"
              data-help="backups-now"
              disabled={creating || !isInstalled}
              title={!isInstalled ? t('settings.backups.createTitleNotInstalled') : undefined}
              onClick={() => void handleCreate()}
            >
              <Plus className="h-4 w-4" />
              {creating ? t('settings.backups.creating') : t('settings.backups.backupNow')}
            </Button>
          )}
        </div>
      </div>
      <div className={pageRhythm.chromeRow} data-help="page-chrome">
        {pageLoading ? (
          <Skeleton className="h-9 w-64 rounded-card" />
        ) : (
          <div className="min-w-0 flex-1 overflow-x-auto">
            <AgentTabStrip
              value={agentId ?? visibleAgents[0]?.id ?? AGENTS[0].id}
              onChange={setAgentId}
              agents={visibleAgents}
              emptyLabel={t('settings.backups.emptyAgents')}
            />
          </div>
        )}
        {!pageLoading && agentMeta && agentId && (
          <span className="shrink-0 text-xs text-muted">
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
          actionLabel={t('settings.backups.goAgents')}
          onAction={() => navigate('/agents')}
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
          action={
            isInstalled ? (
              <Button size="sm" variant="outline" className="mt-2" onClick={() => void handleCreate()}>
                {t('settings.backups.backupNow')}
              </Button>
            ) : undefined
          }
        />
      ) : (
        <div className="flex flex-col gap-2">
          {items.map((bk) => {
            const kind = { label: backupKindLabel(bk.kind, t), variant: KIND_VARIANT[bk.kind] };
            const busy = busyId === bk.id;
            const identity = backupCardIdentity(bk);
            const filesLine = backupFileLabels(bk.files);
            const showFiles = Boolean(filesLine) && filesLine !== identity;
            const identityMono = /^\*\*/.test(identity);

            return (
              <ListRow
                key={bk.id}
                data-help="list-row"
                active={inspect.target === bk.id}
                indicatorColor={agentMeta.color}
                className={`${LIST_ROW_PAD} ${busy ? 'pointer-events-none opacity-60' : ''}`}
                onOpen={() => inspect.open(bk.id)}
              >
                <ListRowBody
                  leading={(
                    <AgentDot
                      agentId={agentId}
                      color={agentMeta.color}
                      size="sm"
                      title={null}
                    />
                  )}
                  main={(
                    <>
                      <Badge variant={kind.variant}>{kind.label}</Badge>
                      <Tip
                        className={`truncate text-body font-medium ${identityMono ? 'font-mono' : ''}`}
                        label={identity}
                      >
                        {identity}
                      </Tip>
                      {showFiles ? (
                        <span className="truncate font-mono text-meta text-muted">{filesLine}</span>
                      ) : null}
                      <span className="text-meta text-muted tabular-nums">
                        {fmtRelativeI18n(bk.createdAt, t)}
                      </span>
                    </>
                  )}
                  actions={(
                    <>
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={busyId !== null || restoreTarget !== null || deleteTarget !== null}
                        onClick={() => {
                          setDeleteTarget(null);
                          setRestoreTarget(bk);
                        }}
                      >
                        <RotateCcw className="h-3.5 w-3.5" />
                        {t('common.restore')}
                      </Button>
                      <Button
                        variant="dangerOutline"
                        size="sm"
                        disabled={busyId !== null || restoreTarget !== null || deleteTarget !== null}
                        onClick={() => {
                          setRestoreTarget(null);
                          setDeleteTarget(bk);
                        }}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                        {t('common.delete')}
                      </Button>
                    </>
                  )}
                />
              </ListRow>
            );
          })}
        </div>
      )}
      </WorkbenchSplitPage>

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

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => !open && busyId === null && setDeleteTarget(null)}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('settings.backups.deleteTitle')}</DialogTitle>
            <DialogDescription>
              {deleteTarget &&
                t('settings.backups.deleteDesc', {
                  name: agentDisplayName(deleteTarget.agentId),
                  when: fmtAbsoluteI18n(deleteTarget.createdAt, lang),
                  kind: backupKindLabel(deleteTarget.kind, t),
                })}
            </DialogDescription>
          </DialogHeader>
          <p className="text-sm text-secondary">
            {t('settings.backups.deleteConfirm')}
          </p>
          <DialogFooter>
            <Button variant="secondary" disabled={busyId !== null} onClick={() => setDeleteTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" disabled={busyId !== null} onClick={() => void handleDelete()}>
              {busyId !== null ? t('settings.backups.deleting') : t('settings.backups.confirmDelete')}
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
        <Skeleton key={i} className="h-12 w-full rounded-card" />
      ))}
    </div>
  );
}
