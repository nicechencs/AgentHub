import { describe, expect, it, vi } from 'vitest';
import { getBackend } from '@/app/runtime';
import { bindTicket, listTicketWallet, planTicket, unbindTicket } from '@/lib/api/tickets';
import { seedConnectFlowAdapterFixtures } from '@/dev/mocks/connect-flow-fixtures';

describe('tickets API façade', () => {
  it('listTicketWallet returns tickets via mock backend', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeOauthAccount: true });
    const wallet = await listTicketWallet();
    expect(wallet.tickets.length).toBeGreaterThan(0);
    expect(wallet.tickets.every((t) => t.id.includes(':'))).toBe(true);
  });

  it('listTicketWallet reuses the process snapshot until a mutation', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeOauthAccount: true });
    const listWallet = vi.spyOn(getBackend().ticket, 'listWallet');
    const first = await listTicketWallet();
    const second = await listTicketWallet();
    expect(listWallet).toHaveBeenCalledOnce();
    expect(second).toEqual(first);
  });

  it('planTicket plans via ticket id', async () => {
    getBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const plan = await planTicket(`provider:${kimiMembership.id}`, 'pi');
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('config_sync');
  });

  it('bindTicket returns the active binding and unbindTicket clears it', async () => {
    getBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const ticketId = `provider:${kimiMembership.id}`;
    const { binding } = await bindTicket(ticketId, 'pi');
    expect(binding.active).toBe(true);
    expect(binding.agentId).toBe('pi');
    expect(binding.ticketId).toBe(ticketId);
    await unbindTicket(ticketId, 'pi');
    const wallet = await listTicketWallet();
    expect(wallet.bindings.some((row) => row.ticketId === ticketId && row.agentId === 'pi')).toBe(false);
  });
});
