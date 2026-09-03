import { describe, expect, it, vi } from 'vitest';
import type { AdapterApplyPlan } from '@/lib/backend/contracts/adapter';
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { BindTicketResult } from '@/lib/backend/contracts/ticket';
import { importLocalTokenToAgent, type ImportLocalTokenDeps } from './token-import-action';

function profile(partial: Partial<AdapterProfile> = {}): AdapterProfile {
  return {
    id: 'bridge-claude',
    name: 'Claude bridge',
    sourceKind: 'provider',
    sourceId: 'src-1',
    targetAgentId: 'claude',
    route: 'local_bridge',
    mode: 'api',
    status: 'active',
    ruleId: 'rule',
    ruleVersion: '1',
    generatedProviderId: 'gen-claude',
    autoStart: true,
    createdAt: '',
    updatedAt: '',
    ...partial,
  };
}

function planMock(): ImportLocalTokenDeps['planTicket'] {
  return vi.fn(async () => ({
    analysis: { route: 'local_bridge' },
    targetAgentId: 'claude',
    canApply: true,
    serviceImpact: [],
    changes: [],
  }) as unknown as AdapterApplyPlan);
}

function bindResult(
  partial: Partial<BindTicketResult['binding']> = {},
): BindTicketResult {
  return {
    binding: {
      ticketId: 'provider:src-1',
      agentId: 'claude',
      route: 'bridge',
      active: true,
      profileId: 'bridge-claude',
      bridge: null,
      ...partial,
    },
  };
}

describe('importLocalTokenToAgent', () => {
  it('switches immediately when the local-gateway provider is already on this Agent', async () => {
    const planTicket = planMock();
    const bindTicket = vi.fn(async () => bindResult());
    const switchProvider = vi.fn(async () => undefined);
    const logGuiEvent = vi.fn(async () => undefined);

    await importLocalTokenToAgent(
      {
        profile: profile(),
        agentId: 'claude',
        localToken: 'ahb_xxxxxxxxxxxxABCD',
      },
      {
        planTicket,
        bindTicket,
        listAdapterProfiles: vi.fn(async () => []),
        switchProvider,
        logGuiEvent,
      },
    );

    expect(planTicket).not.toHaveBeenCalled();
    expect(bindTicket).not.toHaveBeenCalled();
    expect(switchProvider).toHaveBeenCalledWith('claude', 'gen-claude');
    expect(logGuiEvent).toHaveBeenCalledWith('switch_write', {
      agent: 'claude',
      last4: 'ABCD',
    });
  });

  it('plans, binds, then switchProvider when the generated provider is missing', async () => {
    const planTicket = planMock();
    const bindTicket = vi.fn(async () => bindResult());
    const listAdapterProfiles = vi.fn(async () => [profile()]);
    const switchProvider = vi.fn(async () => undefined);

    await importLocalTokenToAgent(
      {
        profile: profile({ generatedProviderId: null }),
        agentId: 'claude',
        siblingProfiles: [profile({ generatedProviderId: null })],
      },
      {
        planTicket,
        bindTicket,
        listAdapterProfiles,
        switchProvider,
        logGuiEvent: vi.fn(async () => undefined),
      },
    );

    expect(planTicket).toHaveBeenCalledWith('provider:src-1', 'claude');
    expect(bindTicket).toHaveBeenCalledWith('provider:src-1', 'claude');
    expect(switchProvider).toHaveBeenCalledWith('claude', 'gen-claude');
  });

  it('uses a sibling generatedProviderId without binding again', async () => {
    const planTicket = planMock();
    const bindTicket = vi.fn(async () => bindResult({ profileId: null }));
    const switchProvider = vi.fn(async () => undefined);
    await importLocalTokenToAgent(
      {
        profile: profile({ generatedProviderId: null }),
        agentId: 'claude',
        siblingProfiles: [profile({ generatedProviderId: 'gen-from-sibling' })],
      },
      {
        planTicket,
        bindTicket,
        listAdapterProfiles: vi.fn(async () => { throw new Error('offline'); }),
        switchProvider,
        logGuiEvent: vi.fn(async () => undefined),
      },
    );
    expect(planTicket).not.toHaveBeenCalled();
    expect(bindTicket).not.toHaveBeenCalled();
    expect(switchProvider).toHaveBeenCalledWith('claude', 'gen-from-sibling');
  });

  it('throws when no generated provider is available', async () => {
    await expect(importLocalTokenToAgent(
      {
        profile: profile({ generatedProviderId: null }),
        agentId: 'pi',
      },
      {
        planTicket: planMock(),
        bindTicket: vi.fn(async () => bindResult({
          agentId: 'pi',
          profileId: null,
        })),
        listAdapterProfiles: vi.fn(async () => []),
        switchProvider: vi.fn(async () => undefined),
        logGuiEvent: vi.fn(async () => undefined),
      },
    )).rejects.toThrow('找不到写入目标工具的本机地址');
  });
});
