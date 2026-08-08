import { UsageParserHealth } from '@/components/shared/UsageParserHealth';

/**
 * 兼容旧路径：Dashboard 用量段数据源状态条。
 * 新代码请直接使用 `UsageParserHealth`（variant="dashboard"）。
 */
export function ParserHealthBar({ refreshKey = 0 }: { refreshKey?: number }) {
  return <UsageParserHealth variant="dashboard" refreshKey={refreshKey} />;
}
