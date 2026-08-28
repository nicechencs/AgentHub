import type { BackupPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import type { AgentId, BackupKind, BackupMeta } from '@/lib/types';

const now = Date.now();
const h = 3600 * 1000;
const d = 24 * h;

function mk(
  id: string,
  agentId: AgentId,
  kind: BackupKind,
  ageMs: number,
  files: string[],
  sizeMb: number,
  note?: string,
): BackupMeta {
  return {
    id,
    agentId,
    kind,
    createdAt: new Date(now - ageMs).toISOString(),
    files,
    sizeBytes: Math.round(sizeMb * 1024 * 1024),
    note,
  };
}

const mockState: BackupMeta[] = [
  mk('bk-1', 'claude', 'auto-switch', 2 * h, ['~/.claude/settings.json', '~/.claude.json'], 0.4, '切换到 "官方" 前自动备份'),
  mk('bk-2', 'claude', 'manual', 2 * d, ['~/.claude/settings.json', '~/.claude.json'], 0.4),
  mk('bk-3', 'claude', 'auto-switch', 6 * d, ['~/.claude/settings.json'], 0.2, '切换到 "xx云中转" 前自动备份'),
  mk('bk-4', 'codex', 'auto-switch', 5 * h, ['~/.codex/config.toml', '~/.codex/auth.json'], 0.1, '切换到 "xx云中转" 前自动备份'),
  mk('bk-5', 'codex', 'manual', 9 * d, ['~/.codex/config.toml'], 0.05),
  mk('bk-6', 'kimi', 'manual', 1 * d, ['~/.kimi-code/config.toml', '~/.kimi-code/credentials/kimi-code.json'], 0.08),
  mk('bk-7', 'grok', 'pre-uninstall', 12 * d, ['~/.grok/config.toml', '~/.grok/auth.json'], 0.06, '卸载前自动备份'),
];

export function createMockBackupPort(): BackupPort {
  return {
    async listBackups(agentId) {
      await delay(randomLatency());
      const all = mockState.map((b) => ({ ...b, files: [...b.files] }));
      return agentId ? all.filter((b) => b.agentId === agentId) : all;
    },

    async createBackup(agentId, note) {
      await delay(500 + Math.random() * 500);
      const bk: BackupMeta = {
        id: `bk-${Date.now()}`,
        agentId,
        kind: 'manual',
        createdAt: new Date().toISOString(),
        files: [`~/.${agentId}/`],
        sizeBytes: Math.round(Math.random() * 1024 * 1024),
        note,
      };
      mockState.unshift(bk);
      return { ...bk };
    },

    async restoreBackup(backupId) {
      await delay(600 + Math.random() * 400);
      const bk = mockState.find((b) => b.id === backupId);
      if (!bk) throw new Error('备份不存在');
      mockState.unshift({
        id: `bk-${Date.now()}`,
        agentId: bk.agentId,
        kind: 'pre-restore',
        createdAt: new Date().toISOString(),
        files: bk.files,
        sizeBytes: bk.sizeBytes,
        note: '恢复前自动备份当前状态',
      });
    },

    async deleteBackup(backupId) {
      await delay(300);
      const idx = mockState.findIndex((b) => b.id === backupId);
      if (idx >= 0) mockState.splice(idx, 1);
    },
  };
}
