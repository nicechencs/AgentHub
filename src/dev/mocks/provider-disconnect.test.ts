import { afterEach, describe, expect, it } from 'vitest';
import type { Provider } from '@/lib/types';
import {
  createMockProviderPort,
  getMockProviderById,
  resetMockProviders,
  upsertMockProvider,
} from './provider';

const piProvider: Provider = {
  id: 'pi-provider-1',
  agentId: 'pi',
  name: 'Pi provider',
  preset: 'custom',
  configText: '{}',
  configFormat: 'json',
  isCurrent: true,
};

describe('mock disconnectPiProvider', () => {
  afterEach(() => resetMockProviders());

  it('clears the current Pi provider without deleting its pool row', async () => {
    upsertMockProvider(piProvider);

    await createMockProviderPort().disconnectPiProvider(piProvider.id, false);

    expect(getMockProviderById(piProvider.id)).toMatchObject({
      id: piProvider.id,
      agentId: 'pi',
      isCurrent: false,
    });
  });

  it('deletes the Pi provider when requested', async () => {
    upsertMockProvider(piProvider);

    await createMockProviderPort().disconnectPiProvider(piProvider.id, true);

    expect(getMockProviderById(piProvider.id)).toBeUndefined();
  });
});
