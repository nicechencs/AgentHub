import { AlertTriangle, ArrowUpCircle, CheckCircle2, Download, RefreshCw, Wrench, XCircle } from 'lucide-react';
import { envOneClickInstallVariant } from '@/components/shared/env-remediation-cta';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import {
  Table,
  TableBody,
  TableCell,
  TableEmptyCell,
  TableHead,
  TableHeader,
  TableHeaderRow,
  TableRow,
} from '@/components/ui/table';
import { Hint, Tip } from '@/components/ui/tooltip';
import { RUNTIME_MAP } from '@/config/runtimes';
import { resolveAutoInstallPlan } from '@/lib/api/env';
import { detectHostPlatform } from '@/lib/platform-detect';
import type { EnvStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';
import {
  envSoftwareAction,
  envSoftwareActionLabel,
  envSoftwareColumnLabel,
  envSoftwareName,
  envSoftwareNoteKey,
  envSoftwareStatusLabel,
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
}: {
  runtimes: RuntimeDetect[];
  loading?: boolean;
  onRefresh?: () => void;
  onAction: (runtime: RuntimeDetect, intent: EnvSoftwareIntent) => void;
  onOneClickFix?: () => void;
  oneClickBusy?: boolean;
}) {
  const { t } = useI18n();
  const platform = detectHostPlatform();
  const issues = runtimes.filter((r) => r.status !== 'ok');
  const allOk = issues.length === 0 && runtimes.length > 0;
  const plan = resolveAutoInstallPlan(runtimes);
  const canOneClick = plan.targets.length > 0 && !!onOneClickFix;
  const busy = Boolean(loading || oneClickBusy);

  return (
    <Card className={cn(!allOk && issues.length > 0 && 'border-warning/40 bg-warning/5')}>
      <div className="flex flex-wrap items-center gap-2 px-3 py-2.5">
        <span className="text-xs font-medium text-secondary">{t('chrome.env.title')}</span>
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
                const action = envSoftwareAction(runtime, runtimes, platform);
                const meta = RUNTIME_MAP[runtime.id];
                const Icon = action ? actionIcon(action) : null;
                return (
                  <TableRow key={runtime.id}>
                    <TableCell className="font-medium">{envSoftwareName(runtime)}</TableCell>
                    <TableCell>
                      <Badge variant={chipVariant(runtime.status)} className="gap-1">
                        {runtime.status === 'missing' ? <StatusPin tone="muted" /> : statusIcon(runtime.status)}
                        {envSoftwareStatusLabel(runtime.status, t)}
                      </Badge>
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
                      {action && Icon ? (
                        <Button
                          size="sm"
                          variant={
                            action === 'upgrade' && runtime.status === 'ok'
                              ? 'outline'
                              : envOneClickInstallVariant(true)
                          }
                          className={cn('h-7', action === 'upgrade' && runtime.status === 'ok' && 'text-muted')}
                          disabled={busy}
                          onClick={() => onAction(runtime, action)}
                          title={
                            action === 'upgrade' && runtime.status === 'ok'
                              ? t('chrome.env.upgradeLatest')
                              : envSoftwareActionLabel(action, t)
                          }
                        >
                          <Icon className="h-3.5 w-3.5" />
                          {envSoftwareActionLabel(action, t)}
                        </Button>
                      ) : (
                        <TableEmptyCell />
                      )}
                    </TableCell>
                  </TableRow>
                );
              })}
        </TableBody>
      </Table>
    </Card>
  );
}
