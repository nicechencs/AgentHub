import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { ConnectionTrashItem } from '@/lib/backend/contracts';
import type { Account, Provider } from '@/lib/types';
import { dedupTrashItems, humanizeTrashLabel } from './connection-trash-model';

const SECRET = 'sk-ant-secret-do-not-leak';
const LOOPBACK_CONFIG = JSON.stringify({
  env: {
    ANTHROPIC_BASE_URL: 'http://127.0.0.1:32123',
    ANTHROPIC_AUTH_TOKEN: SECRET,
  },
});

function acc(partial: Partial<Account> & Pick<Account, 'id' | 'kind' | 'label'>): Account {
  return {
    agentId: 'grok',
    isCurrent: false,
    tokenValid: true,
    ...partial,
  };
}

function prov(partial: Partial<Provider> & Pick<Provider, 'id' | 'name'>): Provider {
  return {
    agentId: 'claude',
    preset: 'custom',
    configText: LOOPBACK_CONFIG,
    configFormat: 'json',
    isCurrent: false,
    ...partial,
  };
}

function trash(
  partial: Partial<ConnectionTrashItem> & Pick<ConnectionTrashItem, 'id' | 'sourceId' | 'label'>,
): ConnectionTrashItem {
  return {
    agentId: 'claude',
    kind: 'provider',
    wasCurrent: false,
    deletedAt: '2026-08-18T12:00:00.000Z',
    expiresAt: '2026-09-17T12:00:00.000Z',
    ...partial,
  };
}

function assertNoInternalLeak(label: string): void {
  expect(label).not.toMatch(/Grok Subscription Bridge|Codex Subscription Bridge|Kimi Code Bridge|Anthropic Bridge/i);
  expect(label).not.toMatch(/bridge/i);
  expect(label).not.toMatch(/grok-live-/i);
  expect(label).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i);
  expect(label).not.toMatch(/127\.0\.0\.1|localhost|::1/i);
  expect(label).not.toContain(SECRET);
  expect(label).not.toMatch(/configText|credentialSummary|ANTHROPIC_AUTH_TOKEN/i);
}

describe('humanizeTrashLabel', () => {
  it('renders Grok Subscription Bridge + grok-live-* as 本机路由', () => {
    const item = trash({
      id: 't-bridge',
      sourceId: 'claude-grok-adapter-bridge-grok-live-452e70db-ffff',
      label: 'Grok Subscription Bridge',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-452e70db-ffff',
        name: 'Grok Subscription Bridge',
      }),
    });
    const label = humanizeTrashLabel(item);
    expect(label).toBe('本机路由');
    assertNoInternalLeak(label);
  });

  it('appends account email when a generated row has one', () => {
    const item = trash({
      id: 't-email',
      kind: 'account',
      agentId: 'grok',
      sourceId: 'grok-live-452e70db-ffff',
      label: 'Grok Subscription Bridge',
      account: acc({
        id: 'grok-live-452e70db-ffff',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
    });
    expect(humanizeTrashLabel(item)).toBe('本机路由 · user@x.ai');
    expect(humanizeTrashLabel(item, createTranslator('en'))).toBe('Local route · user@x.ai');
  });

  it('never prints a raw uuid or grok-live id used as the label', () => {
    const uuid = '550e8400-e29b-41d4-a716-446655440000';
    const uuidLabel = humanizeTrashLabel(
      trash({ id: 't-uuid', sourceId: uuid, label: uuid }),
    );
    expect(uuidLabel).toBe('本机路由');
    expect(uuidLabel).not.toContain(uuid);

    const liveId = 'grok-live-452e70db-ffff';
    const liveLabel = humanizeTrashLabel(
      trash({ id: 't-live', sourceId: liveId, label: liveId }),
    );
    expect(liveLabel).toBe('本机路由');
    expect(liveLabel).not.toContain(liveId);
  });

  it('keeps a custom relay name like xx云中转', () => {
    const item = trash({
      id: 't-custom',
      sourceId: 'p-123',
      label: 'xx云中转',
      provider: prov({
        id: 'p-123',
        name: 'xx云中转',
        configText: JSON.stringify({
          env: {
            ANTHROPIC_BASE_URL: 'https://relay.example.com',
            ANTHROPIC_AUTH_TOKEN: '***',
          },
        }),
      }),
    });
    expect(humanizeTrashLabel(item)).toBe('xx云中转');
  });

  it('does not leak configText secrets in the label', () => {
    const item = trash({
      id: 't-secret',
      sourceId: 'claude-grok-adapter-bridge-grok-live-xxx',
      label: 'Grok Subscription Bridge',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-xxx',
        name: 'Grok Subscription Bridge',
      }),
      account: acc({
        id: 'grok-live-xxx',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
        credentialSummary: `token=${SECRET}`,
      }),
    });
    const label = humanizeTrashLabel(item);
    expect(label).toBe('本机路由 · user@x.ai');
    assertNoInternalLeak(label);
  });
});

describe('dedupTrashItems', () => {
  it('keeps one row for the same sourceId, preferring newest deletedAt', () => {
    const older = trash({
      id: 'old-id',
      sourceId: 'p-same',
      label: 'xx云中转',
      deletedAt: '2026-08-01T00:00:00.000Z',
    });
    const newer = trash({
      id: 'new-id',
      sourceId: 'p-same',
      label: 'xx云中转',
      deletedAt: '2026-08-18T00:00:00.000Z',
    });
    const kept = dedupTrashItems([older, newer]);
    expect(kept).toHaveLength(1);
    expect(kept[0].id).toBe('new-id');
    expect(kept[0].deletedAt).toBe('2026-08-18T00:00:00.000Z');
  });

  it('dedups the same (agentId, kind, sourceId)', () => {
    const older = trash({
      id: 'triple-old',
      agentId: 'claude',
      kind: 'provider',
      sourceId: 'p-triple',
      label: 'xx云中转',
      deletedAt: '2026-08-02T00:00:00.000Z',
    });
    const newer = trash({
      id: 'triple-new',
      agentId: 'claude',
      kind: 'provider',
      sourceId: 'p-triple',
      label: 'xx云中转',
      deletedAt: '2026-08-19T00:00:00.000Z',
    });
    const kept = dedupTrashItems([newer, older]);
    expect(kept).toHaveLength(1);
    expect(kept[0].id).toBe('triple-new');
  });

  it('collapses account grok-live-xxx with a generated provider that contains it', () => {
    const accountRow = trash({
      id: 'acc-trash',
      agentId: 'grok',
      kind: 'account',
      sourceId: 'grok-live-xxx',
      label: 'user@x.ai',
      deletedAt: '2026-08-10T00:00:00.000Z',
      account: acc({
        id: 'grok-live-xxx',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
    });
    const providerRow = trash({
      id: 'prov-trash',
      agentId: 'claude',
      kind: 'provider',
      sourceId: 'claude-grok-adapter-bridge-grok-live-xxx-0123456789abcdef',
      label: 'Grok Subscription Bridge',
      deletedAt: '2026-08-18T00:00:00.000Z',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-xxx-0123456789abcdef',
        name: 'Grok Subscription Bridge',
      }),
    });
    const kept = dedupTrashItems([accountRow, providerRow]);
    expect(kept).toHaveLength(1);
    expect(kept[0].id).toBe('prov-trash');
  });

  it('exposes the kept row id for restore/delete', () => {
    const older = trash({
      id: 'restore-old',
      sourceId: 'grok-live-keep',
      kind: 'account',
      agentId: 'grok',
      label: 'user@x.ai',
      deletedAt: '2026-08-01T00:00:00.000Z',
      account: acc({
        id: 'grok-live-keep',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
    });
    const newer = trash({
      id: 'restore-new',
      sourceId: 'claude-grok-adapter-bridge-grok-live-keep',
      label: 'Grok Subscription Bridge',
      deletedAt: '2026-08-20T00:00:00.000Z',
      provider: prov({
        id: 'claude-grok-adapter-bridge-grok-live-keep',
        name: 'Grok Subscription Bridge',
      }),
    });
    const kept = dedupTrashItems([older, newer]);
    expect(kept).toHaveLength(1);
    expect(kept[0].id).toBe('restore-new');
  });

  it('does not collapse the same email across agents without a shared grok-live id', () => {
    const grokAccount = trash({
      id: 'grok-acc-1-trash',
      agentId: 'grok',
      kind: 'account',
      sourceId: 'grok-acc-1',
      label: 'user@x.ai',
      deletedAt: '2026-08-10T00:00:00.000Z',
      account: acc({
        id: 'grok-acc-1',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
      // Generated (bridge name) so the old unscoped email: key would have applied.
      provider: prov({
        id: 'grok-side-adapter',
        name: 'Grok Subscription Bridge',
      }),
    });
    const claudeProvider = trash({
      id: 'claude-prov-1-trash',
      agentId: 'claude',
      kind: 'provider',
      sourceId: 'claude-prov-1',
      label: 'user@x.ai',
      deletedAt: '2026-08-18T00:00:00.000Z',
      provider: prov({
        id: 'claude-prov-1',
        name: 'user@x.ai',
      }),
      account: acc({
        id: 'claude-side-email',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
    });
    const kept = dedupTrashItems([grokAccount, claudeProvider]);
    expect(kept).toHaveLength(2);
    const ids = kept.map((row) => row.id);
    expect(ids).toEqual(expect.arrayContaining(['grok-acc-1-trash', 'claude-prov-1-trash']));
  });

  it('exposes both row ids for restore/delete when only the email matches', () => {
    const grokAccount = trash({
      id: 'restore-grok-acc',
      agentId: 'grok',
      kind: 'account',
      sourceId: 'grok-acc-1',
      label: 'user@x.ai',
      deletedAt: '2026-08-01T00:00:00.000Z',
      account: acc({
        id: 'grok-acc-1',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
      provider: prov({
        id: 'grok-side-adapter',
        name: 'Grok Subscription Bridge',
      }),
    });
    const claudeProvider = trash({
      id: 'restore-claude-prov',
      agentId: 'claude',
      kind: 'provider',
      sourceId: 'claude-prov-1',
      label: 'user@x.ai',
      deletedAt: '2026-08-20T00:00:00.000Z',
      provider: prov({
        id: 'claude-prov-1',
        name: 'user@x.ai',
      }),
      account: acc({
        id: 'claude-side-email',
        kind: 'oauth',
        label: 'user@x.ai',
        email: 'user@x.ai',
      }),
    });
    const kept = dedupTrashItems([grokAccount, claudeProvider]);
    expect(kept).toHaveLength(2);
    expect(kept.map((row) => row.id).sort()).toEqual(['restore-claude-prov', 'restore-grok-acc']);
  });
});
