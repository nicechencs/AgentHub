import * as React from 'react';
import { AlertTriangle, Check } from 'lucide-react';
import { AGENT_MAP } from '@/config/agents';
import { tryLoadDoctorMapped } from '@/lib/api/doctor';
import { missingPricingModels, parserHealth } from '@/lib/api/usage';
import type { AgentId, ParserHealth } from '@/lib/types';
import { Card } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { Tip } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';

function fmtRecords(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

type Row = {
  agentId: AgentId;
  supported: boolean;
  records: number;
  failRatePct?: number | null;
  skipped?: number | null;
};

function toRowsFromParser(list: ParserHealth[]): Row[] {
  return list.map((h) => ({
    agentId: h.agentId,
    supported: h.supported,
    records: h.records,
    failRatePct: h.failRatePct,
    skipped: h.skipped,
  }));
}

function DashboardItem({ h }: { h: Row }) {
  const name = AGENT_MAP[h.agentId]?.name ?? h.agentId;
  if (!h.supported) {
    return (
      <Tip className="text-muted" label="该 Agent 无本地会话日志解析源">
        {name} —
      </Tip>
    );
  }
  if (h.failRatePct != null && h.failRatePct > 0) {
    return (
      <Tip
        className="text-warning"
        label={`失败率 ${h.failRatePct}%${h.skipped != null ? `，跳过 ${h.skipped} 条` : ''}。日志格式可能已变更。`}
      >
        <AlertTriangle className="inline h-3 w-3 align-text-bottom" /> {name} {h.failRatePct}%
      </Tip>
    );
  }
  return (
    <span>
      {name}{' '}
      <Check className="inline h-3 w-3 align-text-bottom text-success" /> {fmtRecords(h.records)}
    </span>
  );
}

/**
 * 用量解析健康（主入口：Dashboard 用量段）。
 * - variant=dashboard：usage API parserHealth + 缺价模型
 * - variant=compact：doctor report soft-fail（不可用则隐藏；兼容 re-export，页面默认不再挂）
 */
export function UsageParserHealth({
  variant = 'dashboard',
  refreshKey = 0,
  className,
}: {
  variant?: 'dashboard' | 'compact';
  refreshKey?: number;
  className?: string;
}) {
  const [rows, setRows] = React.useState<Row[] | null>(null);
  const [missing, setMissing] = React.useState<string[]>([]);
  const [failed, setFailed] = React.useState(false);

  React.useEffect(() => {
    let alive = true;
    setFailed(false);

    if (variant === 'compact') {
      tryLoadDoctorMapped()
        .then((mapped) => {
          if (!alive) return;
          const health = mapped?.report?.usageHealth;
          if (!health?.length) {
            setRows(null);
            return;
          }
          setRows(
            health.map((h) => ({
              agentId: h.agentId,
              supported: h.supported,
              records: h.records,
              failRatePct: h.failRatePct,
              skipped: null,
            })),
          );
        })
        .catch(() => {
          if (alive) setRows(null);
        });
      return () => {
        alive = false;
      };
    }

    Promise.all([parserHealth(), missingPricingModels(30).catch(() => [] as string[])])
      .then(([h, m]) => {
        if (!alive) return;
        setRows(toRowsFromParser(h));
        setMissing(m);
      })
      .catch(() => {
        if (alive) setFailed(true);
      });

    return () => {
      alive = false;
    };
  }, [refreshKey, variant]);

  if (variant === 'compact') {
    if (!rows) return null;
    const supported = rows.filter((r) => r.supported);
    const withData = supported.filter((r) => r.records > 0);
    const totalRecords = supported.reduce((s, r) => s + r.records, 0);

    return (
      <Card
        className={cn(
          'bg-panel/60 px-3 py-2 text-xs text-secondary shadow-none',
          className,
        )}
      >
        <div className="mb-1 flex items-center justify-between gap-2">
          <span className="font-medium text-primary">用量解析</span>
          <span className="text-muted">
            {withData.length}/{supported.length} 有数据 · 共 {totalRecords.toLocaleString()} 条
          </span>
        </div>
        <div className="flex flex-wrap gap-x-3 gap-y-1">
          {rows.map((h) => {
            const name = AGENT_MAP[h.agentId]?.name ?? h.agentId;
            if (!h.supported) {
              return (
                <span key={h.agentId} className="text-muted">
                  {name} —
                </span>
              );
            }
            const warn = h.failRatePct != null && h.failRatePct >= 10;
            const tip = warn
              ? `失败率 ${h.failRatePct}%`
              : h.records === 0
                ? '可解析，尚无入库记录'
                : undefined;
            return (
              <Tip
                key={h.agentId}
                className={warn ? 'text-warning' : undefined}
                label={tip}
              >
                {name}{' '}
                {h.records > 0 ? (
                  <span className="text-success">
                    <Check className="inline h-3 w-3 align-text-bottom" />
                    {h.records}
                  </span>
                ) : (
                  <span className="text-muted">·0</span>
                )}
              </Tip>
            );
          })}
        </div>
      </Card>
    );
  }

  if (failed) {
    return (
      <p className={cn('mt-4 text-xs text-muted', className)}>
        数据源状态加载失败，其余数据不受影响。
      </p>
    );
  }
  if (!rows) {
    return <Skeleton className={cn('mt-4 h-4 w-2/3', className)} />;
  }

  return (
    <div className={cn('mt-4 space-y-1.5', className)}>
      <p className="text-xs text-secondary">
        <span className="text-muted">解析：</span>{' '}
        {rows.map((h, i) => (
          <span key={h.agentId}>
            {i > 0 && <span className="mx-1.5 text-muted">·</span>}
            <DashboardItem h={h} />
          </span>
        ))}
      </p>
      {missing.length > 0 && (
        <Tip
          className="text-xs text-warning"
          label={`无内嵌单价，成本记为 $0：${missing.join(', ')}。可 pnpm pricing:update 从 LiteLLM 刷新，或在 scripts/pricing/overrides.json 补价。`}
        >
          <AlertTriangle className="mr-1 inline h-3 w-3 align-text-bottom" />
          缺价（成本 $0）{missing.slice(0, 4).join(', ')}
          {missing.length > 4 ? ` 等${missing.length}个` : ''}
        </Tip>
      )}
    </div>
  );
}
