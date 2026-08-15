import { describe, expect, it } from 'vitest';
import { getBackend } from '@/app/runtime';
import { listTicketWallet, planTicket } from '@/lib/api/tickets';
import { seedConnectFlowAdapterFixtures } from '@/dev/mocks/connect-flow-fixtures';

describe('tickets API façade', () => {
  it('listTicketWallet returns tickets via mock backend', async () => {
    getBackend();
    seedConnectFlowAdapterFixtures({ includeOauthAccount: true });
    const wallet = await listTicketWallet();
    expect(wallet.tickets.length).toBeGreaterThan(0);
    expect(wallet.tickets.every((t) => t.id.includes(':'))).toBe(true);
  });

  it('planTicket plans via ticket id', async () => {
    getBackend();
    const { kimiMembership } = seedConnectFlowAdapterFixtures({ includeAnthropic: false });
    const plan = await planTicket(`provider:${kimiMembership.id}`, 'pi');
    expect(plan.canApply).toBe(true);
    expect(plan.analysis.route).toBe('config_sync');
  });
});
