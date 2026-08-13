import * as React from 'react';
import { ChevronDown, FolderOpen, Pencil, RefreshCw, Trash2 } from 'lucide-react';
import type { Account } from '@/lib/types';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { CurrentBadge } from '@/components/shared/CurrentBadge';
import { DetailRow } from '@/components/shared/DetailRow';
import { StatusDot } from '@/components/shared/StatusDot';
import { QuotaBar } from '@/components/shared/QuotaBar';
import { liveConfigPaths } from '@/lib/provider-detect';
import { accountActionPolicy } from '@/lib/backend/contracts/account-actions';
import { authDisplayForAccount } from '@/lib/backend/contracts/auth-state';
import { kindBadge } from '@/lib/connection-kind';
import { cn, fmtRelative } from '@/lib/utils';

/** core 时间多为 `YYYY-MM-DD HH:MM:SS.ffffff`，转相对时间展示 */
function fmtAuthTime(raw?: string): string {
  if (!raw) return '—';
  const normalized = raw.includes('T')
    ? raw
    : `${raw.replace(' ', 'T')}${/[zZ]|[+-]\d{2}:?\d{2}$/.test(raw) ? '' : 'Z'}`;
  const d = new Date(normalized);
  if (Number.isNaN(d.getTime())) return raw.slice(0, 16);
  return fmtRelative(d.toISOString());
}

/**
 * 账号卡:左身份 / 右配额+操作；去掉与 badge 重复的「当前使用」副行文案。
 * 同人多授权时由父级分组；`grouped` 下主标题偏「授权」语义。
 */
export function AccountCard({
  account,
  switching,
  onSwitch,
  onDelete,
  onRefreshToken,
  onEdit,
  onOpenConfigDir,
  grouped = false,
}: {
  account: Account;
  switching?: boolean;
  onSwitch: (a: Account) => void;
  onDelete: (a: Account) => void;
  onRefreshToken: (a: Account) => void;
  /** API Key 账号编辑配置 */
  onEdit?: (a: Account) => void;
  /** 一键打开该 Agent 本机配置/凭据目录 */
  onOpenConfigDir?: (a: Account) => void;
  /** 位于身份分组内时，突出授权时间而非重复身份名 */
  grouped?: boolean;
}) {
  const [expanded, setExpanded] = React.useState(false);
  const detailsId = React.useId();
  const authDisplay = authDisplayForAccount(account);
  const auth = authDisplay.legacyStatus;
  const accountAction = accountActionPolicy(account);
  const paths = liveConfigPaths(account.agentId);
  const badge = kindBadge(account.kind === 'apikey' ? 'apikey' : 'oauth');

  const tokenLine = authDisplay.label;

  const authTime = account.updatedAt ?? account.createdAt;
  const title = grouped
    ? account.kind === 'oauth'
      ? `授权 ${fmtAuthTime(authTime)}`
      : account.label
    : account.label;

  return (
    <Card
      className={cn(
        'p-3 transition-colors',
        account.isCurrent ? 'border-border-strong' : undefined,
      )}
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        {/* 左：身份 / 授权 */}
        <div className="flex min-w-0 flex-1 items-center gap-2">
          <StatusDot status={auth} />
          <span className="truncate text-sm font-medium">{title}</span>
          <Badge variant={badge.variant}>{badge.label}</Badge>
          {account.subscription && <Badge>{account.subscription}</Badge>}
          {account.isCurrent && <CurrentBadge />}
          {grouped && account.kind === 'apikey' && (
            <span className="truncate text-xs text-muted">{account.label}</span>
          )}
        </div>

        {/* 右：配额 + 操作 */}
        <div className="flex shrink-0 items-center gap-2">
          <QuotaBar label="5h" pct={account.quota5hPct} resetIn={account.quotaResetIn} />
          <QuotaBar label="7d" pct={account.quota7dPct} resetIn={account.quota7dResetIn} />
          {!account.isCurrent && (
            <Button size="sm" variant="outline" disabled={switching} onClick={() => onSwitch(account)}>
              切换
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
            <ChevronDown className={cn('h-3.5 w-3.5 transition-transform', expanded && 'rotate-180')} />
          </Button>
        </div>
      </div>

      <p className="mt-1 pl-5 text-xs text-muted">
        {account.isCurrent
          ? tokenLine
          : grouped
            ? `未生效 · 入库 ${fmtAuthTime(authTime)}`
            : `上次使用 ${fmtRelative(account.lastUsedAt)}`}
        {account.source ? ` · ${account.source}` : ''}
        {account.credentialFormat ? ` · ${account.credentialFormat}` : ''}
      </p>

      {expanded && (
        <Card
          id={detailsId}
          variant="plain"
          className="mt-3 flex flex-col gap-2.5 bg-canvas p-3 text-xs"
        >
          <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
            <DetailRow label="账号 ID" value={account.id} mono />
            <DetailRow
              label="类型"
              value={account.kind === 'oauth' ? 'OAuth 登录' : 'API Key'}
            />
            {account.identityLabel && (
              <DetailRow label="身份" value={account.identityLabel} />
            )}
            {account.email && <DetailRow label="邮箱" value={account.email} />}
            {account.status && <DetailRow label="状态" value={account.status} />}
            {account.credentialFormat && (
              <DetailRow label="凭据格式" value={account.credentialFormat} mono />
            )}
            {account.source && <DetailRow label="来源" value={account.source} mono />}
            {account.liveAuthSource && (
              <DetailRow label="实时认证来源" value={account.liveAuthSource} mono />
            )}
            {account.liveAuthRevision && (
              <DetailRow label="实时认证修订" value={account.liveAuthRevision} mono />
            )}
            {account.envKey && (
              <DetailRow label="环境变量键" value={account.envKey} mono />
            )}
            <DetailRow label="创建" value={fmtAuthTime(account.createdAt)} />
            <DetailRow label="更新" value={fmtAuthTime(account.updatedAt)} />
            <span className="inline-flex items-center gap-1.5 sm:col-span-2">
              登录态 <StatusDot status={auth} />
              <span className="text-xs text-secondary">{authDisplay.label}</span>
            </span>
            {account.credentialSummary && (
              <DetailRow
                label="凭据摘要"
                value={account.credentialSummary}
                mono
                className="sm:col-span-2"
              />
            )}
            <DetailRow
              label="本机当前配置"
              value={paths.config}
              mono
              className="sm:col-span-2"
            />
            {paths.auth && (
              <DetailRow label="本机登录凭据" value={paths.auth} mono className="sm:col-span-2" />
            )}
            <DetailRow
              label="打开目录"
              value={paths.openDir}
              mono
              className="sm:col-span-2"
            />
          </div>
          {(account.quota5hPct != null || account.quota7dPct != null) && (
            <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
              <QuotaBar label="5h" pct={account.quota5hPct} resetIn={account.quotaResetIn} />
              <QuotaBar label="7d" pct={account.quota7dPct} resetIn={account.quota7dResetIn} />
            </div>
          )}
          <div className="flex flex-wrap items-center justify-end gap-2 pt-1">
            {onOpenConfigDir && (
              <Button
                size="sm"
                variant="outline"
                onClick={() => onOpenConfigDir(account)}
                title="打开配置目录"
              >
                <FolderOpen className="h-3.5 w-3.5" /> 打开配置目录
              </Button>
            )}
            {accountAction && (
              <Button size="sm" variant="secondary" onClick={() => onRefreshToken(account)}>
                <RefreshCw className="h-3.5 w-3.5" />
                {accountAction.label}
              </Button>
            )}
            {account.kind === 'apikey' && onEdit && (
              <Button size="sm" variant="secondary" onClick={() => onEdit(account)}>
                <Pencil className="h-3.5 w-3.5" /> 编辑
              </Button>
            )}
            <Button
              size="sm"
              variant="dangerOutline"
              title={
                account.isCurrent
                  ? '移入回收站；本机连接可能仍继续生效'
                  : '移入回收站；不会修改本机配置文件'
              }
              onClick={() => onDelete(account)}
            >
              <Trash2 className="h-3.5 w-3.5" /> 删除账号
            </Button>
          </div>
        </Card>
      )}
    </Card>
  );
}

