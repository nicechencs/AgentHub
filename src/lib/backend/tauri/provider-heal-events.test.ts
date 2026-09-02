import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PROVIDER_BINDING_HEAL_EVENT } from '@/lib/backend/contracts/provider-heal-types';

const { isTauriMock, listenMock } = vi.hoisted(() => ({
  isTauriMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock('@/lib/platform', () => ({ isTauriApp: isTauriMock }));
vi.mock('@tauri-apps/api/event', () => ({ listen: listenMock }));

async function loadEvents() {
  return import('./provider-heal-events');
}

describe('tauri provider heal events', () => {
  beforeEach(() => {
    isTauriMock.mockReset();
    listenMock.mockReset();
    vi.resetModules();
  });

  it('fails closed outside Tauri', async () => {
    isTauriMock.mockReturnValue(false);
    const { onProviderBindingHeal, startProviderBindingHealListen } = await loadEvents();
    await expect(onProviderBindingHeal(() => {})).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
    await expect(startProviderBindingHealListen()).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
  });

  it('surfaces listener initialization errors', async () => {
    isTauriMock.mockReturnValue(true);
    listenMock.mockRejectedValue(new Error('listen failed'));
    const { onProviderBindingHeal } = await loadEvents();
    await expect(onProviderBindingHeal(() => {})).rejects.toMatchObject({
      code: 'backend.unavailable',
    });
  });

  it('subscribes to provider-binding-heal and forwards healed/conflict payloads', async () => {
    isTauriMock.mockReturnValue(true);
    let listener: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (event: string, handler: (event: { payload: unknown }) => void) => {
      expect(event).toBe(PROVIDER_BINDING_HEAL_EVENT);
      listener = handler;
      return () => {};
    });

    const { onProviderBindingHeal } = await loadEvents();
    const seen: string[] = [];
    await onProviderBindingHeal((payload) => {
      seen.push(`${payload.kind}:${payload.agent}`);
    });

    listener?.({ payload: { kind: 'healed', agent: 'claude', fromName: 'OpenAI', toName: 'Codex' } });
    listener?.({ payload: { kind: 'conflict', agent: 'claude' } });
    listener?.({ payload: { kind: 'healed' } });
    listener?.({ payload: null });

    expect(seen).toEqual(['healed:claude', 'conflict:claude']);
  });
});
