import type { DashboardAlert } from '@/lib/types';

export interface DashboardPort {
  listAlerts(): Promise<DashboardAlert[]>;
  dismissAlert(id: string): Promise<void>;
}
