import { describe, expect, it, beforeEach } from 'vitest';
import {
  AGENTS,
  AGENT_IDS,
  AGENT_MAP,
  applyAgentCatalog,
  agentMetaFromCatalogEntry,
  resolveAgentMeta,
  type AgentMeta,
} from '@/config/agents';
import {
  MOCK_AGENT_CATALOG,
  MOCK_UNKNOWN_DEMO_ENTRY,
  mockCatalogWithUnknownDemo,
} from '@/dev/mocks/fixtures/agent-catalog';
import { isCapabilityBlocked, isCapabilityUsable } from '@/lib/capability';
import { mergeAgentListWithCatalog } from '@/lib/backend/contracts/agent-catalog';
import type { AgentStatus } from '@/lib/types';

describe('agent catalog façade', () => {
  beforeEach(() => {
    applyAgentCatalog(MOCK_AGENT_CATALOG);
  });

  it('applyAgentCatalog rebuilds product set from backend entries only', () => {
    expect(AGENTS.map((a) => a.id)).toEqual(MOCK_AGENT_CATALOG.map((e) => e.key));
    expect(AGENTS).toHaveLength(MOCK_AGENT_CATALOG.length);
    expect(AGENT_MAP.claude?.name).toBe('Claude Code');
    expect(AGENT_MAP.claude?.installChannels.length).toBeGreaterThan(0);
    expect(AGENT_MAP.zcode?.occupancy).toBe('catalogAppend');
    expect(AGENT_MAP.workbuddy?.occupancy).toBe('catalogAppend');
    expect(AGENT_MAP.claude?.occupancy).toBe('exclusive');
    expect(AGENT_MAP.pi?.occupancy).toBe('namedSlots');
  });

  it('unknown-demo appears with fallback display after fixture-only change', () => {
    applyAgentCatalog(mockCatalogWithUnknownDemo());
    const demo = AGENTS.find((a) => a.id === 'unknown-demo');
    expect(demo).toBeDefined();
    expect(demo!.name).toBe('Unknown Demo');
    expect(demo!.color).toBe('var(--text-muted)');
    expect(demo!.letter).toBe('U');
    expect(demo!.installChannels[0]?.id).toBe('npm');
  });

  it('resolveAgentMeta falls back for keys never in catalog', () => {
    const meta = resolveAgentMeta('totally-unknown');
    expect(meta.id).toBe('totally-unknown');
    expect(meta.color).toBe('var(--text-muted)');
    expect(meta.letter).toBe('T');
  });

  it('catalog capabilities gate Planned/Unsupported as blocked', () => {
    const demo = agentMetaFromCatalogEntry(MOCK_UNKNOWN_DEMO_ENTRY);
    expect(isCapabilityBlocked(demo.capabilities?.skills)).toBe(true);
    expect(isCapabilityBlocked(demo.capabilities?.usage)).toBe(true);
    expect(isCapabilityUsable(demo.capabilities?.skills)).toBe(false);
  });

  it('mergeAgentListWithCatalog uses runtime catalog order and fills missing rows', () => {
    applyAgentCatalog(mockCatalogWithUnknownDemo());
    const detected: AgentStatus[] = [
      {
        agentId: 'claude',
        installed: true,
        authStatus: 'none',
        authLabel: '未配置',
        running: false,
      },
    ];
    const merged = mergeAgentListWithCatalog(detected, AGENTS);
    expect(merged).toHaveLength(AGENTS.length);
    expect(merged[0]?.agentId).toBe('claude');
    expect(merged[0]?.installed).toBe(true);
    const demo = merged.find((a) => a.agentId === 'unknown-demo');
    expect(demo?.installed).toBe(false);
    expect(demo?.capabilities?.skills?.level).toBe('unsupported');
  });

  it('applyAgentCatalog replaces a frozen snapshot', () => {
    expect(Object.isFrozen(AGENTS)).toBe(true);
    expect(Object.isFrozen(AGENT_MAP)).toBe(true);
    expect(Object.isFrozen(AGENT_IDS)).toBe(true);
    expect(() => {
      (AGENTS as AgentMeta[]).push(resolveAgentMeta('injected'));
    }).toThrow();
  });

  it('empty catalog does not invent agents from static list', () => {
    applyAgentCatalog([]);
    expect(AGENTS).toHaveLength(0);
    const detected: AgentStatus[] = [
      {
        agentId: 'claude',
        installed: true,
        authStatus: 'none',
        authLabel: 'x',
        running: false,
      },
    ];
    expect(mergeAgentListWithCatalog(detected, AGENTS)).toEqual(detected);
  });
});
