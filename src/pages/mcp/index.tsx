import { Plug } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { EmptyState } from '@/components/shared/EmptyState';

/** MCP 管理（占位） */
export default function McpPage() {
  return (
    <div>
      <PageHeader title="MCP" description="Model Context Protocol 服务与工具" />
      <EmptyState icon={Plug} title="规划中" description="该功能正在规划中，稍后开放。" />
    </div>
  );
}
