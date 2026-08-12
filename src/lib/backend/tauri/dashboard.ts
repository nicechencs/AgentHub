import type { DashboardPort } from '@/lib/backend/contracts';
import type { DashboardAlert } from '@/lib/types';
import { logger } from '@/lib/logger';
import { createTauriDoctorPort } from './doctor';
import {
  buildAlertsFromAgents,
  dismissAlertLocal,
  filterDismissedAlerts,
} from './dashboard-alerts';

const log = logger.scope('backend:tauri:dashboard');

/**
 * Production alerts derived from doctor-mapped agent status (auth / env / updates).
 * No demo notifications. Dismiss is local until the condition fingerprint changes.
 */
export function createTauriDashboardPort(): DashboardPort {
  const doctor = createTauriDoctorPort();
  let lastBuilt: DashboardAlert[] = [];

  return {
    async listAlerts(): Promise<DashboardAlert[]> {
      try {
        const mapped = await doctor.loadDoctorMapped();
        lastBuilt = buildAlertsFromAgents(mapped.agents);
        return filterDismissedAlerts(lastBuilt);
      } catch (e) {
        log.warn('listAlerts failed; returning empty', e);
        return [];
      }
    },

    async dismissAlert(id: string): Promise<void> {
      dismissAlertLocal(id, lastBuilt);
    },
  };
}
