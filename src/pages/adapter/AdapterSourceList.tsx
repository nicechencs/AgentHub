import { Boxes } from 'lucide-react';
import { EmptyState } from '@/components/shared/EmptyState';
import { ErrorState } from '@/components/shared/ErrorState';
import { ListRow } from '@/components/shared/ListRow';
import { SegmentedControl } from '@/components/shared/SegmentedControl';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { Skeleton } from '@/components/ui/skeleton';
import { kindBadge } from '@/pages/connections/connection-model';
import type { ConnectionEntry } from '@/pages/connections/connection-model';
import {
  ADAPTER_CREDENTIAL_FILTERS,
  adapterCredentialFilterLabel,
  sourceKindLabel,
  sourceStatusHint,
  targetAgentName,
  type AdapterCredentialFilter,
} from './adapter-model';
import { isOAuthAuthIncomplete } from './adapter-sources';

export type AdapterSourceListProps = {
  groups: Array<{
    id: string;
    label: string;
    entries: ConnectionEntry[];
  }>;
  selectedKey: string;
  filter: AdapterCredentialFilter;
  counts: Record<AdapterCredentialFilter, number>;
  query: string;
  loading: boolean;
  loadError: unknown;
  totalCount: number;
  visibleCount: number;
  onSelect: (entry: ConnectionEntry) => void;
  onFilterChange: (filter: AdapterCredentialFilter) => void;
  onQueryChange: (query: string) => void;
  onRetry: () => void;
  onGoConnections: (filter: AdapterCredentialFilter) => void;
};

export function AdapterSourceList({
  groups,
  selectedKey,
  filter,
  counts,
  query,
  loading,
  loadError,
  totalCount,
  visibleCount,
  onSelect,
  onFilterChange,
  onQueryChange,
  onRetry,
  onGoConnections,
}: AdapterSourceListProps) {
  return (
    <div className="flex h-full min-h-0 flex-col gap-3">
      <div className="space-y-1">
        <h2 className="text-sm font-medium">可用连接</h2>
        <p className="text-xs text-secondary">选择一条 Connection 作为适配来源。凭据仍留在 Connections。</p>
      </div>
      <Input
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder="搜索名称、Agent 或凭据类型"
        aria-label="搜索可用连接"
      />
      <SegmentedControl
        value={filter}
        onChange={onFilterChange}
        aria-label="凭据类型筛选"
        options={ADAPTER_CREDENTIAL_FILTERS.map((item) => ({
          value: item,
          label: adapterCredentialFilterLabel(item),
          count: counts[item],
        }))}
      />
      <div className="min-h-0 flex-1 overflow-y-auto">
        {loading ? (
          <div className="space-y-2" aria-live="polite">
            <Skeleton className="h-14 w-full" />
            <Skeleton className="h-14 w-full" />
          </div>
        ) : loadError ? (
          <ErrorState
            compact
            error={loadError}
            title="无法读取连接"
            onRetry={onRetry}
          />
        ) : totalCount === 0 ? (
          <EmptyState
            icon={Boxes}
            title="还没有可用来源"
            description="先在 Connections 保存 API Key 或完成官方登录。Adapter 只引用 connectionId，不复制凭据。"
            actionLabel="去 Connections"
            onAction={() => onGoConnections('all')}
          />
        ) : visibleCount === 0 ? (
          <EmptyState
            icon={Boxes}
            title="没有匹配的连接"
            description={query.trim()
              ? '换个关键词，或把筛选改回全部。'
              : filter === 'oauth'
                ? '当前没有官方登录。可前往 Connections 完成授权。'
                : '当前没有 API Key。可前往 Connections 添加。'}
            actionLabel={query.trim() ? undefined : '去 Connections'}
            onAction={query.trim() ? undefined : () => onGoConnections(filter)}
          />
        ) : (
          <div className="space-y-3">
            {groups.map((group) => (
              <section key={group.id} className="space-y-1.5">
                <h3 className="px-0.5 text-xs font-medium text-secondary">{group.label}</h3>
                <div className="space-y-1.5">
                  {group.entries.map((entry) => {
                    const badge = kindBadge(entry.kind);
                    const incomplete = isOAuthAuthIncomplete(entry);
                    return (
                      <ListRow
                        key={entry.key}
                        active={entry.key === selectedKey}
                        className="cursor-pointer p-3"
                        role="button"
                        tabIndex={0}
                        aria-pressed={entry.key === selectedKey}
                        onClick={() => onSelect(entry)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter' || event.key === ' ') {
                            event.preventDefault();
                            onSelect(entry);
                          }
                        }}
                      >
                        <div className="min-w-0 space-y-1">
                          <div className="flex flex-wrap items-center gap-1.5">
                            <p className="truncate text-sm font-medium">{entry.title}</p>
                            <Badge variant={badge.variant}>{badge.label}</Badge>
                            {entry.isCurrent ? <Badge variant="success">当前</Badge> : null}
                          </div>
                          <p className="truncate text-xs text-secondary">
                            {targetAgentName(entry.agentId)} · {sourceKindLabel(entry.source)}
                          </p>
                          <p className="truncate text-xs text-muted">{sourceStatusHint(entry)}</p>
                          {incomplete ? (
                            <p className="text-xs text-warning">授权未完成，需先到 Connections 登录</p>
                          ) : null}
                        </div>
                      </ListRow>
                    );
                  })}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
