import type { Backend, DashboardPort } from '@/lib/backend/contracts';
import type { DashboardAlert } from '@/lib/types';
import { createTranslator, loadStoredLanguage } from '@/lib/i18n';
import {
  buildAlertsFromAgents,
  dismissAlertLocal,
  filterDismissedAlerts,
} from './dashboard-alerts';

/**
 * Production alerts derived from listAgents (doctor + pool enrichment).
 * No demo notifications. Dismiss is local until the condition fingerprint changes.
 */
export function createTauriDashboardPort(backend: Backend): DashboardPort {
  let lastBuilt: DashboardAlert[] = [];

  return {
    async listAlerts(): Promise<DashboardAlert[]> {
      const agents = await backend.agent.listAgents();
      lastBuilt = buildAlertsFromAgents(agents, createTranslator(loadStoredLanguage()));
      return filterDismissedAlerts(lastBuilt);
    },

    async dismissAlert(id: string): Promise<void> {
      dismissAlertLocal(id, lastBuilt);
    },
  };
}
