import { UsageParserHealth } from '@/components/shared/UsageParserHealth';

/**
 * 兼容旧路径：紧凑用量解析健康条。
 * 主展示在 Dashboard（`UsageParserHealth` variant="dashboard"）；
 * compact 不再挂在 Agents 页。新代码请直接用 `UsageParserHealth`。
 */
export function UsageHealthStrip({
  className,
  refreshKey = 0,
}: {
  className?: string;
  refreshKey?: number;
}) {
  return <UsageParserHealth variant="compact" refreshKey={refreshKey} className={className} />;
}
