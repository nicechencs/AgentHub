/**
 * Build production dashboard alerts from live agent status.
 * Dismiss is local-only (session/localStorage); alerts reappear when the
 * underlying condition changes (stable id + fingerprint).
 */
import type { AgentId, AgentStatus, DashboardAlert } from '@/lib/types';
import { agentDisplayName } from '@/config/agents';
import { createTranslator, type TranslateFn } from '@/lib/i18n';
import { loadJson, saveJson } from '@/lib/ui-preferences';

const DISMISS_KEY = 'agenthub:dismissed-alerts';

type DismissMap = Record<string, string>; // id -> fingerprint when dismissed

function agentName(id: string): string {
  return agentDisplayName(id as AgentId);
}

function fingerprint(alert: Omit<DashboardAlert, 'id'> & { id: string }): string {
  return `${alert.id}|${alert.level}|${alert.actionKind}`;
}

function isUsefulConnectionLabel(value: string | undefined): boolean {
  const label = value?.trim();
  if (!label) return false;
  return label !== '未配置' && label.toLowerCase() !== 'not configured';
}

/** Pool/route already configured — do not treat live probe authStatus none as empty. */
function hasConfiguredConnection(a: AgentStatus): boolean {
  if (a.effectiveKind === 'account' || a.effectiveKind === 'api') return true;
  if (isUsefulConnectionLabel(a.effectiveLabel) || isUsefulConnectionLabel(a.currentProvider)) {
    return true;
  }
  return a.authHealth === 'configured' || a.authHealth === 'renewable' || a.authHealth === 'verified';
}

function loadDismissed(): DismissMap {
  return loadJson<DismissMap>(DISMISS_KEY, {});
}

function saveDismissed(map: DismissMap): void {
  saveJson(DISMISS_KEY, map);
}

/** Pure builder — used by dashboard port and unit tests. */
export function buildAlertsFromAgents(
  agents: AgentStatus[],
  t: TranslateFn = createTranslator('zh'),
): DashboardAlert[] {
  const out: DashboardAlert[] = [];

  for (const a of agents) {
    if (a.hidden) continue;
    const name = agentName(a.agentId);

    if (a.installed && a.authStatus === 'expired') {
      out.push({
        id: `auth-expired:${a.agentId}`,
        level: 'danger',
        message: t('dashboard.alerts.authExpired', { name }),
        actionLabel: t('dashboard.alerts.authExpiredAction'),
        actionKind: 'refresh-token',
        agentId: a.agentId,
      });
    } else if (a.installed && a.authStatus === 'expiring') {
      out.push({
        id: `auth-expiring:${a.agentId}`,
        level: 'warning',
        message: t('dashboard.alerts.authExpiring', { name }),
        actionLabel: t('dashboard.alerts.authExpiringAction'),
        actionKind: 'refresh-token',
        agentId: a.agentId,
      });
    } else if (a.installed && a.authStatus === 'none' && !hasConfiguredConnection(a)) {
      out.push({
        id: `auth-none:${a.agentId}`,
        level: 'info',
        message: t('dashboard.alerts.authNone', { name }),
        actionLabel: t('dashboard.alerts.authNoneAction'),
        actionKind: 'refresh-token',
        agentId: a.agentId,
      });
    }

    if (a.installed && a.envReady === false) {
      out.push({
        id: `env-not-ready:${a.agentId}`,
        level: 'warning',
        message: t('dashboard.alerts.envNotReady', { name }),
        actionLabel: t('dashboard.alerts.envNotReadyAction'),
        actionKind: 'upgrade',
        agentId: a.agentId,
      });
    }

    const latest = a.update?.latestVersion?.trim() || a.latestVersion?.trim();
    if (a.installed && a.update?.state === 'update_available' && latest) {
      out.push({
        id: `upgrade:${a.agentId}`,
        level: 'info',
        message: t('dashboard.alerts.upgrade', { name, version: latest }),
        actionLabel: t('dashboard.alerts.upgradeAction'),
        actionKind: 'upgrade',
        agentId: a.agentId,
      });
    }
  }

  // Stable order: danger → warning → info, then id
  const rank = { danger: 0, warning: 1, info: 2 } as const;
  out.sort((x, y) => rank[x.level] - rank[y.level] || x.id.localeCompare(y.id));
  return out;
}

export function filterDismissedAlerts(alerts: DashboardAlert[]): DashboardAlert[] {
  const dismissed = loadDismissed();
  return alerts.filter((a) => {
    const fp = fingerprint(a);
    return dismissed[a.id] !== fp;
  });
}

export function dismissAlertLocal(id: string, alerts: DashboardAlert[]): void {
  const hit = alerts.find((a) => a.id === id);
  if (!hit) {
    // Still mark id so a race doesn't re-show immediately without fingerprint.
    const map = loadDismissed();
    map[id] = map[id] ?? '';
    saveDismissed(map);
    return;
  }
  const map = loadDismissed();
  map[id] = fingerprint(hit);
  saveDismissed(map);
}

/** Test helper */
export function __resetDismissedAlertsForTests(): void {
  saveDismissed({});
}
