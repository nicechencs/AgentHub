import { AlertTriangle, CheckCircle2, Download, RefreshCw, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint, Tip } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { StatusPin } from '@/components/shared/StatusPin';
import { RUNTIME_MAP, runtimeDescriptionKey } from '@/config/runtimes';
import { resolveAutoInstallPlan } from '@/lib/api/env';
import type { TranslateFn } from '@/lib/i18n';
import type { EnvStatus, RuntimeDetect } from '@/lib/types';
import { cn } from '@/lib/utils';

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

function statusLabel(status: EnvStatus, t: TranslateFn): string {
  switch (status) {
    case 'ok':
      return t('chrome.env.statusOk');
    case 'outdated':
      return t('chrome.env.statusOutdated');
    case 'broken_path':
      return t('chrome.env.statusBrokenPath');
    case 'missing':
      return t('chrome.env.statusMissing');
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
      // 「未安装」不是错误，用 muted 默认态
      return 'default';
    default:
      return 'default';
  }
}

function statusIconMuted(status: EnvStatus) {
  if (status === 'missing') {
    return <StatusPin tone="muted" />;
  }
  return statusIcon(status);
}

/** Windows：从 notes 提炼 5.1 / 7 双版本芯片文案；其它平台回退默认。 */
function powerShellChipLabel(r: RuntimeDetect, t: TranslateFn): string | null {
  if (r.id !== 'powershell' || !r.notes?.length) return null;
  const has51Ok = r.notes.some(
    (n) => n.includes('Windows PowerShell 5.1:') && !n.includes('missing') && !n.includes('broken') && !n.includes('not applicable'),
  );
  const has51Na = r.notes.some((n) => n.includes('not applicable'));
  const has7Ok = r.notes.some(
    (n) => n.includes('PowerShell 7') && !n.includes('missing') && !n.includes('broken'),
  );
  if (has51Na) {
    // macOS / Linux：不展示虚假 5.1
    if (r.status === 'ok' && r.version) return t('chrome.env.pwshVersion', { version: r.version });
    return has7Ok ? t('chrome.env.pwsh') : t('chrome.env.pwshStatus', { status: statusLabel(r.status, t) });
  }
  const parts: string[] = [];
  if (has51Ok) parts.push('5.1');
  if (has7Ok) parts.push('7');
  if (parts.length === 0) {
    return r.status === 'ok' && r.version
      ? t('chrome.env.psVersion', { version: r.version })
      : t('chrome.env.psStatus', { status: statusLabel(r.status, t) });
  }
  return r.version
    ? t('chrome.env.psPartsVersion', { parts: parts.join('+'), version: r.version })
    : t('chrome.env.psParts', { parts: parts.join('+') });
}

/** 页顶共享运行时状态条 — 含一键修复 */
export function EnvStatusBar({
  runtimes,
  loading,
  onRefresh,
  onFix,
  onOneClickFix,
  oneClickBusy,
  className,
}: {
  runtimes: RuntimeDetect[];
  loading?: boolean;
  onRefresh?: () => void;
  /** 点击有问题的 Runtime 芯片 */
  onFix?: (runtime: RuntimeDetect) => void;
  /** 一键安装全部可自动修复项 */
  onOneClickFix?: () => void;
  oneClickBusy?: boolean;
  className?: string;
}) {
  const { t } = useI18n();
  const issues = runtimes.filter((r) => r.status !== 'ok');
  const allOk = issues.length === 0 && runtimes.length > 0;
  const plan = resolveAutoInstallPlan(runtimes);
  const canOneClick = plan.targets.length > 0 && !!onOneClickFix;

  return (
    <Card
      className={cn(
        'flex flex-wrap items-center gap-2 px-3 py-2.5',
        !allOk && issues.length > 0 && 'border-warning/40 bg-warning/5',
        className,
      )}
    >
      <span className="text-xs font-medium text-secondary">{t('chrome.env.title')}</span>

      <div className="flex flex-wrap items-center gap-1.5">
        {loading && runtimes.length === 0
          ? Array.from({ length: 4 }).map((_, i) => (
              <span key={i} className="h-6 w-20 animate-pulse rounded-full bg-hover" />
            ))
          : runtimes.map((r) => {
              const meta = RUNTIME_MAP[r.id];
              const label = powerShellChipLabel(r, t) ??
                (r.status === 'ok' && r.version
                  ? `${meta.shortName} v${r.version}`
                  : `${meta.shortName} · ${statusLabel(r.status, t)}`);
              return (
                <Hint
                  key={r.id}
                  side="bottom"
                  contentClassName="space-y-0.5"
                  label={
                    <>
                      <p className="font-medium">{meta.name}</p>
                      <p className="text-muted">{t(runtimeDescriptionKey(meta.id))}</p>
                      {r.path && <p className="mt-1 font-mono text-meta">{r.path}</p>}
                      {r.notes?.map((n) => (
                        <p key={n} className="mt-0.5 font-mono text-meta text-secondary">
                          {n}
                        </p>
                      ))}
                      {r.status !== 'ok' && onFix && (
                        <p className="mt-1 text-warning">{t('chrome.env.clickFix')}</p>
                      )}
                    </>
                  }
                >
                  <button
                    type="button"
                    className="outline-none"
                    onClick={() => r.status !== 'ok' && onFix?.(r)}
                    disabled={r.status === 'ok' || !onFix}
                  >
                    <Badge
                      variant={chipVariant(r.status)}
                      className={cn(
                        'gap-1',
                        r.status !== 'ok' && onFix && 'cursor-pointer hover:opacity-90',
                      )}
                    >
                      {statusIconMuted(r.status)}
                      {label}
                    </Badge>
                  </button>
                </Hint>
              );
            })}
      </div>

      <div className="ml-auto flex flex-wrap items-center gap-2">
        {allOk ? (
          <span className="text-xs text-success">{t('chrome.env.allReady')}</span>
        ) : issues.length > 0 ? (
          <Tip
            className="text-xs text-warning"
            label={
              canOneClick
                ? t('chrome.env.oneClickInstall', { summary: plan.summary })
                : t('chrome.env.hoverFix')
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
            disabled={loading || oneClickBusy}
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
            disabled={loading || oneClickBusy}
            className="h-7"
            title={t('chrome.env.refreshTitle')}
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
            {t('chrome.env.detect')}
          </Button>
        )}
      </div>
    </Card>
  );
}
