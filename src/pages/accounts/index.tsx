import * as React from 'react';
import { ChevronDown, DownloadCloud, KeyRound, LogIn, Plus, UserCircle } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { Button } from '@/components/ui/button';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { ListSkeleton } from '@/components/ui/skeleton';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useToast } from '@/components/ui/toast';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { SwitchConfirmDialog } from '@/components/shared/SwitchConfirmDialog';
import { OAuthFlowDialog } from '#oauth-flow-dialog';
import {
  deleteAccount,
  importCurrentLogin,
  listAccounts,
  refreshToken,
  switchAccount,
  undoSwitchAccount,
} from '@/lib/api/account';
import { groupAccountsByIdentity } from '@/lib/backend/contracts/account-map';
import { listAgents } from '@/lib/api/agent';
import { openAgentConfigDir } from '@/lib/api/install';
import { resolveAgentMeta } from '@/config/agents';
import { isCapabilityBlocked } from '@/lib/capability';
import type { Account, AgentId, AgentStatus, SwitchPreview } from '@/lib/types';
import { AccountCard } from './AccountCard';
import { ApiKeyAccountDialog } from './ApiKeyAccountDialog';

/** 不支持账号切换的 agent；必须传入 detect/doctor 下发的 statuses。无 statuses 时不猜 MOCK。 */
export function accountDisabledAgents(statuses?: AgentStatus[] | null): AgentId[] {
  if (!statuses?.length) return [];
  return statuses
    .filter((s) => isCapabilityBlocked(s.capabilities?.accountSwitch))
    .map((s) => s.agentId);
}

type AccountKindFilter = 'all' | 'oauth' | 'apikey';

const ACCOUNT_KIND_FILTERS: Array<{ value: AccountKindFilter; label: string }> = [
  { value: 'all', label: '全部' },
  { value: 'oauth', label: '官方登录' },
  { value: 'apikey', label: 'API Key' },
];

export interface AccountsPanelProps {
  /** 嵌入 Connections 时隐藏页头与 AgentTabStrip */
  embedded?: boolean;
  /** 受控 agent;不传则内部自管 */
  agentId?: AgentId;
  onAgentIdChange?: (id: AgentId) => void;
  /** 账号池变更后通知父级刷新 Tab 角标 */
  onPoolChanged?: () => void;
}

/** Accounts 面板:账号池 + 一键切换(docs/ui-design.md §4.4);可嵌入 Connections */
export default function AccountsPage({
  embedded = false,
  agentId: controlledAgentId,
  onAgentIdChange,
  onPoolChanged,
}: AccountsPanelProps = {}) {
  const { toast } = useToast();
  const controlled = controlledAgentId !== undefined;
  const [internalAgent, setInternalAgent] = React.useState<AgentId>('claude');
  const agent = controlled ? controlledAgentId : internalAgent;

  const [accounts, setAccounts] = React.useState<Account[]>([]);
  const [kindFilter, setKindFilter] = React.useState<AccountKindFilter>('all');
  const [phase, setPhase] = React.useState<'loading' | 'error' | 'ready'>('loading');
  const [error, setError] = React.useState<unknown>(null);
  // partial:agent 运行状态加载失败仅影响切换警告,不阻塞页面
  const [agentStatuses, setAgentStatuses] = React.useState<AgentStatus[]>([]);

  const [switchTarget, setSwitchTarget] = React.useState<Account | null>(null);
  const [switching, setSwitching] = React.useState(false);
  const [deleteTarget, setDeleteTarget] = React.useState<Account | null>(null);
  const [deleting, setDeleting] = React.useState(false);
  const [oauthOpen, setOauthOpen] = React.useState(false);
  const [apiKeyOpen, setApiKeyOpen] = React.useState(false);
  const [editTarget, setEditTarget] = React.useState<Account | null>(null);

  const setAgent = (id: AgentId) => {
    if (controlled) {
      onAgentIdChange?.(id);
      return;
    }
    setInternalAgent(id);
  };

  const load = React.useCallback(
    async (agentId: AgentId) => {
      setPhase('loading');
      setError(null);
      try {
        const list = await listAccounts(agentId);
        setAccounts(list);
        setPhase('ready');
        onPoolChanged?.();
      } catch (e) {
        setError(e);
        setPhase('error');
      }
    },
    [onPoolChanged],
  );

  React.useEffect(() => {
    load(agent);
  }, [agent, load]);

  // 换 agent 时回到「全部」，避免上一个 agent 的筛选空结果
  React.useEffect(() => {
    setKindFilter('all');
  }, [agent]);

  React.useEffect(() => {
    listAgents()
      .then(setAgentStatuses)
      .catch(() => setAgentStatuses([]));
  }, []);

  const current = accounts.find((a) => a.isCurrent);
  const meta = resolveAgentMeta(agent);
  const kindCounts = React.useMemo(() => {
    let oauth = 0;
    let apikey = 0;
    for (const a of accounts) {
      if (a.kind === 'oauth') oauth += 1;
      else if (a.kind === 'apikey') apikey += 1;
    }
    return { all: accounts.length, oauth, apikey };
  }, [accounts]);
  const visibleAccounts = React.useMemo(
    () =>
      kindFilter === 'all' ? accounts : accounts.filter((account) => account.kind === kindFilter),
    [accounts, kindFilter],
  );
  const identityGroups = React.useMemo(
    () => groupAccountsByIdentity(visibleAccounts),
    [visibleAccounts],
  );

  const doUndo = React.useCallback(async () => {
    const ok = await undoSwitchAccount(agent);
    if (ok) {
      await load(agent);
      toast({ title: '已撤销切换', description: `${meta.name} 已切回原账号` });
    } else {
      toast({ title: '无法撤销', description: '没有可回滚的切换记录', variant: 'danger' });
    }
  }, [agent, load, meta.name, toast]);

  const confirmSwitch = async () => {
    if (!switchTarget) return;
    const target = switchTarget;
    setSwitching(true);
    try {
      await switchAccount(agent, target.id);
      setSwitchTarget(null);
      await load(agent);
      toast({
        title: `已切换到 ${target.label}`,
        description: '已写入本机；原配置已回存并备份',
        variant: 'success',
        actionLabel: '撤销',
        onAction: () => {
          doUndo().catch(() => {});
        },
        duration: 5000,
      });
    } catch (e) {
      toast({ title: '切换失败', description: String(e), variant: 'danger' });
    } finally {
      setSwitching(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteAccount(agent, deleteTarget.id);
      setDeleteTarget(null);
      await load(agent);
      toast({ title: '凭据已删除', variant: 'success' });
    } catch (e) {
      toast({ title: '删除失败', description: String(e), variant: 'danger' });
    } finally {
      setDeleting(false);
    }
  };

  const handleImport = async () => {
    try {
      const acc = await importCurrentLogin(agent);
      await load(agent);
      toast({
        title: '已导入当前登录态',
        description: `${acc.label} 已入库`,
        variant: 'success',
      });
    } catch (e) {
      toast({ title: '导入失败', description: String(e), variant: 'danger' });
    }
  };

  const handleRefreshToken = async (acc: Account) => {
    try {
      await refreshToken(agent, acc.id);
      await load(agent);
      toast({ title: 'Token 已刷新', description: acc.label, variant: 'success' });
    } catch (e) {
      toast({ title: '刷新失败', description: String(e), variant: 'danger' });
    }
  };

  const handleOpenConfigDir = async (acc: Account) => {
    try {
      const path = await openAgentConfigDir(acc.agentId);
      toast({
        title: '已打开配置目录',
        description: path,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: '打开配置目录失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  // OAuth 完成后询问"立即切换?"(docs/ui-design.md §7.3)
  const handleOAuthCompleted = (acc: Account) => {
    load(agent).then(() => setSwitchTarget(acc));
  };

  const switchPreview: SwitchPreview | undefined = switchTarget
    ? {
        backfillSummary: current
          ? `当前凭据将回存为「${current.label}」`
          : '当前没有需要先保存的生效凭据',
        backupPath: `~/.agenthub/backups/${agent}/`,
        processWarning: agentStatuses.find((s) => s.agentId === agent)?.running
          ? `${meta.name} 正在运行，切换后需重启生效`
          : undefined,
      }
    : undefined;

  const addAccountMenu = (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button>
          <Plus className="h-4 w-4" /> 添加账号/密钥 <ChevronDown className="h-3.5 w-3.5" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => setOauthOpen(true)}>
          <LogIn className="h-4 w-4" /> OAuth 登录
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => setApiKeyOpen(true)}>
          <KeyRound className="h-4 w-4" /> API Key
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => handleImport()}>
          <DownloadCloud className="h-4 w-4" /> 导入当前登录态
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );

  const kindFilterBar =
    phase === 'ready' && accounts.length > 0 ? (
      <SegmentedControl
        value={kindFilter}
        onChange={setKindFilter}
        aria-label="认证方式筛选"
        options={ACCOUNT_KIND_FILTERS.map((filter) => ({
          value: filter.value,
          label: filter.label,
          count: kindCounts[filter.value],
        }))}
      />
    ) : null;

  return (
    <div>
      {!embedded ? (
        <PageHeader
          title="账号与密钥"
          description="官方登录与 API Key"
          descriptionTip="切换前自动保存当前凭据并备份。同时只能有一条生效。"
          actions={addAccountMenu}
        />
      ) : (
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          {kindFilterBar ?? <span />}
          {(phase === 'ready' || phase === 'error') && addAccountMenu}
        </div>
      )}

      {!embedded && (
        <div className="mb-3">
          <AgentTabStrip
            value={agent}
            onChange={setAgent}
            disabled={accountDisabledAgents(agentStatuses)}
            disabledReason={
              agentStatuses.find((s) => s.agentId === agent)?.capabilities?.accountSwitch
                ?.reason ?? '暂不支持账号切换'
            }
          />
        </div>
      )}

      {!embedded && kindFilterBar && (
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          {kindFilterBar}
          <span className="text-xs text-muted">
            显示 {visibleAccounts.length} / {accounts.length} 条
          </span>
        </div>
      )}
      {phase === 'loading' && <ListSkeleton rows={3} />}

      {phase === 'error' && <ErrorState error={error} onRetry={() => load(agent)} />}

      {phase === 'ready' && accounts.length === 0 && (
        <EmptyState
          icon={UserCircle}
          title={`${meta.name} 暂无账号或密钥`}
          description="可官方登录、添加 API Key，或导入本机登录态"
          // 嵌入态工具栏已有「添加」；独立页用 EmptyState 主按钮
          actionLabel={embedded ? undefined : '添加账号或密钥'}
          onAction={embedded ? undefined : () => setOauthOpen(true)}
        />
      )}
      {phase === 'ready' && accounts.length > 0 && visibleAccounts.length === 0 && (
        <EmptyState
          icon={KeyRound}
          title="没有匹配的凭据"
          description={
            kindFilter === 'oauth'
              ? '筛选为官方登录；可改筛选或添加登录'
              : kindFilter === 'apikey'
                ? '筛选为 API Key；可改筛选或添加密钥'
                : '请调整筛选条件'
          }
          actionLabel="显示全部"
          onAction={() => setKindFilter('all')}
        />
      )}

      {phase === 'ready' && visibleAccounts.length > 0 && (
        <div className="flex flex-col gap-4">
          {identityGroups.map((group) => {
            const multiAuth = group.accounts.length > 1;
            return (
              <section key={group.identity} className="flex flex-col gap-2">
                {multiAuth || identityGroups.length > 1 ? (
                  <header className="flex items-baseline gap-2 px-0.5">
                    <h3 className="truncate text-sm font-medium text-primary">
                      {group.identity}
                    </h3>
                    {multiAuth && (
                      <span className="shrink-0 text-xs text-muted">
                        {group.accounts.length} 个授权 · 仅一条生效
                      </span>
                    )}
                  </header>
                ) : null}
                <div className="flex flex-col gap-2">
                  {group.accounts.map((acc) => (
                    <AccountCard
                      key={acc.id}
                      account={acc}
                      switching={switching}
                      grouped={multiAuth}
                      onSwitch={setSwitchTarget}
                      onDelete={setDeleteTarget}
                      onRefreshToken={handleRefreshToken}
                      onEdit={setEditTarget}
                      onOpenConfigDir={handleOpenConfigDir}
                    />
                  ))}
                </div>
              </section>
            );
          })}
        </div>
      )}
      {/* 切换确认(backfill + 备份 + 进程警告) */}
      <SwitchConfirmDialog
        open={!!switchTarget}
        onOpenChange={(v) => !v && setSwitchTarget(null)}
        targetName={switchTarget?.label ?? ''}
        preview={switchPreview}
        loading={switching}
        onConfirm={() => confirmSwitch()}
      />

      {/* 删除二次确认 */}
      <Dialog open={!!deleteTarget} onOpenChange={(v) => !v && setDeleteTarget(null)}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>删除凭据 "{deleteTarget?.label}"?</DialogTitle>
            <DialogDescription>
              将从凭据池移除该项（官方登录或 API Key），此操作不可撤销。不修改本机 live，除非该项正是当前生效项的池记录。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              取消
            </Button>
            <Button variant="danger" disabled={deleting} onClick={() => confirmDelete()}>
              {deleting ? '删除中…' : '删除'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <OAuthFlowDialog
        agentId={agent}
        open={oauthOpen}
        onOpenChange={setOauthOpen}
        onCompleted={handleOAuthCompleted}
      />
      <ApiKeyAccountDialog
        agentId={agent}
        mode="add"
        open={apiKeyOpen}
        onOpenChange={setApiKeyOpen}
        onSaved={() => load(agent)}
      />
      <ApiKeyAccountDialog
        agentId={agent}
        mode="edit"
        account={editTarget}
        open={!!editTarget}
        onOpenChange={(v) => !v && setEditTarget(null)}
        onSaved={() => {
          setEditTarget(null);
          void load(agent);
        }}
      />
    </div>
  );
}
