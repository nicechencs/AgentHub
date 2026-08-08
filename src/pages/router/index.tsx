import { Route } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { EmptyState } from '@/components/shared/EmptyState';

/** Router 路由管理（占位） */
export default function RouterPage() {
  return (
    <div>
      <PageHeader title="Router" description="请求路由与转发" />
      <EmptyState icon={Route} title="规划中" description="该功能正在规划中，稍后开放。" />
    </div>
  );
}
