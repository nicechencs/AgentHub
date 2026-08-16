import { useCallback, useEffect, useMemo, useState } from 'react';
import { FolderOpen, Loader2, Plug, RefreshCw } from 'lucide-react';
import { AgentTabStrip, type AgentTabId } from '@/components/layout/AgentTabStrip';
import { PageHeader } from '@/components/layout/PageHeader';
import { PageSection } from '@/components/layout/PageSection';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { AgentDot } from '@/components/shared/AgentDot';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { ListSkeleton } from '@/components/ui/skeleton';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { resolveAgentMeta, agentDisplayName, type AgentMeta } from '@/config/agents';
import { useInstalledAgents } from '@/lib/hooks/useInstalledAgents';
import { listMcpInventory } from '@/lib/api/mcp';
import { openPathInFileManager } from '@/lib/api/skill';
import type { McpInventory, McpServerEntry, McpSourceFile } from '@/lib/backend/contracts/mcp-types';
import type { AgentId } from '@/lib/types';

function agentName(id: AgentId): string {
  return agentDisplayName(id);
}

function transportLabel(t: string): string {
  switch (t) {
    case 'stdio':
      return 'stdio';
    case 'sse':
      return 'SSE';
    case 'http':
      return 'HTTP';
    default:
      return t || '未知';
  }
}

function parentDir(path: string): string {
  const norm = path.replace(/[/\\]+$/, '');
  const i = Math.max(norm.lastIndexOf('\\'), norm.lastIndexOf('/'));
  return i > 0 ? norm.slice(0, i) : norm;
}

export default function McpPage() {
  const { toast } = useToast();
  const { hiddenIds } = useInstalledAgents();
  const hiddenSet = useMemo(() => new Set(hiddenIds), [hiddenIds]);
  const [data, setData] = useState<McpInventory | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | string | null>(null);
  const [filterAgent, setFilterAgent] = useState<AgentTabId>('all');

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const inv = await listMcpInventory();
      setData(inv);
    } catch (e) {
      setError(e instanceof Error ? e : String(e));
      setData(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const filterAgents = useMemo(() => {
    if (!data) return [] as AgentMeta[];
    const ids = new Set<AgentId>();
    for (const s of data.sources) {
      if (s.exists || s.serverCount > 0) ids.add(s.agent);
    }
    for (const s of data.servers) ids.add(s.agent);
    return [...ids]
      .filter((id) => !hiddenSet.has(id))
      .sort((a, b) => a.localeCompare(b))
      .map((id) => resolveAgentMeta(id));
  }, [data, hiddenSet]);

  useEffect(() => {
    if (filterAgent !== 'all' && hiddenSet.has(filterAgent)) {
      setFilterAgent('all');
    }
  }, [filterAgent, hiddenSet]);

  const agentCounts = useMemo(() => {
    const visibleServers = data?.servers.filter((s) => !hiddenSet.has(s.agent)) ?? [];
    const counts: Partial<Record<AgentTabId, number>> = {
      all: visibleServers.length,
    };
    for (const s of visibleServers) {
      counts[s.agent] = (counts[s.agent] ?? 0) + 1;
    }
    return counts;
  }, [data, hiddenSet]);

  const servers = useMemo(() => {
    if (!data) return [] as McpServerEntry[];
    const visible = data.servers.filter((s) => !hiddenSet.has(s.agent));
    if (filterAgent === 'all') return visible;
    return visible.filter((s) => s.agent === filterAgent);
  }, [data, filterAgent, hiddenSet]);

  const sources = useMemo(() => {
    if (!data) return [] as McpSourceFile[];
    const visible = data.sources.filter((s) => !hiddenSet.has(s.agent));
    const list =
      filterAgent === 'all' ? visible : visible.filter((s) => s.agent === filterAgent);
    // Prefer existing / errored first for the "sources" strip
    return [...list].sort((a, b) => Number(b.exists) - Number(a.exists));
  }, [data, filterAgent, hiddenSet]);

  const existingSources = sources.filter((s) => s.exists);

  async function openSource(path: string) {
    try {
      await openPathInFileManager(parentDir(path));
    } catch (e) {
      toast({
        title: '无法打开目录',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    }
  }

  return (
    <div>
      <PageHeader
        title="MCP"
        description="只读扫描 · 不注入"
        descriptionTip="管理/注入仍为规划能力。此处仅汇总已发现的配置文件与 server 条目，便于排查与打开配置目录。"
        actions={
          <Button
            size="sm"
            variant="secondary"
            disabled={loading}
            onClick={() => void load()}
            className="gap-1.5"
          >
            {loading ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
            刷新
          </Button>
        }
      />

      {loading && !data ? (
        <ListSkeleton rows={5} />
      ) : error && !data ? (
        <ErrorState error={error} onRetry={() => void load()} />
      ) : (
        <>
          <div className={pageRhythm.chrome}>
            <AgentTabStrip
              showAll
              value={filterAgent}
              onChange={setFilterAgent}
              agents={filterAgents}
              counts={agentCounts}
              countMode="defined"
              countTitle={(id, n) => (id === 'all' ? `${n} 个 server` : `${agentName(id)} · ${n} 个`)}
              emptyLabel="尚未发现任何 MCP 配置"
              aria-label="按 Agent 筛选 MCP"
            />
          </div>

          {existingSources.length > 0 ? (
            <PageSection title="配置来源">
              <div className="grid gap-2 sm:grid-cols-2">
                {existingSources.map((s) => (
                  <Card key={`${s.agent}:${s.path}`}>
                    <CardContent className="flex items-start justify-between gap-3 p-3">
                      <div className="min-w-0">
                        <div className="flex items-center gap-1.5 text-sm font-medium text-primary">
                          <AgentDot agentId={s.agent} className="h-2 w-2" />
                          {agentName(s.agent)}
                          <span className="font-normal text-muted">· {s.label}</span>
                        </div>
                        <Tip label={s.path}>
                          <p className="mt-1 truncate font-mono text-2xs text-muted">{s.path}</p>
                        </Tip>
                        <p className="mt-1 text-xs text-secondary">
                          {s.readable
                            ? `${s.serverCount} 个 server`
                            : s.error
                              ? `读取失败：${s.error}`
                              : '不可读'}
                        </p>
                      </div>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="shrink-0 gap-1"
                        onClick={() => void openSource(s.path)}
                      >
                        <FolderOpen className="h-3.5 w-3.5" />
                        打开
                      </Button>
                    </CardContent>
                  </Card>
                ))}
              </div>
            </PageSection>
          ) : null}

          <PageSection title="Server 列表">
            {servers.length === 0 ? (
              <EmptyState
                icon={Plug}
                title="未发现 MCP server"
                description={
                  filterAgent === 'all'
                    ? '在各 Agent 官方配置中添加 MCP 后点刷新。AgentHub 当前只读展示，不会写入或注入。'
                    : `${agentName(filterAgent)} 下未解析到 server 条目。`
                }
                actionLabel="刷新"
                onAction={() => void load()}
              />
            ) : (
              <div className={pageRhythm.stackDense}>
                {servers.map((s) => (
                  <Card key={`${s.agent}:${s.name}:${s.sourcePath}`}>
                    <CardHeader className="flex flex-row items-center justify-between gap-3 space-y-0 p-3 pb-1">
                      <CardTitle className="flex min-w-0 items-center gap-2 text-sm font-medium">
                        <AgentDot agentId={s.agent} className="h-2 w-2" />
                        <span className="truncate">{s.name}</span>
                        <Badge>{transportLabel(s.transport)}</Badge>
                        {s.enabled === false ? <Badge variant="warning">已禁用</Badge> : null}
                      </CardTitle>
                      <span className="shrink-0 text-xs text-muted">{agentName(s.agent)}</span>
                    </CardHeader>
                    <CardContent className="space-y-1 px-3 pb-3 pt-0">
                      {s.command ? (
                        <Tip
                          className="block truncate font-mono text-2xs text-secondary"
                          label={s.command}
                        >
                          {s.command}
                        </Tip>
                      ) : null}
                      {s.url ? (
                        <Tip
                          className="block truncate font-mono text-2xs text-secondary"
                          label={s.url}
                        >
                          {s.url}
                        </Tip>
                      ) : null}
                      <div className="flex items-center justify-between gap-2 pt-1">
                        <Tip label={s.sourcePath}>
                          <p className="min-w-0 truncate text-2xs text-muted">
                            {s.sourceFormat} · {s.sourcePath}
                          </p>
                        </Tip>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-6 shrink-0 gap-1 px-1.5 text-xs"
                          onClick={() => void openSource(s.sourcePath)}
                        >
                          <FolderOpen className="h-3 w-3" />
                          目录
                        </Button>
                      </div>
                    </CardContent>
                  </Card>
                ))}
              </div>
            )}
          </PageSection>
        </>
      )}
    </div>
  );
}
