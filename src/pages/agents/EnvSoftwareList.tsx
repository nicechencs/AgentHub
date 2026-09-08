import { useEffect, useState } from 'react';
import { AlertTriangle, ArrowUpCircle, CheckCircle2, ChevronDown, Download, RefreshCw, Wrench, XCircle } from 'lucide-react';
import { envOneClickInstallVariant } from '@/components/shared/env-remediation-cta';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
} from '@/components/ui/table';
import { Hint, Tip } from '@/components/ui/tooltip';
import { RUNTIME_MAP } from '@/config/runtimes';
import { resolveAutoInstallPlan } from '@/lib/api/env';
import { openExternalLink } from '@/lib/open-external';
import { detectHostPlatform } from '@/lib/platform-detect';
import type { EnvStatus, RuntimeDetect, RuntimeUpdateInfo } from '@/lib/types';
import { cn } from '@/lib/utils';
import { useToast } from '@/components/ui/toast';
import {
  envSoftwareActionLabel,
  envSoftwareColumnLabel,
  envSoftwareControl,
  envSoftwareListOpenByDefault,
  envSoftwareName,
  envSoftwareNoteKey,
  envSoftwareStatusLabel,
  envSoftwareUpgradeTitle,
  envSoftwareVersion,
  type EnvSoftwareAction,
} from './env-software-list-model';

function statusIcon(status: EnvStatus) {
  switch (status) {
    case 'ok':
      return <CheckCircle2 className="h-3.5 w-3.5 text-success" />;
    case 'outdated':
    case 'broken_path':
      return <AlertTriangle className="h-3.5 w-3.5 text-warning" />;
    case 'missing':
    default:
      return <XCircle className="h-3.5 w-3.5 text-danger" />;
  }
}

function chipVariant(status: EnvStatus): 'success' | 'warning' | 'default' {
  switch (status) {
    case 'ok':
      return 'success';
    case 'outdated':
    case 'broken_path':
      return 'warning';
    case 'missing':
      return 'default';
  }
}

function actionIcon(action: EnvSoftwareAction) {
  switch (action) {
    case 'install':
      return Download;
    case 'upgrade':
      return ArrowUpCircle;
    case 'repair':
      return Wrench;
  }
}

export type EnvSoftwareIntent = 'install' | 'upgrade' | 'repair';

/** Agents 页顶：列出环境需要的软件，并提供安装 / 升级 / 修复。 */
export function EnvSoftwareList({
  runtimes,
  loading,
  onRefresh,
  onAction,
  onOneClickFix,
  oneClickBusy,
  runtimeUpdates,
  updatesLoading = false,
}: {
  runtimes: RuntimeDetect[];
  loading?: boolean;
  onRefresh?: () => void;
  onAction: (runtime: RuntimeDetect, intent: EnvSoftwareIntent, canAutoUpgrade?: boolean) => void;
  onOneClickFix?: () => void;
  oneClickBusy?: boolean;
  runtimeUpdates?: Partial<Record<RuntimeDetect['id'], RuntimeUpdateInfo>>;
  updatesLoading?: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const platform = detectHostPlatform();
  const [forceRuntime, setForceRuntime] = useState<RuntimeDetect | null>(null);
  const issues = runtimes.filter((r) => r.status !== 'ok');
  const allOk = issues.length === 0 && runtimes.length > 0;
  const openByDefault = envSoftwareListOpenByDefault(runtimes);
  const [userOpen, setUserOpen] = useState<boolean | null>(null);
  useEffect(() => {
    setUserOpen(null);
  }, [openByDefault]);
  const expanded = userOpen ?? openByDefault;
  const plan = resolveAutoInstallPlan(runtimes);
  const canOneClick = plan.targets.length > 0 && !!onOneClickFix;
  const busy = Boolean(loading || oneClickBusy);

  return (
    <Card className={cn(!allOk && issues.length > 0 && 'border-warning/40 bg-warning/5')}>
      <div className="flex flex-wrap items-center gap-2 px-3 py-2.5">
        <button
          type="button"
          className="inline-flex min-w-0 flex-1 items-center gap-1 text-xs font-medium text-secondary transition-colors hover:text-primary"
          aria-expanded={expanded}
          onClick={() => setUserOpen(!expanded)}
        >
          <ChevronDown
            className={cn('h-3.5 w-3.5 shrink-0 transition-transform', !expanded && '-rotate-90')}
            aria-hidden
          />
          {t('chrome.env.title')}
        </button>
        <div className="ml-auto flex flex-wrap items-center gap-2">
          {allOk ? (
            <span className="text-xs text-success">{t('chrome.env.allReady')}</span>
          ) : issues.length > 0 ? (
            <Tip
              className="text-xs text-warning"
              label={
                canOneClick
                  ? t('chrome.env.oneClickInstall', { summary: plan.summary })
                  : t('chrome.env.clickFix')
              }
            >
              {t('chrome.env.issuesCount', { n: issues.length })}
            </Tip>
          ) : null}

          {canOneClick && (
            <Button
              size="sm"
              variant="secondary"
              onClick={onOneClickFix}
              disabled={busy}
              className="h-7"
              title={t('chrome.env.autoInstallTitle', { summary: plan.summary })}
            >
              <Download className={cn('h-3.5 w-3.5', oneClickBusy && 'animate-pulse')} />
              {oneClickBusy ? t('chrome.env.installing') : t('chrome.env.oneClickFix')}
            </Button>
          )}

          {onRefresh && (
            <Button
              size="sm"
              variant="ghost"
              onClick={onRefresh}
              disabled={busy}
              className="h-7"
              title={t('chrome.env.refreshTitle')}
            >
              <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
              {t('chrome.env.detect')}
            </Button>
          )}
        </div>
      </div>

      {expanded ? (
        <Table className="w-full">
        <TableHeader>
          <TableHeaderRow>
            <TableHead>{envSoftwareColumnLabel('software', t)}</TableHead>
            <TableHead>{envSoftwareColumnLabel('status', t)}</TableHead>
            <TableHead>{envSoftwareColumnLabel('version', t)}</TableHead>
            <TableHead>{envSoftwareColumnLabel('note', t)}</TableHead>
            <TableHead className="text-right">{envSoftwareColumnLabel('actions', t)}</TableHead>
          </TableHeaderRow>
        </TableHeader>
        <TableBody>
          {loading && runtimes.length === 0
            ? Array.from({ length: 3 }).map((_, i) => (
                <TableRow key={i}>
                  <TableCell colSpan={5}>
                    <span className="block h-6 w-full animate-pulse rounded-btn bg-hover" />
                  </TableCell>
                </TableRow>
              ))
            : runtimes.map((runtime) => {
                const update = runtimeUpdates?.[runtime.id];
                const control = envSoftwareControl(runtime, runtimes, platform, update);
                const action = control.action;
                const meta = RUNTIME_MAP[runtime.id];
                const Icon = actionIcon(action);
                const actionLabel = envSoftwareActionLabel(action, t);
                const checking = Boolean(updatesLoading && action === 'upgrade');
                const actionTitle = envSoftwareUpgradeTitle(control, t, update, checking);
                return (
                  <TableRow key={runtime.id}>
                    <TableCell className="font-medium">{envSoftwareName(runtime)}</TableCell>
                    <TableCell>
                      <Tip label={envSoftwareStatusLabel(runtime.status, t)}>
                        <Badge
                          variant={chipVariant(runtime.status)}
                          className="px-1.5"
                          aria-label={envSoftwareStatusLabel(runtime.status, t)}
                        >
                          {runtime.status === 'missing' ? <StatusPin tone="muted" /> : statusIcon(runtime.status)}
                        </Badge>
                      </Tip>
                    </TableCell>
                    <TableCell className="font-mono text-meta text-secondary">
                      {envSoftwareVersion(runtime)}
                    </TableCell>
                    <TableCell>
                      <Hint
                        side="bottom"
                        contentClassName="space-y-0.5"
                        label={
                          <>
                            <p className="font-medium">{meta.name}</p>
                            <p className="text-muted">{t(envSoftwareNoteKey(runtime.id))}</p>
                            {runtime.path && <p className="mt-1 font-mono text-meta">{runtime.path}</p>}
                            {runtime.notes?.map((n) => (
                              <p key={n} className="mt-0.5 font-mono text-meta text-secondary">
                                {n}
                              </p>
                            ))}
                          </>
                        }
                      >
                        <span className="line-clamp-1 text-meta text-secondary">
                          {t(envSoftwareNoteKey(runtime.id))}
                        </span>
                      </Hint>
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        size={action === 'upgrade' ? 'icon' : 'sm'}
                        variant={
                          action === 'upgrade'
                            ? control.muted
                              ? 'outline'
                              : 'secondary'
                            : envOneClickInstallVariant(true)
                        }
                        className={cn(
                          'h-7',
                          action === 'upgrade' && 'w-7',
                          control.muted && 'text-muted',
                        )}
                        disabled={busy || checking || control.kind === 'hint_only'}
                        onClick={() => {
                          if (action === 'upgrade' && control.kind === 'open_setup') {
                            const url = update?.setupUrl?.trim();
                            if (!url) return;
                            void openExternalLink(url).catch((e) => {
                              toast({
                                title: t('chrome.env.openLinkFailed'),
                                description: e instanceof Error ? e.message : String(e),
                                variant: 'danger',
                              });
                            });
                            return;
                          }
                          if (action === 'upgrade' && control.kind === 'in_app' && !control.upgradable) {
                            setForceRuntime(runtime);
                            return;
                          }
                          onAction(
                            runtime,
                            action,
                            action !== 'upgrade' || update?.canAutoUpgrade !== false,
                          );
                        }}
                        title={actionTitle}
                        aria-label={actionTitle}
                      >
                        <Icon
                          className={cn(
                            'h-3.5 w-3.5',
                            action === 'upgrade' && !control.muted && control.upgradable && 'text-success',
                            control.muted && 'text-muted',
                            checking && 'animate-pulse opacity-70',
                          )}
                        />
                        {action === 'upgrade' ? null : actionLabel}
                      </Button>
                    </TableCell>
                  </TableRow>
                );
              })}
        </TableBody>
        </Table>
      ) : null}

      <Dialog
        open={forceRuntime != null}
        onOpenChange={(open) => {
          if (!open) setForceRuntime(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {t('chrome.env.forceUpgradeTitle', {
                name: forceRuntime ? envSoftwareName(forceRuntime) : '',
              })}
            </DialogTitle>
            <DialogDescription>
              {forceRuntime && runtimeUpdates?.[forceRuntime.id]?.state === 'up_to_date'
                ? t('chrome.env.forceUpgradeUpToDate')
                : t('chrome.env.forceUpgradeUnknown')}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => setForceRuntime(null)} disabled={busy}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="default"
              disabled={busy || !forceRuntime}
              onClick={() => {
                const runtime = forceRuntime;
                if (!runtime) return;
                setForceRuntime(null);
                onAction(runtime, 'upgrade', true);
              }}
            >
              {t('chrome.env.confirmForceUpgrade')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
