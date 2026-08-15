import { AlertTriangle, CheckCircle2, Download, RefreshCw, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Hint, Tip } from '@/components/ui/tooltip';
import { StatusPin } from '@/components/shared/StatusPin';
import { RUNTIME_MAP } from '@/config/runtimes';
import { resolveAutoInstallPlan } from '@/lib/api/env';
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

function statusLabel(status: EnvStatus): string {
  switch (status) {
    case 'ok':
      return '就绪';
    case 'outdated':
      return '版本过旧';
    case 'broken_path':
      return 'PATH 异常';
    case 'missing':
      return '未安装';
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
function powerShellChipLabel(r: RuntimeDetect): string | null {
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
    if (r.status === 'ok' && r.version) return `pwsh v${r.version}`;
    return has7Ok ? 'pwsh' : `pwsh · ${statusLabel(r.status)}`;
  }
  const parts: string[] = [];
  if (has51Ok) parts.push('5.1');
  if (has7Ok) parts.push('7');
  if (parts.length === 0) {
    return r.status === 'ok' && r.version
      ? `PS v${r.version}`
      : `PS · ${statusLabel(r.status)}`;
  }
  return `PS ${parts.join('+')}${r.version ? ` · ${r.version}` : ''}`;
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
      <span className="text-xs font-medium text-secondary">运行环境</span>

      <div className="flex flex-wrap items-center gap-1.5">
        {loading && runtimes.length === 0
          ? Array.from({ length: 4 }).map((_, i) => (
              <span key={i} className="h-6 w-20 animate-pulse rounded-full bg-hover" />
            ))
          : runtimes.map((r) => {
              const meta = RUNTIME_MAP[r.id];
              const label = powerShellChipLabel(r) ??
                (r.status === 'ok' && r.version
                  ? `${meta.shortName} v${r.version}`
                  : `${meta.shortName} · ${statusLabel(r.status)}`);
              return (
                <Hint
                  key={r.id}
                  side="bottom"
                  contentClassName="max-w-sm space-y-0.5"
                  label={
                    <>
                      <p className="font-medium">{meta.name}</p>
                      <p className="text-muted">{meta.description}</p>
                      {r.path && <p className="mt-1 font-mono text-2xs">{r.path}</p>}
                      {r.notes?.map((n) => (
                        <p key={n} className="mt-0.5 font-mono text-2xs text-secondary">
                          {n}
                        </p>
                      ))}
                      {r.status !== 'ok' && onFix && (
                        <p className="mt-1 text-warning">点击查看修复详情</p>
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
          <span className="text-xs text-success">全部就绪</span>
        ) : issues.length > 0 ? (
          <Tip
            className="text-xs text-warning"
            label={
              canOneClick
                ? `可一键安装：${plan.summary}`
                : '悬停芯片看详情，点击打开修复'
            }
          >
            {issues.length} 项待修
          </Tip>
        ) : null}

        {canOneClick && (
          <Button
            size="sm"
            variant="secondary"
            onClick={onOneClickFix}
            disabled={loading || oneClickBusy}
            className="h-7"
            title={`自动安装：${plan.summary}`}
          >
            <Download className={cn('h-3.5 w-3.5', oneClickBusy && 'animate-pulse')} />
            {oneClickBusy ? '安装中…' : '一键修复'}
          </Button>
        )}

        {onRefresh && (
          <Button
            size="sm"
            variant="ghost"
            onClick={onRefresh}
            disabled={loading || oneClickBusy}
            className="h-7"
            title="刷新环境与 Agent 检测结果"
          >
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'animate-spin')} />
            检测
          </Button>
        )}
      </div>
    </Card>
  );
}
