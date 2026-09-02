import { describe, expect, it } from 'vitest';
import {
  agentMatchesTokenSurface,
  eligibleAgentsForTokenImport,
  isTokenImportAgentVisible,
  tokenImportGate,
  tokenImportSurface,
} from './token-import-model';
import type { LocalTokenRow } from './tokens-model';

function row(
  partial: Partial<LocalTokenRow> = {},
): Pick<LocalTokenRow, 'kind' | 'token' | 'profileId' | 'unavailable'> {
  return {
    kind: 'messages',
    token: 'ahb_secret',
    profileId: 'profile-1',
    unavailable: false,
    ...partial,
  };
}

describe('tokenImportSurface', () => {
  it('maps LocalTokenRow.kind onto wire conversation surfaces', () => {
    expect(tokenImportSurface('messages')).toBe('messages');
    expect(tokenImportSurface('responses_codex')).toBe('responses');
    expect(tokenImportSurface('responses_grok')).toBe('responses');
    expect(tokenImportSurface('chat_completions')).toBe('chat_completions');
  });
});

describe('agentMatchesTokenSurface', () => {
  it('matches Agents that speak the token surface', () => {
    expect(agentMatchesTokenSurface('claude', 'messages')).toBe(true);
    expect(agentMatchesTokenSurface('codex', 'messages')).toBe(false);
    expect(agentMatchesTokenSurface('codex', 'responses_codex')).toBe(true);
    expect(agentMatchesTokenSurface('grok', 'responses_grok')).toBe(true);
    expect(agentMatchesTokenSurface('workbuddy', 'chat_completions')).toBe(true);
    expect(agentMatchesTokenSurface('workbuddy', 'messages')).toBe(false);
  });

  it('lets multi-surface Agents match any of the three kinds', () => {
    for (const agent of ['pi', 'zcode', 'kimi', 'dsh'] as const) {
      expect(agentMatchesTokenSurface(agent, 'messages')).toBe(true);
      expect(agentMatchesTokenSurface(agent, 'responses_codex')).toBe(true);
      expect(agentMatchesTokenSurface(agent, 'chat_completions')).toBe(true);
    }
  });

  it('rejects Cursor Agent (no public HTTP surface)', () => {
    expect(agentMatchesTokenSurface('cursor', 'messages')).toBe(false);
    expect(agentMatchesTokenSurface('cursor', 'responses_codex')).toBe(false);
    expect(agentMatchesTokenSurface('cursor', 'chat_completions')).toBe(false);
  });
});

describe('isTokenImportAgentVisible', () => {
  it('requires installed && !hidden', () => {
    expect(isTokenImportAgentVisible({ installed: true, hidden: false })).toBe(true);
    expect(isTokenImportAgentVisible({ installed: true, hidden: true })).toBe(false);
    expect(isTokenImportAgentVisible({ installed: false, hidden: false })).toBe(false);
    expect(isTokenImportAgentVisible(null)).toBe(false);
  });
});

describe('eligibleAgentsForTokenImport', () => {
  it('keeps installed order and drops surface mismatches', () => {
    const agents = eligibleAgentsForTokenImport({
      kind: 'messages',
      installedIds: ['codex', 'claude', 'pi', 'cursor'],
      agentName: (id) => id.toUpperCase(),
    });
    expect(agents.map((a) => a.id)).toEqual(['claude', 'pi']);
    expect(agents[0]?.name).toBe('CLAUDE');
  });

  it('builds installed ids from statuses when installedIds omitted', () => {
    const agents = eligibleAgentsForTokenImport({
      kind: 'responses_codex',
      statuses: [
        { agentId: 'claude', installed: true, hidden: false },
        { agentId: 'codex', installed: true, hidden: false },
        { agentId: 'grok', installed: true, hidden: true },
        { agentId: 'pi', installed: false, hidden: false },
      ],
    });
    expect(agents.map((a) => a.id)).toEqual(['codex']);
  });

  it('returns empty for chat_completions when only Claude/Codex/Grok are installed', () => {
    expect(eligibleAgentsForTokenImport({
      kind: 'chat_completions',
      installedIds: ['claude', 'codex', 'grok'],
    })).toEqual([]);
  });

  it('includes WorkBuddy for chat_completions when installed', () => {
    expect(eligibleAgentsForTokenImport({
      kind: 'chat_completions',
      installedIds: ['claude', 'workbuddy', 'kimi'],
    }).map((a) => a.id)).toEqual(['workbuddy', 'kimi']);
  });
});

describe('tokenImportGate', () => {
  const eligible = [
    { id: 'claude' as const, name: 'Claude' },
    { id: 'pi' as const, name: 'Pi' },
  ];

  it('enables when key, profile, and eligible Agents are ready', () => {
    const gate = tokenImportGate(row(), eligible);
    expect(gate.enabled).toBe(true);
    expect(gate.reason).toBeNull();
    expect(gate.agents.map((a) => a.id)).toEqual(['claude', 'pi']);
  });

  it('disables with a short hint when nobody is eligible (no empty menu)', () => {
    const gate = tokenImportGate(
      row({ kind: 'chat_completions' }),
      [{ id: 'claude', name: 'Claude' }, { id: 'codex', name: 'Codex' }],
    );
    expect(gate.enabled).toBe(false);
    expect(gate.agents).toEqual([]);
    expect(gate.reason).toBe('没有已安装且匹配此端点的 Agent');
  });

  it('disables without a key or profile', () => {
    expect(tokenImportGate(row({ token: null }), eligible).reason).toBe('先有入口 Key 才能导入');
    expect(tokenImportGate(row({ profileId: null }), eligible).reason).toBe('本机入口还没就绪');
    expect(tokenImportGate(row({ unavailable: true }), eligible).reason).toBe('状态不可用');
  });

  it('filters the passed agent list to surface matches', () => {
    const gate = tokenImportGate(
      row({ kind: 'responses_codex' }),
      [
        { id: 'claude', name: 'Claude' },
        { id: 'codex', name: 'Codex' },
        { id: 'pi', name: 'Pi' },
      ],
    );
    expect(gate.agents.map((a) => a.id)).toEqual(['codex', 'pi']);
    expect(gate.enabled).toBe(true);
  });
});
