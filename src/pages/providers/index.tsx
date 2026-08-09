// Providers API 配置面板(docs/ui-design.md §4.3 / Connections §4.3b):
// 卡片列表 + 弹窗编辑：智能识别 URL / API Key，不依赖预设列表。
// 切换走 switchPreview → SwitchConfirmDialog → switchProvider → toast(可撤销)。
// 可独立路由,也可嵌入 Connections(embedded=true,agent 由父级控制)。
import { useCallback, useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Cable, FolderOpen, Import, Pencil, Plus, Trash2 } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentTabStrip } from '@/components/layout/AgentTabStrip';
import { SwitchConfirmDialog } from '@/components/shared/SwitchConfirmDialog';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { ListSkeleton } from '@/components/ui/skeleton';
import { Badge } from '@/components/ui/badge';
import { useToast } from '@/components/ui/toast';
import { AGENT_IDS, AGENT_MAP } from '@/config/agents';
import { openAgentConfigDir } from '@/lib/api/install';
import {
  deleteProvider,
  importProviderLive,
  listProviders,
  switchPreview,
  switchProvider,
  testLatency,
  undoSwitch,
} from '@/lib/api/provider';
import { liveConfigPaths } from '@/lib/provider-detect';
import type { AgentId, Provider, SwitchPreview } from '@/lib/types';
import { cn } from '@/lib/utils';
import { ProviderEditDialog } from './ProviderEditDialog';

function parseAgentParam(raw: string | null): AgentId {
  if (raw && (AGENT_IDS as string[]).includes(raw)) return raw as AgentId;
  return 'claude';
}

export interface ProvidersPanelProps {
  /** 嵌入 Connections 时隐藏页头与 AgentTabStrip */
  embedded?: boolean;
  /** 受控 agent;不传则内部自管(并同步 ?agent=) */
  agentId?: AgentId;
  onAgentIdChange?: (id: AgentId) => void;
  /** 供应商池变更后通知父级刷新 Tab 角标 */
  onPoolChanged?: () => void;
}

export default function ProvidersPage({
  embedded = false,
  agentId: controlledAgentId,
  onAgentIdChange,
  onPoolChanged,
}: ProvidersPanelProps = {}) {
  const { toast } = useToast();
  const [searchParams, setSearchParams] = useSearchParams();
  const controlled = controlledAgentId !== undefined;

  const [internalAgentId, setInternalAgentId] = useState<AgentId>(() =>
    parseAgentParam(searchParams.get('agent')),
  );
  const agentId = controlled ? controlledAgentId : internalAgentId;

  const [providers, setProviders] = useState<Provider[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [importing, setImporting] = useState(false);
  const [deletingAll, setDeletingAll] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [switching, setSwitching] = useState(false);
  const [switchState, setSwitchState] = useState<{
    target: Provider;
    preview: SwitchPreview;
  } | null>(null);
  const [retryCount, setRetryCount] = useState(0);

  // 弹窗：add | edit
  const [addOpen, setAddOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<Provider | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [latencyById, setLatencyById] = useState<Record<string, number>>({});

  // 独立页:Dashboard 深链 /providers?agent=xxx 同步 tab
  useEffect(() => {
    if (controlled) return;
    const fromUrl = parseAgentParam(searchParams.get('agent'));
    if (fromUrl !== internalAgentId) setInternalAgentId(fromUrl);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, controlled]);

  const handleAgentChange = (id: AgentId) => {
    if (controlled) {
      onAgentIdChange?.(id);
      return;
    }
    setInternalAgentId(id);
    setSearchParams(id === 'claude' ? {} : { agent: id }, { replace: true });
  };

  const brandColor = AGENT_MAP[agentId].color;

  const refresh = useCallback(
    async (agent: AgentId) => {
      const list = await listProviders(agent);
      setProviders(list);
      onPoolChanged?.();
    },
    [onPoolChanged],
  );

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    setProviders([]);
    setEditTarget(null);
    setAddOpen(false);
    listProviders(agentId)
      .then((list) => {
        if (cancelled) return;
        setProviders(list);
        onPoolChanged?.();
      })
      .catch((e) => {
        if (!cancelled) setError(e);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [agentId, retryCount, onPoolChanged]);

  const handleImport = async () => {
    // 设计：import 会立刻在供应商池落一条记录（不是草稿），再打开编辑便于改名/微调
    if (
      !window.confirm(
        '将读取本机当前配置并新增一条供应商记录，导入后可编辑或删除。是否继续？',
      )
    ) {
      return;
    }
    setImporting(true);
    try {
      const imported = await importProviderLive(agentId);
      toast({
        title: `已导入「${imported.name}」`,
        description: '已写入列表；可改名后保存，不需要可删除。',
        variant: 'success',
      });
      await refresh(agentId);
      setEditTarget(imported);
    } catch (e) {
      toast({
        title: '导入失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setImporting(false);
    }
  };

  const openSwitchDialog = async (p: Provider) => {
    if (p.isCurrent) return;
    setPreviewLoading(true);
    try {
      const preview = await switchPreview(agentId, p.id);
      setSwitchState({ target: p, preview });
    } catch (e) {
      toast({
        title: '无法预览切换',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setPreviewLoading(false);
    }
  };

  const confirmSwitch = async () => {
    if (!switchState) return;
    const target = switchState.target;
    setSwitching(true);
    try {
      await switchProvider(agentId, target.id);
      setSwitchState(null);
      await refresh(agentId);
      toast({
        title: `已切换到 "${target.name}"`,
        variant: 'success',
        actionLabel: '撤销',
        duration: 5000,
        onAction: () => {
          void undoSwitch(agentId)
            .then((ok) => {
              if (ok === false) {
                toast({
                  title: '无法撤销',
                  description: '当前 backend 不支持撤销切换',
                  variant: 'danger',
                });
                return;
              }
              return refresh(agentId);
            })
            .catch((e) => {
              toast({
                title: '撤销失败',
                description: e instanceof Error ? e.message : String(e),
                variant: 'danger',
              });
            });
        },
      });
    } catch (e) {
      toast({
        title: '切换失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setSwitching(false);
    }
  };

  const handleDelete = async (p: Provider) => {
    const msg = p.isCurrent
       ? `「${p.name}」是当前连接。删除只移入 AgentHub 回收站，不会修改本机配置文件（如 ~/.claude/settings.json / ~/.codex/config.toml）。确定继续？`
       : `确定删除供应商「${p.name}」？记录会移入回收站，本机配置文件不会修改。`;
    if (!window.confirm(msg)) return;
    try {
      await deleteProvider(agentId, p.id);
      toast({ title: `已将「${p.name}」移入回收站` });
      await refresh(agentId);
    } catch (e) {
      toast({
        title: '删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  };

  /** 清空当前 Agent 供应商池内全部记录（不改本机配置） */
  const handleDeleteAll = async () => {
    if (providers.length === 0) return;
    const n = providers.length;
    if (
      !window.confirm(
        `确定删除 ${AGENT_MAP[agentId].name} 的全部 ${n} 条供应商配置？\n记录会移入回收站，不会修改本机配置文件。`,
      )
    ) {
      return;
    }
    setDeletingAll(true);
    try {
      // 逐条删除（含当前项）；后端允许删 is_current
      for (const p of providers) {
        await deleteProvider(agentId, p.id);
      }
      setEditTarget(null);
      toast({ title: `已将全部 ${n} 条供应商移入回收站` });
      await refresh(agentId);
    } catch (e) {
      toast({
        title: '批量删除失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
      await refresh(agentId);
    } finally {
      setDeletingAll(false);
    }
  };

  const handleTest = async (p: Provider) => {
    setTestingId(p.id);
    try {
      const ms = await testLatency(agentId, p.id);
      setLatencyById((prev) => ({ ...prev, [p.id]: ms }));
    } catch (e) {
      toast({
        title: '测速失败',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setTestingId(null);
    }
  };

  const openEdit = (p: Provider) => setEditTarget(p);

  const openAdd = () => {
    setAddOpen(true);
  };

  const handleOpenConfigDir = async () => {
    try {
      const path = await openAgentConfigDir(agentId);
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

  const livePaths = liveConfigPaths(agentId);

  const addProviderActions = (
    <div className="flex items-center gap-2">
      {providers.length > 0 && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => void handleDeleteAll()}
          disabled={loading || deletingAll || importing}
        title="清空当前 Agent 供应商池（不改本机配置）"
        >
          <Trash2 className="h-3.5 w-3.5 text-danger" />
          {deletingAll ? '删除中…' : '删除全部'}
        </Button>
      )}
      <Button
        variant="outline"
        size="sm"
        onClick={() => void handleOpenConfigDir()}
        disabled={loading}
        title={`打开本机配置目录：${livePaths.openDir}`}
      >
        <FolderOpen className="h-3.5 w-3.5" />
        打开配置目录
      </Button>
      <Button
        variant="outline"
        size="sm"
        onClick={() => void handleImport()}
        disabled={loading || importing || deletingAll}
         title="读取本机配置并保存到供应商列表"
      >
        <Import className="h-3.5 w-3.5" />
         {importing ? '导入中…' : '从本机配置导入'}
      </Button>
      <Button size="sm" onClick={openAdd} disabled={loading || deletingAll}>
        <Plus className="h-3.5 w-3.5" /> 添加供应商
      </Button>
    </div>
  );

  return (
    <div>
      {!embedded ? (
        <>
          <PageHeader
            title="API 配置"
            description="供应商与连接"
            descriptionTip="为各 Agent 管理 API 供应商；粘贴配置可智能识别 URL 与 API Key。"
            actions={addProviderActions}
          />
          <div className={pageRhythm.chrome}>
            <AgentTabStrip value={agentId} onChange={handleAgentChange} />
          </div>
        </>
      ) : !loading ? (
        <div className={cn(pageRhythm.chrome, 'flex items-center justify-end')}>
          {addProviderActions}
        </div>
      ) : null}

      {error ? (
        <ErrorState error={error} onRetry={() => setRetryCount((c) => c + 1)} />
      ) : (
        <div>
          {loading && <ListSkeleton rows={3} />}

          {!loading && providers.length === 0 && (
            <EmptyState
              icon={Cable}
              title="还没有供应商"
              description="添加中转/自部署配置，或从本机配置导入"
              // 嵌入态工具栏已有添加；独立页用 EmptyState 主按钮
              actionLabel={embedded ? undefined : '添加供应商'}
              onAction={embedded ? undefined : openAdd}
            />
          )}

          {!loading && providers.length > 0 && (
            <div className={pageRhythm.stackDense}>
              {providers.map((p) => {
                const latency = latencyById[p.id] ?? p.latencyMs;
                return (
                  <Card
                    key={p.id}
                    onClick={() => openEdit(p)}
                    className={cn(
                      'cursor-pointer p-3 transition-colors hover:border-border-strong/80',
                      p.isCurrent ? 'border-border-strong' : undefined,
                    )}
                    style={
                      p.isCurrent
                        ? { borderLeft: `3px solid ${brandColor}` }
                        : undefined
                    }
                  >
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                      <div className="flex min-w-0 flex-1 items-center gap-2">
                        <span
                          className={cn(
                            'text-xs',
                            p.isCurrent ? 'text-success' : 'text-muted',
                          )}
                          aria-hidden
                        >
                          {p.isCurrent ? '●' : '○'}
                        </span>
                        <span className="truncate text-sm font-medium">{p.name}</span>
                        <Badge variant="info">供应商</Badge>
                        {p.isCurrent && <Badge variant="accent">当前</Badge>}
                        {latency != null && (
                          <span className="font-mono text-xs text-muted">{latency} ms</span>
                        )}
                      </div>

                      <div
                        className="flex shrink-0 items-center gap-2"
                        onClick={(e) => e.stopPropagation()}
                      >
                        {!p.isCurrent && (
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={switching || previewLoading}
                            onClick={() => void openSwitchDialog(p)}
                          >
                            切换
                          </Button>
                        )}
                        <Button size="sm" variant="secondary" onClick={() => openEdit(p)}>
                          <Pencil className="h-3.5 w-3.5" /> 编辑
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={testingId === p.id}
                          onClick={() => void handleTest(p)}
                        >
                          {testingId === p.id ? '测速中…' : '测速'}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={deletingAll}
                          aria-label="删除供应商"
                          title={
                            p.isCurrent
                              ? '删除池内记录（当前项也可删；不改本机配置）'
                              : '删除供应商'
                          }
                          onClick={() => void handleDelete(p)}
                        >
                          <Trash2 className="h-3.5 w-3.5 text-danger" />
                        </Button>
                      </div>
                    </div>

                    <p className="mt-1 pl-5 text-xs text-muted">
                      {p.isCurrent
                        ? `当前生效 · 本机配置：${livePaths.config}`
                        : `未生效 · 切换后写入本机配置：${livePaths.config}`}
                      {livePaths.auth ? ` · 凭据 ${livePaths.auth}` : ''}
                    </p>
                  </Card>
                );
              })}
            </div>
          )}
        </div>
      )}

      <ProviderEditDialog
        agentId={agentId}
        mode="add"
        open={addOpen}
        onOpenChange={setAddOpen}
        onSaved={() => {
          setAddOpen(false);
          void refresh(agentId);
        }}
      />

      <ProviderEditDialog
        agentId={agentId}
        mode="edit"
        provider={editTarget}
        open={!!editTarget}
        onOpenChange={(v) => !v && setEditTarget(null)}
        onSaved={() => {
          setEditTarget(null);
          void refresh(agentId);
        }}
      />

      <SwitchConfirmDialog
        open={switchState !== null}
        onOpenChange={(v) => !v && setSwitchState(null)}
        targetName={switchState?.target.name ?? ''}
        preview={switchState?.preview}
        loading={switching}
        onConfirm={() => void confirmSwitch()}
      />
    </div>
  );
}
