import * as React from 'react';
import {
  ChevronDown,
  FolderOpen,
  Pencil,
  RefreshCw,
  Trash2,
  Gauge,
} from 'lucide-react';
import { CurrentBadge } from '@/components/shared/CurrentBadge';
import { DetailRow } from '@/components/shared/DetailRow';
import { ListRow } from '@/components/shared/ListRow';
import { StatusDot } from '@/components/shared/StatusDot';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { cn } from '@/lib/utils';
import { accountActionPolicy } from '@/lib/backend/contracts/account-actions';
import { authDisplayForAccount, authHealthLabel } from '@/lib/backend/contracts/auth-state';
import {
  endpointModeBadge,
  kindBadge,
  type ConnectionEntry,
} from './connection-model';

/**
 * 统一连接卡：官方登录 / API Key / 供应商共用外壳，操作按 kind 分支。
 * 详情只保留用户决策相关字段，不展示内部 ID / 调试摘要。
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
  canEditProvider = true,
  canSwitchProvider = true,
  canSwitchAccount = true,
  accountSwitchBlockedReason,
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
  /** Provider/API Key configuration is unavailable when the capability is blocked. */
  canEditProvider?: boolean;
  /** Applying a saved Provider writes the agent's live config. */
  canSwitchProvider?: boolean;
  /** Account-pool switching is unavailable when the capability is blocked. */
  canSwitchAccount?: boolean;
  /** Shown on the switch control when account switching is blocked. */
  accountSwitchBlockedReason?: string;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const badge = kindBadge(entry.kind);
  const account = entry.account;
  const accountAction = account ? accountActionPolicy(account) : undefined;
  const authLabel = account
    ? authDisplayForAccount(account).label
    : authHealthLabel(entry.authHealth ?? 'unknown');

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
          <span className="truncate text-sm font-medium" title={entry.title}>
            {entry.title}
          </span>
          <Badge variant={badge.variant}>{badge.label}</Badge>
          {(() => {
            const ep = endpointModeBadge(entry.endpointMode);
            return ep ? <Badge variant={ep.variant}>{ep.label}</Badge> : null;
          })()}
          {entry.subscription ? <Badge>{entry.subscription}</Badge> : null}
          {entry.isCurrent ? <CurrentBadge /> : null}
          {entry.latencyMs != null ? (
            <span className="font-mono text-xs text-muted">{entry.latencyMs} ms</span>
          ) : null}
        </div>

        <div className="flex shrink-0 items-center gap-2">
          {entry.kind === 'oauth' || entry.kind === 'apikey' ? (
            <>
              <QuotaBar label="5h" pct={entry.quota5hPct} resetIn={entry.quotaResetIn} />
              <QuotaBar label="7d" pct={entry.quota7dPct} resetIn={entry.quota7dResetIn} />
            </>
          ) : null}
          {!entry.isCurrent && (
            <Button
              size="sm"
              variant="outline"
              disabled={
                switching ||
                (entry.source === 'provider' && !canSwitchProvider) ||
                (entry.source === 'account' && !canSwitchAccount)
              }
              title={
                entry.source === 'provider' && !canSwitchProvider
                  ? '该 Agent 不支持配置写入'
                  : entry.source === 'account' && !canSwitchAccount
                    ? accountSwitchBlockedReason ?? '该 Agent 不支持账号池切换'
                    : undefined
              }
              onClick={() => onSwitch(entry)}
            >
              切换
            </Button>
          )}
          {entry.source === 'provider' && canEditProvider && (
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
            <DetailRow label="类型" value={badge.label} />
            {entry.endpointMode ? (
              <DetailRow
                label="端点"
                value={entry.endpointMode === 'official' ? '官方' : '自定义'}
              />
            ) : null}
            {account ? (
              <DetailRow
                label={entry.kind === 'oauth' ? '官方账号' : '账号'}
                value={
                  entry.kind === 'oauth'
                    ? account.email ??
                      account.identityLabel ??
                      account.subjectId ??
                      '官方未提供账号信息'
                    : account.email ?? account.identityLabel ?? account.label
                }
              />
            ) : null}
            {account?.provider && !entry.title.includes(account.provider) ? (
              <DetailRow label="提供商" value={account.provider} />
            ) : null}
            {entry.endpointHost ? (
              <DetailRow label="Endpoint" value={entry.endpointHost} mono />
            ) : null}
            {account ? (
              <span className="inline-flex items-center gap-1.5 sm:col-span-2">
                登录态 <StatusDot status={entry.authStatus} />
                <span className="text-xs text-secondary">{authLabel}</span>
              </span>
            ) : null}
          </div>

          {(account?.quota5hPct != null || account?.quota7dPct != null) && (
            <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
              <QuotaBar
                label="5h"
                pct={account.quota5hPct}
                resetIn={account.quotaResetIn}
              />
              <QuotaBar
                label="7d"
                pct={account.quota7dPct}
                resetIn={account.quota7dResetIn}
              />
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
            {accountAction && onRefreshToken && (
              <Button size="sm" variant="secondary" onClick={() => onRefreshToken(entry)}>
                <RefreshCw className="h-3.5 w-3.5" />
                {accountAction.label}
              </Button>
            )}
            {entry.kind === 'apikey' && (
              <Button size="sm" variant="secondary" onClick={() => onEdit(entry)}>
                <Pencil className="h-3.5 w-3.5" /> 编辑密钥
              </Button>
            )}
            {entry.source === 'provider' && canEditProvider && (
              <Button size="sm" variant="secondary" onClick={() => onEdit(entry)}>
                <Pencil className="h-3.5 w-3.5" /> 编辑配置
              </Button>
            )}
            {/* Account and Provider rows are both pool-only deletions, including current rows. */}
            <Button
              size="sm"
              variant="dangerOutline"
              title={
                entry.isCurrent
                  ? '移入回收站；本机连接可能仍继续生效'
                  : undefined
              }
              onClick={() => onDelete(entry)}
            >
              <Trash2 className="h-3.5 w-3.5" /> 删除
            </Button>
          </div>
        </Card>
      )}
    </ListRow>
  );
}

