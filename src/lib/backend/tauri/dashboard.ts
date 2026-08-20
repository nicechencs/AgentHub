import type { Backend, DashboardPort } from '@/lib/backend/contracts';
import type { DashboardAlert } from '@/lib/types';
import { createTranslator, loadStoredLanguage } from '@/lib/i18n';
import { logger } from '@/lib/logger';
import {
  buildAlertsFromAgents,
  dismissAlertLocal,
  filterDismissedAlerts,
} from './dashboard-alerts';

const log = logger.scope('backend:tauri:dashboard');

/**
 * Production alerts derived from listAgents (doctor + pool enrichment).
 * No demo notifications. Dismiss is local until the condition fingerprint changes.
 */
export function createTauriDashboardPort(backend: Backend): DashboardPort {
  let lastBuilt: DashboardAlert[] = [];

  return {
    async listAlerts(): Promise<DashboardAlert[]> {
      try {
        const agents = await backend.agent.listAgents();
        lastBuilt = buildAlertsFromAgents(agents, createTranslator(loadStoredLanguage()));
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
