import type { BackupPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import type { AgentKey, BackupInspect, BackupKind, BackupMeta } from '@/lib/types';

const now = Date.now();
const h = 3600 * 1000;
const d = 24 * h;

function mk(
  id: string,
  agentId: AgentKey,
  kind: BackupKind,
  ageMs: number,
  files: string[],
  sizeMb: number,
  note?: string,
  identity?: string,
): BackupMeta {
  return {
    id,
    agentId,
    kind,
    createdAt: new Date(now - ageMs).toISOString(),
    files,
    sizeBytes: Math.round(sizeMb * 1024 * 1024),
    note,
    identity,
  };
}

const mockState: BackupMeta[] = [
  mk('bk-1', 'claude', 'auto-switch', 2 * h, ['~/.claude/settings.json', '~/.claude.json'], 0.4, '切换到 "官方" 前自动备份', 'ada@claude.test'),
  mk('bk-2', 'claude', 'manual', 2 * d, ['~/.claude/settings.json', '~/.claude.json'], 0.4, undefined, '**ANT1'),
  mk('bk-3', 'claude', 'auto-switch', 6 * d, ['~/.claude/settings.json'], 0.2, '切换到 "xx云中转" 前自动备份', 'relay.example.com'),
  mk('bk-4', 'codex', 'auto-switch', 5 * h, ['~/.codex/config.toml', '~/.codex/auth.json'], 0.1, '切换到 "xx云中转" 前自动备份', 'me@openai.test'),
  mk('bk-5', 'codex', 'manual', 9 * d, ['~/.codex/config.toml'], 0.05, undefined, '**WXYZ'),
  mk('bk-6', 'kimi', 'manual', 1 * d, ['~/.kimi-code/config.toml', '~/.kimi-code/credentials/kimi-code.json'], 0.08, undefined, 'kimi@moonshot.test'),
  mk('bk-7', 'grok', 'pre-uninstall', 12 * d, ['~/.grok/config.toml', '~/.grok/auth.json'], 0.06, '卸载前自动备份', 'user@x.ai'),
];

function mockInspect(bk: BackupMeta): BackupInspect {
  const files = bk.files.map((path) => {
    const name = path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? path;
    const isAuth = /auth|credential/i.test(name);
    const content = isAuth
      ? `{\n  "email": ${JSON.stringify(bk.identity ?? '')},\n  "refresh_token": "***"\n}\n`
      : `api_key = "***"\n`;
    return {
      name,
      source: path,
      path,
      size: Math.max(32, Math.round(bk.sizeBytes / Math.max(bk.files.length, 1))),
      content,
      facts: bk.identity
        ? [{ key: bk.identity.includes('@') ? 'email' : 'secretTail', value: bk.identity }]
        : [],
    };
  });
  return {
    id: bk.id,
    agentId: bk.agentId,
    kind: bk.kind,
    createdAt: bk.createdAt,
    size: bk.sizeBytes,
    note: bk.note,
    identity: bk.identity,
    facts: files.flatMap((file) => file.facts ?? []),
    files,
  };
}

export function createMockBackupPort(): BackupPort {
  return {
    async listBackups(agentId) {
      await delay(randomLatency());
      const all = mockState.map((b) => ({ ...b, files: [...b.files] }));
      return agentId ? all.filter((b) => b.agentId === agentId) : all;
    },

    async inspectBackup(backupId) {
      await delay(randomLatency());
      const bk = mockState.find((b) => b.id === backupId);
      if (!bk) throw new Error('备份不存在');
      return mockInspect(bk);
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
