import { Boxes } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { EmptyState } from '@/components/shared/EmptyState';

/**
 * Adapter 管理页（原 Router 占位）。
 * 产品语义：各 Agent 适配层的能力与接入状态，而非请求转发代理。
 */
export default function AdapterPage() {
  return (
    <div>
      <PageHeader
        title="Adapter"
        description="各 Agent 适配层能力与接入状态（规划中）"
      />
      <EmptyState
        icon={Boxes}
        title="规划中"
        description="将展示适配层能力矩阵、接入状态与诊断入口；当前请用 Agents 与能力矩阵了解支持范围。"
      />
    </div>
  );
}
