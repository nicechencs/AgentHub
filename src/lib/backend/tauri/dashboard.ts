import type { DashboardPort } from '@/lib/backend/contracts';
import type { DashboardAlert } from '@/lib/types';

/**
 * 告警聚合尚未接线：生产返回空列表，不展示模拟通知。
 */
export function createTauriDashboardPort(): DashboardPort {
  return {
    async listAlerts(): Promise<DashboardAlert[]> {
      return [];
    },

    async dismissAlert(_id: string): Promise<void> {
      // no-op until real alert source exists
    },
  };
}
