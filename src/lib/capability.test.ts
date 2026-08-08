import { describe, expect, it } from 'vitest';

import {
  isCapabilityBlocked,
  isCapabilityUsable,
  type Capability,
  type CapabilityLevel,
} from '@/lib/capability';
import { MOCK_CAPABILITIES } from '@/dev/mocks/capabilities';
import type { AgentId } from '@/lib/types';

const ALL_CAPS: Capability[] = [
  'configWrite',
  'accountSwitch',
  'apiKeyAccount',
  'skills',
  'liveBackup',
  'structuredStream',
  'dangerousMode',
  'projectHistory',
  'projectDelete',
  'providerPresets',
  'usage',
  'mcp',
  'modelSelect',
  'sessionResume',
];

const AGENT_IDS: AgentId[] = [
  'claude',
  'codex',
  'kimi',
  'grok',
  'pi',
  'workbuddy',
  'cursor',
];

describe('isCapabilityUsable / isCapabilityBlocked', () => {
  it.each([
    ['full', true],
    ['partial', true],
    ['unsupported', false],
    ['planned', false],
  ] as [CapabilityLevel, boolean][])('level %s usable=%s', (level, usable) => {
    expect(isCapabilityUsable({ level })).toBe(usable);
    expect(isCapabilityBlocked({ level })).toBe(!usable);
  });

  it('treats missing / undefined as blocked', () => {
    expect(isCapabilityUsable(undefined)).toBe(false);
    expect(isCapabilityUsable(null)).toBe(false);
    expect(isCapabilityBlocked(undefined)).toBe(true);
  });
});

describe('MOCK_CAPABILITIES (dev/mocks)', () => {
  it('covers every catalog agent', () => {
    for (const id of AGENT_IDS) {
      expect(MOCK_CAPABILITIES[id]).toBeDefined();
    }
  });

  it('declares all 14 capability keys per agent', () => {
    for (const id of AGENT_IDS) {
      const row = MOCK_CAPABILITIES[id]!;
      for (const cap of ALL_CAPS) {
        expect(row[cap], `${id}.${cap}`).toBeDefined();
        expect(row[cap]!.level).toMatch(/^(full|partial|unsupported|planned)$/);
      }
    }
  });

  it('non-full cells carry a reason (UI/CLI copy)', () => {
    for (const id of AGENT_IDS) {
      const row = MOCK_CAPABILITIES[id]!;
      for (const cap of ALL_CAPS) {
        const cell = row[cap]!;
        if (cell.level !== 'full') {
          expect(cell.reason, `${id}.${cap} needs reason`).toBeTruthy();
        }
      }
    }
  });

  it('matches known product boundaries', () => {
    expect(MOCK_CAPABILITIES.kimi!.skills!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.workbuddy!.accountSwitch!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.cursor!.accountSwitch!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.cursor!.providerPresets!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.claude!.accountSwitch!.level).toBe('full');
  });

  it('accountSwitch blocked agents match Connections TabStrip expectations', () => {
    const disabled = AGENT_IDS.filter((id) =>
      isCapabilityBlocked(MOCK_CAPABILITIES[id]?.accountSwitch),
    );
    expect(disabled).toEqual(expect.arrayContaining(['workbuddy', 'cursor']));
    expect(disabled).not.toContain('claude');
    expect(disabled).not.toContain('kimi');
  });
});
