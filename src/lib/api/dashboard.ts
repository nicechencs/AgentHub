/**
 * Dashboard alerts façade.
 * Production: derived from doctor agent status (auth / env / updates); dismiss is local.
 * Mock: demo alerts under dev:mock.
 */
import { getBackend } from '@/app/runtime';
import type { DashboardAlert } from '@/lib/types';

export async function listAlerts(): Promise<DashboardAlert[]> {
  return getBackend().dashboard.listAlerts();
}

export async function dismissAlert(id: string): Promise<void> {
  return getBackend().dashboard.dismissAlert(id);
}
