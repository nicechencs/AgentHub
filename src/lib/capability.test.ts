import { describe, expect, it } from 'vitest';

import {
  isCapabilityBlocked,
  isCapabilityUsable,
  providerCapabilityGate,
  type Capability,
  type CapabilityLevel,
} from '@/lib/capability';
import { createTranslator } from '@/lib/i18n';
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
  'dsh',
  'zcode',
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

describe('providerCapabilityGate', () => {
  it('allows Provider/API Key management and switching when both contracts are usable', () => {
    expect(
      providerCapabilityGate({
        configWrite: { level: 'full' },
        providerPresets: { level: 'partial' },
      }),
    ).toEqual({ canManage: true, canSwitch: true, canUsePresets: true });
  });

  it('blocks add/edit and switching when configWrite is unsupported', () => {
    const gate = providerCapabilityGate({
      configWrite: { level: 'unsupported', reason: 'no live writer' },
      providerPresets: { level: 'full' },
    });
    expect(gate).toMatchObject({
      canManage: false,
      canSwitch: false,
      canUsePresets: true,
      reason: 'no live writer',
    });
  });

  it('keeps custom Provider management usable when built-in presets are unsupported', () => {
    const gate = providerCapabilityGate({
      configWrite: { level: 'full' },
      providerPresets: { level: 'unsupported', reason: 'no presets' },
    });
    expect(gate).toEqual({ canManage: true, canSwitch: true, canUsePresets: false });
  });

  it('fails closed when capability data is missing', () => {
    expect(providerCapabilityGate()).toMatchObject({
      canManage: false,
      canSwitch: false,
      canUsePresets: false,
    });
  });

  it('falls back to an English default reason when no translator is passed', () => {
    const gate = providerCapabilityGate({ configWrite: { level: 'unsupported' } });
    expect(gate.reason).toBe('This agent does not support config writes');
  });

  it('translates the default reason via t when configWrite carries no explicit reason', () => {
    const tZh = createTranslator('zh');
    const tEn = createTranslator('en');
    expect(
      providerCapabilityGate({ configWrite: { level: 'unsupported' } }, tZh).reason,
    ).toBe('该 Agent 不支持配置写入');
    expect(
      providerCapabilityGate({ configWrite: { level: 'unsupported' } }, tEn).reason,
    ).toBe('This agent does not support config writes');
  });

  it('prefers the explicit configWrite.reason over the translator default', () => {
    const tZh = createTranslator('zh');
    const gate = providerCapabilityGate(
      { configWrite: { level: 'unsupported', reason: 'no live writer' } },
      tZh,
    );
    expect(gate.reason).toBe('no live writer');
  });

  it('allows the current Pi and WorkBuddy provider controls without affecting account capability', () => {
    for (const id of ['pi', 'workbuddy'] as const) {
      const gate = providerCapabilityGate(MOCK_CAPABILITIES[id]);
      expect(gate.canManage, id).toBe(true);
      expect(gate.canSwitch, id).toBe(true);
      expect(MOCK_CAPABILITIES[id]!.accountSwitch!.level).toBe(
        id === 'pi' ? 'full' : 'partial',
      );
    }
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
    expect(MOCK_CAPABILITIES.kimi!.skills!.level).toBe('partial');
    expect(MOCK_CAPABILITIES.workbuddy!.accountSwitch!.level).toBe('partial');
    expect(MOCK_CAPABILITIES.cursor!.accountSwitch!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.cursor!.providerPresets!.level).toBe('unsupported');
    expect(MOCK_CAPABILITIES.claude!.accountSwitch!.level).toBe('full');
    expect(MOCK_CAPABILITIES.dsh!.apiKeyAccount!.level).toBe('full');
    expect(MOCK_CAPABILITIES.dsh!.usage!.level).toBe('full');
    expect(MOCK_CAPABILITIES.dsh!.structuredStream!.level).toBe('planned');
    expect(MOCK_CAPABILITIES.dsh!.configWrite!.level).toBe('partial');
  });

  it('accountSwitch blocked agents match Connections TabStrip expectations', () => {
    const disabled = AGENT_IDS.filter((id) =>
      isCapabilityBlocked(MOCK_CAPABILITIES[id]?.accountSwitch),
    );
    expect(disabled).toEqual(expect.arrayContaining(['cursor']));
    expect(disabled).not.toContain('workbuddy');
    expect(disabled).not.toContain('claude');
    expect(disabled).not.toContain('kimi');
  });
});
