import * as React from 'react';
import {
  ChevronDown,
  FolderOpen,
  Pencil,
  RefreshCw,
  Trash2,
  Gauge,
} from 'lucide-react';
import { ListRow } from '@/components/shared/ListRow';
import { StatusDot } from '@/components/shared/StatusDot';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { liveConfigPaths } from '@/lib/provider-detect';
import { cn, fmtRelative, fmtRemaining } from '@/lib/utils';
import {
  endpointModeBadge,
  kindBadge,
  type ConnectionEntry,
} from './connection-model';

/**
 * 统一连接卡：官方登录 / API Key / 供应商共用外壳，操作按 kind 分支。
 */
export function ConnectionCard({
  entry,
  brandColor,
  switching,
  testing,
  onSwitch,
  onDelete,
  onEdit,
  onRefreshToken,
  onTest,
  onOpenConfigDir,
}: {
  entry: ConnectionEntry;
  brandColor?: string;
  switching?: boolean;
  testing?: boolean;
  onSwitch: (e: ConnectionEntry) => void;
  onDelete: (e: ConnectionEntry) => void;
  onEdit: (e: ConnectionEntry) => void;
  onRefreshToken?: (e: ConnectionEntry) => void;
  onTest?: (e: ConnectionEntry) => void;
  onOpenConfigDir?: (e: ConnectionEntry) => void;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const badge = kindBadge(entry.kind);
  const paths = liveConfigPaths(entry.agentId);
  const account = entry.account;
  const provider = entry.provider;

  return (
    <ListRow
      active={entry.isCurrent}
      indicatorColor={
        entry.isCurrent && brandColor && entry.source === 'provider'
          ? brandColor
          : undefined
      }
      className="p-3"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <div className="flex min-w-0 flex-1 items-center gap-2">
          {entry.source === 'provider' ? (
            <span
              className={cn('text-xs', entry.isCurrent ? 'text-success' : 'text-muted')}
              aria-hidden
            >
              {entry.isCurrent ? '●' : '○'}
            </span>
          ) : (
            <StatusDot status={entry.authStatus} />
          )}
          <span className="truncate text-sm font-medium">{entry.title}</span>
          <Badge variant={badge.variant}>{badge.label}</Badge>
          {(() => {
            const ep = endpointModeBadge(entry.endpointMode);
            return ep ? <Badge variant={ep.variant}>{ep.label}</Badge> : null;
          })()}
          {entry.subscription ? <Badge>{entry.subscription}</Badge> : null}
          {entry.isCurrent ? <Badge variant="accent">当前</Badge> : null}
          {entry.latencyMs != null ? (
            <span className="font-mono text-xs text-muted">{entry.latencyMs} ms</span>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {entry.kind === 'oauth' || entry.kind === 'apikey' ? (
            <QuotaBar label="5h" pct={entry.quota5hPct} resetIn={entry.quotaResetIn} />
          ) : null}
          {!entry.isCurrent && (
            <Button
              size="sm"
              variant="outline"
              disabled={switching}
              onClick={() => onSwitch(entry)}
            >
              切换
            </Button>
          )}
          {entry.source === 'provider' && (
            <Button size="sm" variant="secondary" onClick={() => onEdit(entry)}>
              <Pencil className="h-3.5 w-3.5" /> 编辑
            </Button>
          )}
          {entry.source === 'provider' && onTest && (
            <Button
              size="sm"
              variant="ghost"
              disabled={testing}
              onClick={() => onTest(entry)}
            >
              <Gauge className="h-3.5 w-3.5" />
              {testing ? '测速中…' : '测速'}
            </Button>
          )}
          <Button
            size="sm"
            variant="ghost"
            aria-expanded={expanded}
            aria-controls={detailsId}
            onClick={() => setExpanded((v) => !v)}
          >
            详情
            <ChevronDown
              className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')}
            />
          </Button>
        </div>
      </div>

      <p className="mt-1 pl-5 text-xs text-muted">{entry.subtitle}</p>

      {expanded && (
        <Card
          id={detailsId}
          variant="plain"
          className="mt-3 flex flex-col gap-2.5 bg-canvas p-3 text-xs"
        >
          <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
            <DetailRow label="ID" value={entry.id} mono />
            <DetailRow label="类型" value={badge.label} />
            {entry.endpointMode ? (
              <DetailRow
                label="端点"
                value={entry.endpointMode === 'official' ? '官方' : '自定义'}
              />
            ) : null}
            {entry.identityLabel ? (
              <DetailRow label="身份" value={entry.identityLabel} />
            ) : null}
            {account?.email ? <DetailRow label="邮箱" value={account.email} /> : null}
            {account?.source ? (
              <DetailRow label="来源" value={account.source} mono />
            ) : null}
            {account?.credentialFormat ? (
              <DetailRow label="凭据格式" value={account.credentialFormat} mono />
            ) : null}
            {account?.envKey ? (
              <DetailRow label="环境变量键" value={account.envKey} mono />
            ) : null}
            {entry.endpointHost ? (
              <DetailRow label="Endpoint" value={entry.endpointHost} mono />
            ) : null}
            {provider?.preset ? (
              <DetailRow label="预设" value={provider.preset} mono />
            ) : null}
            {account?.createdAt ? (
              <DetailRow label="创建" value={fmtLooseTime(account.createdAt)} />
            ) : null}
            {entry.sortKey ? (
              <DetailRow label="更新" value={fmtLooseTime(entry.sortKey)} />
            ) : null}
            {account && (
              <span className="inline-flex items-center gap-1.5 sm:col-span-2">
                Token <StatusDot status={entry.authStatus} withLabel />
                {account.tokenValid && account.tokenRemainingSec !== undefined && (
                  <span className="text-muted">
                    剩余 {fmtRemaining(account.tokenRemainingSec)}
                  </span>
                )}
              </span>
            )}
            {account?.credentialSummary ? (
              <DetailRow
                label="凭据摘要"
                value={account.credentialSummary}
                mono
                className="sm:col-span-2"
              />
            ) : null}
            <DetailRow
              label="Live 配置"
              value={paths.config}
              mono
              className="sm:col-span-2"
            />
            {paths.auth ? (
              <DetailRow
                label="Live 凭据"
                value={paths.auth}
                mono
                className="sm:col-span-2"
              />
            ) : null}
          </div>

          {(account?.quota5hPct != null || account?.quota7dPct != null) && (
            <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
              <QuotaBar
                label="5h"
                pct={account.quota5hPct}
                resetIn={account.quotaResetIn}
              />
              <QuotaBar label="7d" pct={account.quota7dPct} />
            </div>
          )}

          <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
            {onOpenConfigDir && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => onOpenConfigDir(entry)}
                title="打开配置目录"
              >
                <FolderOpen className="h-3.5 w-3.5" /> 打开配置目录
              </Button>
            )}
            {entry.kind === 'oauth' && onRefreshToken && (
              <Button size="sm" variant="secondary" onClick={() => onRefreshToken(entry)}>
                <RefreshCw className="h-3.5 w-3.5" /> 刷新 Token
              </Button>
            )}
            {entry.kind === 'apikey' && (
              <Button size="sm" variant="secondary" onClick={() => onEdit(entry)}>
                <Pencil className="h-3.5 w-3.5" /> 编辑密钥
              </Button>
            )}
            {entry.source === 'provider' && (
              <Button size="sm" variant="secondary" onClick={() => onEdit(entry)}>
                <Pencil className="h-3.5 w-3.5" /> 编辑配置
              </Button>
            )}
            {/* 供应商允许删当前项（只清池）；账号当前项不删以免误伤 */}
            {(entry.source === 'provider' || !entry.isCurrent) && (
              <Button
                size="sm"
                variant="dangerOutline"
                onClick={() => onDelete(entry)}
              >
                <Trash2 className="h-3.5 w-3.5" /> 删除
              </Button>
            )}
          </div>
        </Card>
      )}
    </ListRow>
  );
}

function fmtLooseTime(raw?: string): string {
  if (!raw) return '—';
  const normalized = raw.includes('T')
    ? raw
    : `${raw.replace(' ', 'T')}${/[zZ]|[+-]\d{2}:?\d{2}$/.test(raw) ? '' : 'Z'}`;
  const d = new Date(normalized);
  if (Number.isNaN(d.getTime())) return raw.slice(0, 16);
  return fmtRelative(d.toISOString());
}

function DetailRow({
  label,
  value,
  mono,
  className,
}: {
  label: string;
  value: string;
  mono?: boolean;
  className?: string;
}) {
  return (
    <span className={cn('min-w-0', className)}>
      <span className="text-muted">{label} </span>
      {mono ? (
        <code className="break-all font-mono text-secondary">{value}</code>
      ) : (
        <span className="break-all text-secondary">{value}</span>
      )}
    </span>
  );
}
