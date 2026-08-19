import * as React from 'react';
import { Copy, Download, ExternalLink, RefreshCw, X } from 'lucide-react';
import { envOneClickInstallVariant } from '@/components/shared/env-remediation-cta';
import { InlineTerminal, type TerminalStatus } from '@/components/shared/InlineTerminal';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { RUNTIME_MAP, runtimeRemediationsForPlatform } from '@/config/runtimes';
import {
  installRuntimeDetailed,
  resolveAutoInstallPlan,
  RuntimeInstallFailedError,
} from '@/lib/api/env';
import { formatMissingList } from '@/lib/env';
import { runtimeChannelForPlan } from '@/lib/env-plan';
import { detectHostPlatform } from '@/lib/platform-detect';
import { openExternalLink } from '@/lib/open-external';
import type { EnvRemediation, RuntimeDetect, RuntimeId } from '@/lib/types';
import { cn } from '@/lib/utils';

const DONE_HOLD_MS = 600;

/** 缺失 Runtime 的修复步骤面板 — 统一走 installRuntimeDetailed（Tauri / mock 同 contract） */
export function EnvRemediationPanel({
  runtime,
  runtimes,
  focusIds,
  onDone,
  onDismiss,
  onRunningChange,
  className,
  compact,
  /** 打开后自动开始一键安装 */
  autoStart = false,
  pageHasPrimaryCta = false,
}: {
  /** 主展示的 Runtime(兼容单点修复) */
  runtime?: RuntimeDetect;
  /** 完整列表;有则按 plan 一键装全部可自动项 */
  runtimes?: RuntimeDetect[];
  /** 限定修复范围(如渠道 requires) */
  focusIds?: RuntimeId[];
  onDone: () => void;
  onDismiss?: () => void;
  /** 安装 running 态变化,供父级驱动 busy 而不是误用 autoStart */
  onRunningChange?: (running: boolean) => void;
  className?: string;
  compact?: boolean;
  autoStart?: boolean;
  /** 本页已有主 CTA 时，一键安装降为 secondary */
  pageHasPrimaryCta?: boolean;
}) {
  const { toast } = useToast();
  const allRuntimes = runtimes ?? (runtime ? [runtime] : []);
  const plan = resolveAutoInstallPlan(allRuntimes, focusIds ?? (runtime ? [runtime.id] : undefined));

  const primary =
    runtime ??
    allRuntimes.find((r) => r.status !== 'ok' && plan.targets.includes(r.id === 'npm' ? 'nodejs' : r.id)) ??
    allRuntimes.find((r) => r.status !== 'ok') ??
    allRuntimes[0];

  const meta = primary ? RUNTIME_MAP[primary.id] : null;
  const canOneClick = plan.targets.length > 0;
  const hostPlatform = detectHostPlatform();
  const runtimeChannel = runtimeChannelForPlan(hostPlatform);

  const [lines, setLines] = React.useState<string[]>([]);
  const [status, setStatus] = React.useState<TerminalStatus | null>(null);
  const cancelRef = React.useRef({ cancelled: false });
  const autoStartedRef = React.useRef(false);
  const inFlightRef = React.useRef(false);
  const onRunningChangeRef = React.useRef(onRunningChange);
  onRunningChangeRef.current = onRunningChange;

  const setRunning = React.useCallback((running: boolean) => {
    inFlightRef.current = running;
    onRunningChangeRef.current?.(running);
  }, []);

  React.useEffect(() => {
    return () => {
      cancelRef.current.cancelled = true;
      setRunning(false);
    };
  }, [setRunning]);

  const startOneClickInstall = React.useCallback(async () => {
    if (!canOneClick || inFlightRef.current) return;
    cancelRef.current = { cancelled: false };
    setLines(['正在安装…']);
    setStatus('running');
    setRunning(true);
    try {
      const allLogs: string[] = [];
      for (const id of plan.targets) {
        const outcome = await installRuntimeDetailed(id, runtimeChannel);
        allLogs.push(...outcome.logs);
        setLines([...allLogs]);
        if (!outcome.ok) {
          throw new RuntimeInstallFailedError(outcome);
        }
      }
      if (cancelRef.current.cancelled) return;
      setStatus('done');
      toast({
        title: `已安装 ${plan.summary}`,
        description: plan.skipped.length
          ? `仍需手动处理: ${formatMissingList(plan.skipped)}`
          : '若仍检测异常，请完全退出并重启 AgentHub 后再检测',
        variant: 'success',
      });
      await new Promise((r) => setTimeout(r, DONE_HOLD_MS));
      if (!cancelRef.current.cancelled) onDone();
    } catch (e) {
      if (String(e) === 'Error: cancelled' || (e instanceof Error && e.message === 'cancelled')) return;
      setStatus('failed');
      if (e instanceof RuntimeInstallFailedError) {
        setLines(e.logs.length ? e.logs : [e.message]);
        toast({ title: '一键安装失败', description: e.message, variant: 'danger' });
      } else {
        toast({ title: '一键安装失败', description: String(e), variant: 'danger' });
      }
    } finally {
      setRunning(false);
    }
  }, [canOneClick, plan.targets, plan.summary, plan.skipped, onDone, toast, setRunning, runtimeChannel]);

  React.useEffect(() => {
    if (!autoStart || autoStartedRef.current || !canOneClick) return;
    autoStartedRef.current = true;
    void startOneClickInstall();
  }, [autoStart, canOneClick, startOneClickInstall]);

  const copy = (text: string) => {
    navigator.clipboard.writeText(text).catch(() => {});
    toast({ title: '已复制' });
  };

  const openUrl = (url: string) => {
    void openExternalLink(url).catch((e) => {
      toast({
        title: '无法打开链接',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    });
  };

  if (!primary || !meta) return null;

  const remediations = runtimeRemediationsForPlatform(
    primary.remediations.length ? primary.remediations : meta.remediations,
    hostPlatform,
  );
  const title =
    focusIds && focusIds.length > 1
      ? `环境未就绪 · ${formatMissingList(focusIds)}`
      : plan.targets.length > 1
        ? `环境未就绪 · 可一键修复 ${plan.summary}`
        : `${meta.name} 未就绪`;

  return (
    <Card
      className={cn(
        'border-warning/40 bg-warning/5',
        compact ? 'p-3' : 'p-4',
        className,
      )}
    >
      <div className="flex items-start justify-between gap-2">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-sm font-medium">{title}</span>
            <Badge variant="warning">
              {primary.status === 'outdated'
                ? '版本过旧'
                : primary.status === 'broken_path'
                  ? 'PATH 异常'
                  : '未安装'}
            </Badge>
            {canOneClick && <Badge variant="accent">支持一键安装</Badge>}
          </div>
          <p className="mt-1 text-xs text-secondary">
            {canOneClick
              ? `将自动安装 ${plan.summary}${plan.skipped.length ? `;其余需手动: ${formatMissingList(plan.skipped)}` : ''}`
              : meta.description}
          </p>
        </div>
        {onDismiss && status !== 'running' && (
          <Button size="icon" variant="ghost" onClick={onDismiss}>
            <X className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>

      {primary.status === 'broken_path' && (
        <Tip
          className="mt-2 block rounded-btn border border-warning/30 bg-panel px-2.5 py-2 text-xs text-warning"
          label="可能已安装但不在当前进程 PATH。先点「检测」；仍失败请完全退出并重启 AgentHub。"
        >
          PATH 异常：先检测，必要时重启应用
        </Tip>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-2">
        {canOneClick && status !== 'running' && status !== 'done' && (
          <Button
            size="sm"
            variant={envOneClickInstallVariant(pageHasPrimaryCta ?? false)}
            onClick={() => void startOneClickInstall()}
          >
            <Download className="h-3.5 w-3.5" />
            一键安装 {plan.summary}
          </Button>
        )}
        {status === 'running' && (
          <Button size="sm" variant={envOneClickInstallVariant(pageHasPrimaryCta ?? false)} disabled>
            <Download className="h-3.5 w-3.5 animate-pulse" />
            正在一键安装…
          </Button>
        )}
        <Button
          size="sm"
          variant="outline"
          onClick={() => {
            toast({ title: '正在重新检测…' });
            onDone();
          }}
          disabled={status === 'running'}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          已装好，检测
        </Button>
      </div>

      {status && (
        <div className="mt-3">
          <InlineTerminal lines={lines} status={status} />
        </div>
      )}

      {status !== 'running' && (
        <details className="mt-3" open={!canOneClick}>
          <summary className="cursor-pointer text-xs text-muted hover:text-secondary">
            {canOneClick ? '手动步骤' : '修复步骤'}
          </summary>
          <ul className="mt-2 space-y-2">
            {remediations.map((r, i) => (
              <RemediationRow key={i} item={r} onCopy={copy} onOpenUrl={openUrl} />
            ))}
          </ul>
        </details>
      )}
    </Card>
  );
}

function RemediationRow({
  item,
  onCopy,
  onOpenUrl,
}: {
  item: EnvRemediation;
  onCopy: (t: string) => void;
  onOpenUrl: (u: string) => void;
}) {
  if (item.kind === 'hint') {
    return <li className="text-xs text-muted">💡 {item.value}</li>;
  }

  if (item.kind === 'url') {
    return (
      <li className="flex items-center justify-between gap-2 rounded-btn border border-border bg-panel px-2.5 py-1.5">
        <span className="text-xs text-secondary">{item.label ?? '官方下载'}</span>
        <Button size="sm" variant="ghost" className="h-7" onClick={() => onOpenUrl(item.value)}>
          <ExternalLink className="h-3.5 w-3.5" /> 打开
        </Button>
      </li>
    );
  }

  return (
    <li className="flex items-center justify-between gap-2 rounded-btn border border-border bg-panel px-2.5 py-1.5">
      <div className="min-w-0">
        <p className="text-xs text-muted">
          {item.label ??
            (item.kind === 'winget'
              ? 'winget'
              : item.kind === 'brew'
                ? 'Homebrew'
                : '命令')}
        </p>
        <p className="truncate font-mono text-xs text-secondary">{item.value}</p>
      </div>
      <Button size="sm" variant="ghost" className="h-7 shrink-0" onClick={() => onCopy(item.value)}>
        <Copy className="h-3.5 w-3.5" /> 复制
      </Button>
    </li>
  );
}
