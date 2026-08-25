import { describe, expect, it, vi } from 'vitest';
import { requestRemoteModels } from './remote-models-request';

describe('requestRemoteModels', () => {
  it('uses the live API key for an unsaved provider', async () => {
    const listRemoteOpenAiModels = vi.fn(async () => ['gpt-4o']);
    const listRemoteOpenAiModelsForProvider = vi.fn(async () => ['saved-model']);

    await expect(
      requestRemoteModels(
        {
          baseUrl: 'https://relay.example/v1',
          apiKey: 'sk-live-abcdefgh',
        },
        { listRemoteOpenAiModels, listRemoteOpenAiModelsForProvider },
      ),
    ).resolves.toEqual(['gpt-4o']);

    expect(listRemoteOpenAiModels).toHaveBeenCalledWith(
      'https://relay.example/v1',
      'sk-live-abcdefgh',
    );
    expect(listRemoteOpenAiModelsForProvider).not.toHaveBeenCalled();
  });

  it('uses the stored provider secret when editing an existing provider', async () => {
    const listRemoteOpenAiModels = vi.fn(async () => ['live-model']);
    const listRemoteOpenAiModelsForProvider = vi.fn(async () => ['saved-model']);

    await expect(
      requestRemoteModels(
        {
          baseUrl: 'https://relay.example/v1',
          apiKey: '***',
          providerId: 'provider-1',
        },
        { listRemoteOpenAiModels, listRemoteOpenAiModelsForProvider },
      ),
    ).resolves.toEqual(['saved-model']);

    expect(listRemoteOpenAiModels).not.toHaveBeenCalled();
    expect(listRemoteOpenAiModelsForProvider).toHaveBeenCalledWith(
      'provider-1',
      'https://relay.example/v1',
    );
  });

  it('fails closed when no live key or stored provider is available', async () => {
    await expect(
      requestRemoteModels(
        { baseUrl: 'https://relay.example/v1', apiKey: '' },
        {
          listRemoteOpenAiModels: vi.fn(),
          listRemoteOpenAiModelsForProvider: vi.fn(),
        },
      ),
    ).rejects.toThrow('no stored secret');
  });
});
