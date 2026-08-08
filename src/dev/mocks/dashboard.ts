import type { DashboardPort } from '@/lib/backend/contracts';
import { delay } from '@/dev/mocks/delay';
import type { DashboardAlert } from '@/lib/types';

let alerts: DashboardAlert[] = [
  {
    id: 'alert-1',
    level: 'danger',
    message: 'Grok 账号 token 将于 2 小时后过期',
    actionLabel: '立即刷新',
    actionKind: 'refresh-token',
    agentId: 'grok',
  },
  {
    id: 'alert-2',
    level: 'warning',
    message: 'Codex 配置被外部修改,与当前供应商不一致',
    actionLabel: '查看差异',
    actionKind: 'view-diff',
    agentId: 'codex',
  },
  {
    id: 'alert-3',
    level: 'info',
    message: 'Claude Code 可升级到 v2.2.0',
    actionLabel: '去升级',
    actionKind: 'upgrade',
    agentId: 'claude',
  },
  {
    id: 'alert-4',
    level: 'warning',
    message: 'Grok 上次备份已是 12 天前',
    actionLabel: '立即备份',
    actionKind: 'backup-now',
    agentId: 'grok',
  },
];

export function createMockDashboardPort(): DashboardPort {
  return {
    async listAlerts() {
      await delay(200 + Math.random() * 300);
      return [...alerts];
    },

    async dismissAlert(id) {
      await delay(200);
      alerts = alerts.filter((a) => a.id !== id);
    },
  };
}
