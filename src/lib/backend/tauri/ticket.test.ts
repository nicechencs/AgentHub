import { beforeEach, describe, expect, it, vi } from 'vitest';
import { createTauriTicketPort } from './ticket';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('Tauri ticket port', () => {
  it('forwards bind_ticket { ticketId, targetAgentId } and maps binding', async () => {
    invokeMock.mockResolvedValueOnce({
      binding: {
        ticketId: 'account:anth-1',
        agentId: 'pi',
        route: 'reshape',
        active: true,
        profileId: 'prof-1',
        bridge: null,
      },
    });
    const port = createTauriTicketPort();
    const result = await port.bind('account:anth-1', 'pi');
    expect(invokeMock).toHaveBeenCalledWith('bind_ticket', {
      ticketId: 'account:anth-1',
      targetAgentId: 'pi',
    });
    expect(result.binding).toMatchObject({
      ticketId: 'account:anth-1',
      agentId: 'pi',
      route: 'reshape',
      active: true,
      profileId: 'prof-1',
    });
  });

  it('forwards unbind_ticket { ticketId, agentId } and accepts empty result', async () => {
    invokeMock.mockResolvedValueOnce({});
    const port = createTauriTicketPort();
    await expect(port.unbind('provider:kimi-1', 'claude')).resolves.toBeUndefined();
    expect(invokeMock).toHaveBeenCalledWith('unbind_ticket', {
      ticketId: 'provider:kimi-1',
      agentId: 'claude',
    });
  });
});
